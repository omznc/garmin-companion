//! Coaching chat over an OpenAI-compatible endpoint.
//!
//! Two providers, both speaking the same wire format: OpenRouter (hosted, needs
//! the user's own key) and Ollama (local, needs nothing). The key lives in the
//! OS keyring; the chosen provider and model live in the cache's key-value
//! table alongside the sync state.
//!
//! The model answers by calling tools that read the local SQLite cache. It
//! never receives the cache wholesale, and it never gets a network path to
//! Garmin — the only data that leaves this machine is whatever a tool returned
//! for the question actually asked.

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use garmin_core::{db::Db, query, store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Runtime};

const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";
const OLLAMA_BASE: &str = "http://localhost:11434/v1";

/// Cap on tool round trips in one turn. Every tool here reads a bounded slice
/// of a local table, so a model that keeps calling them is looping, not working.
const MAX_TOOL_ROUNDS: usize = 6;

const SYSTEM_PROMPT: &str = "\
You are a running and training coach with direct read access to this athlete's \
Garmin history, held in a local cache on their machine.

Answer from the tools, not from general knowledge. Call a tool before making \
any claim about a number. If a tool returns nothing, say so — never estimate a \
figure to fill a gap.

What this athlete cares about, in order:
1. Heart-rate zone distribution per session, in minutes and percent.
2. Drift back into hard efforts (Z4/Z5) across recent runs.
3. Distance, pace, average and max HR, cadence, training effect, compared with \
   recent sessions.
4. Recovery: resting HR trend, HRV, training readiness, sleep.

Zone numbers are the athlete's own Garmin configuration. Treat Z1+Z2 as easy \
and Z3-Z5 as hard. An activity with `has_hr_data: false` recorded no heart \
rate at all — that is not an easy session, so exclude it rather than counting \
it as zero.

The same care applies to food: a day with `logged: false` has no food log, \
which is not a day of eating nothing. Never average those in as zero, and say \
plainly when the log is too thin to draw a conclusion from.

Be direct and quantitative. Flag overreaching when the data shows it, without \
catastrophising, and say plainly what is going well. Prefer short paragraphs \
over bullet lists.";

/* ----------------------------------------------------------------- config --- */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Openrouter,
    Ollama,
}

impl Provider {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "openrouter" => Some(Self::Openrouter),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Openrouter => "openrouter",
            Self::Ollama => "ollama",
        }
    }

    fn base(self) -> &'static str {
        match self {
            Self::Openrouter => OPENROUTER_BASE,
            Self::Ollama => OLLAMA_BASE,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatConfig {
    pub provider: Option<&'static str>,
    pub model: Option<String>,
    pub has_key: bool,
    pub ollama_reachable: bool,
    pub ollama_models: Vec<String>,
}

pub fn load_config(db: &Db) -> Result<(Option<Provider>, Option<String>)> {
    let provider = db
        .sync_state("chat_provider")?
        .and_then(|s| Provider::parse(&s));
    let model = db.sync_state("chat_model")?;
    Ok((provider, model))
}

pub fn save_config(db: &Db, provider: Provider, model: &str) -> Result<()> {
    db.set_sync_state("chat_provider", provider.as_str())?;
    db.set_sync_state("chat_model", model)?;
    Ok(())
}

/// Whether a local Ollama is running, and what it has pulled. Failure here is
/// an answer ("not running"), not an error.
pub async fn probe_ollama(http: &reqwest::Client) -> (bool, Vec<String>) {
    let resp = http
        .get(format!("{OLLAMA_BASE}/models"))
        .timeout(std::time::Duration::from_millis(700))
        .send()
        .await;

    let Ok(resp) = resp else {
        return (false, vec![]);
    };
    if !resp.status().is_success() {
        return (false, vec![]);
    }
    let Ok(body) = resp.json::<Value>().await else {
        return (true, vec![]);
    };
    let models = body["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    (true, models)
}

/* ------------------------------------------------------------------ tools --- */

/// The tool surface offered to the model. Deliberately the same set the MCP
/// server exposes, backed by the same `garmin_core::query` functions — an
/// answer given here and one given in Claude Desktop come from identical code.
fn tool_schemas() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "recent_activities",
                "description": "List recent activities with per-zone HR breakdown, pace, cadence and training effect. Start here for 'how am I doing'.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "How many to return. Default 10." },
                        "sport": { "type": "string", "description": "Substring of Garmin's type key, e.g. 'running', 'strength', 'jump_rope'." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "activity_zones",
                "description": "Full HR zone breakdown for one activity: minutes and percent in zones 1-5. Omit activity_id for the most recent.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "activity_id": { "type": "integer" }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "zone_drift",
                "description": "Hard-effort drift across recent runs, plus the time-weighted easy/hard split for the window. Use for whether easy runs are staying easy.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "description": "How many recent runs. Default 10." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "cadence_trend",
                "description": "Running cadence across recent runs.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "description": "How many recent runs. Default 10." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "recovery",
                "description": "Resting HR, HRV, training readiness, sleep, stress and body battery by day. Use before advising a hard session.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "days": { "type": "integer", "description": "How many days back. Default 14." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "nutrition",
                "description": "Calories eaten against calories burned by day, plus hydration and sweat loss. Days with no food log have consumed_kcal null and logged:false — a missing log, not a day of eating nothing.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "days": { "type": "integer", "description": "How many days back. Default 30." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "workouts",
                "description": "The athlete's saved Garmin workouts. There is no training plan or goal race on the account, so these are the closest thing to a plan.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "routes",
                "description": "Routes grouped from cached GPS traces. Only outdoor activities have a trace; treadmill sessions never will.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "cache_status",
                "description": "What's in the local cache and when it was last refreshed. Check if data looks stale.",
                "parameters": { "type": "object", "properties": {} }
            }
        }
    ])
}

/// A human-readable label for what a tool call is about to read, shown in the
/// UI while the model works. Users should be able to see which of their data
/// a question actually touched.
fn describe(name: &str, args: &Value) -> String {
    match name {
        "recent_activities" => {
            let n = args["limit"].as_u64().unwrap_or(10);
            match args["sport"].as_str() {
                Some(s) => format!("Reading your last {n} {s} sessions"),
                None => format!("Reading your last {n} activities"),
            }
        }
        "activity_zones" => "Reading one activity's zone breakdown".into(),
        "zone_drift" => "Checking hard-effort drift across recent runs".into(),
        "cadence_trend" => "Checking cadence across recent runs".into(),
        "recovery" => {
            let d = args["days"].as_u64().unwrap_or(14);
            format!("Reading {d} days of recovery signals")
        }
        "nutrition" => {
            let d = args["days"].as_u64().unwrap_or(30);
            format!("Reading {d} days of food and hydration")
        }
        "workouts" => "Reading your saved Garmin workouts".into(),
        "routes" => "Reading your cached GPS routes".into(),
        "cache_status" => "Checking what's cached".into(),
        other => format!("Running {other}"),
    }
}

/// Runs one tool against the cache. Errors come back as JSON rather than
/// aborting the turn — the model can say "that failed" far better than a
/// stack trace can.
fn run_tool(name: &str, args: &Value) -> Value {
    let db = match Db::open_default() {
        Ok(db) => db,
        Err(e) => return json!({ "error": format!("could not open the cache: {e}") }),
    };

    let result: Result<Value> = (|| {
        Ok(match name {
            "recent_activities" => serde_json::to_value(query::recent_activities(
                &db,
                args["limit"].as_u64().unwrap_or(10) as u32,
                args["sport"].as_str(),
            )?)?,
            "activity_zones" => match query::activity_zones(&db, args["activity_id"].as_i64())? {
                Some(v) => serde_json::to_value(v)?,
                None => json!({ "error": "no such activity in the cache" }),
            },
            "zone_drift" => serde_json::to_value(query::zone_drift(
                &db,
                args["count"].as_u64().unwrap_or(10) as u32,
            )?)?,
            "cadence_trend" => serde_json::to_value(query::cadence_trend(
                &db,
                args["count"].as_u64().unwrap_or(10) as u32,
            )?)?,
            "recovery" => serde_json::to_value(query::recovery(
                &db,
                args["days"].as_u64().unwrap_or(14) as u32,
            )?)?,
            "nutrition" => serde_json::to_value(query::nutrition(
                &db,
                args["days"].as_u64().unwrap_or(30) as u32,
            )?)?,
            "workouts" => serde_json::to_value(db.workouts()?)?,
            "routes" => serde_json::to_value(query::route_summaries(&db)?)?,
            "cache_status" => serde_json::to_value(query::cache_status(&db)?)?,
            other => json!({ "error": format!("unknown tool {other}") }),
        })
    })();

    result.unwrap_or_else(|e| json!({ "error": e.to_string() }))
}

/* ------------------------------------------------------------------- turn --- */

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Event<'a> {
    Status { text: String },
    Delta { text: &'a str },
    Done { sources: Vec<String> },
    Error { text: String },
}

fn emit<R: Runtime>(app: &AppHandle<R>, channel: &str, event: Event<'_>) {
    // A failed emit means the window went away mid-turn; there is nobody left
    // to tell, so dropping it is the whole correct response.
    let _ = app.emit(channel, event);
}

/// Runs one assistant turn: tool rounds until the model stops asking for them,
/// then streams the answer. Progress and text both arrive on `chat:{id}`.
pub async fn run_turn<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    history: Vec<HistoryMessage>,
) -> Result<()> {
    let channel = format!("chat:{id}");

    match turn_inner(&app, &channel, history).await {
        Ok(sources) => {
            emit(&app, &channel, Event::Done { sources });
            Ok(())
        }
        Err(e) => {
            let text = e
                .chain()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(": ");
            emit(&app, &channel, Event::Error { text: text.clone() });
            Err(anyhow!(text))
        }
    }
}

async fn turn_inner<R: Runtime>(
    app: &AppHandle<R>,
    channel: &str,
    history: Vec<HistoryMessage>,
) -> Result<Vec<String>> {
    let db = Db::open_default()?;
    let (provider, model) = load_config(&db)?;
    let provider = provider.context("No model provider chosen yet.")?;
    let model = model.context("No model chosen yet.")?;

    let key = match provider {
        Provider::Openrouter => Some(
            store::load_openrouter_key()?
                .context("No OpenRouter key stored. Add one in Settings.")?,
        ),
        Provider::Ollama => None,
    };

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;

    let mut messages = vec![json!({ "role": "system", "content": SYSTEM_PROMPT })];
    for m in history {
        messages.push(json!({ "role": m.role, "content": m.content }));
    }

    let mut sources: Vec<String> = Vec::new();

    for round in 0..=MAX_TOOL_ROUNDS {
        // On the last permitted round, drop the tools so the model is forced to
        // answer from what it already has rather than asking for more.
        let offer_tools = round < MAX_TOOL_ROUNDS;

        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": true,
        });
        if offer_tools {
            body["tools"] = tool_schemas();
        }

        let mut req = http
            .post(format!("{}/chat/completions", provider.base()))
            .json(&body);
        if let Some(k) = &key {
            req = req
                .header("Authorization", format!("Bearer {k}"))
                // OpenRouter attributes requests by these; they're optional but
                // it's rude to show up anonymous on someone else's rate limit.
                .header("HTTP-Referer", "https://github.com/omznc/garmin-companion")
                .header("X-Title", "Garmin Companion");
        }

        let resp = req.send().await.context("could not reach the model")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "{} returned {status}: {}",
                provider.as_str(),
                text.chars().take(300).collect::<String>()
            ));
        }

        let stream = stream_completion(app, channel, resp).await?;

        if stream.tool_calls.is_empty() {
            return Ok(sources);
        }

        // Echo the model's own tool-call message back before the results, or
        // the next request has orphaned tool responses and the API rejects it.
        messages.push(json!({
            "role": "assistant",
            "content": stream.content,
            "tool_calls": stream.tool_calls.iter().map(|c| json!({
                "id": c.id,
                "type": "function",
                "function": { "name": c.name, "arguments": c.arguments },
            })).collect::<Vec<_>>(),
        }));

        for call in &stream.tool_calls {
            let args: Value = serde_json::from_str(&call.arguments).unwrap_or(json!({}));
            let label = describe(&call.name, &args);
            emit(
                app,
                channel,
                Event::Status {
                    text: label.clone(),
                },
            );
            sources.push(label);

            let result = run_tool(&call.name, &args);
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call.id,
                "name": call.name,
                "content": result.to_string(),
            }));
        }
    }

    Ok(sources)
}

/* ------------------------------------------------------------- SSE parsing --- */

#[derive(Default)]
struct ToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct StreamResult {
    content: String,
    tool_calls: Vec<ToolCall>,
}

/// Consumes an OpenAI-style SSE stream, forwarding text deltas to the UI as
/// they arrive and accumulating any tool calls.
///
/// Tool calls stream in fragments: the first chunk carries the id and name, and
/// later chunks append to `arguments` a few characters at a time, addressed by
/// the `index` field rather than by id. Both have to be reassembled before the
/// call is usable.
async fn stream_completion<R: Runtime>(
    app: &AppHandle<R>,
    channel: &str,
    resp: reqwest::Response,
) -> Result<StreamResult> {
    let mut out = StreamResult::default();
    let mut by_index: Vec<ToolCall> = Vec::new();
    let mut buf = String::new();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("the model stream was cut short")?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        // SSE events are separated by a blank line; a partial event stays in
        // the buffer until its terminator arrives.
        while let Some(split) = buf.find("\n\n").or_else(|| buf.find("\r\n\r\n")) {
            let raw = buf[..split].to_string();
            let skip = if buf[split..].starts_with("\r\n\r\n") {
                4
            } else {
                2
            };
            buf.drain(..split + skip);

            for line in raw.lines() {
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<Value>(data) else {
                    continue;
                };

                // OpenRouter reports upstream failures inside the stream body
                // with a 200 status, so this is the only place they surface.
                if let Some(err) = v.get("error") {
                    let msg = err["message"]
                        .as_str()
                        .unwrap_or("the model returned an error");
                    return Err(anyhow!(msg.to_string()));
                }

                let delta = &v["choices"][0]["delta"];

                if let Some(text) = delta["content"].as_str() {
                    if !text.is_empty() {
                        out.content.push_str(text);
                        emit(app, channel, Event::Delta { text });
                    }
                }

                if let Some(calls) = delta["tool_calls"].as_array() {
                    for c in calls {
                        let idx = c["index"].as_u64().unwrap_or(0) as usize;
                        while by_index.len() <= idx {
                            by_index.push(ToolCall::default());
                        }
                        let slot = &mut by_index[idx];
                        if let Some(id) = c["id"].as_str() {
                            if !id.is_empty() {
                                slot.id = id.to_string();
                            }
                        }
                        if let Some(name) = c["function"]["name"].as_str() {
                            if !name.is_empty() {
                                slot.name = name.to_string();
                            }
                        }
                        if let Some(args) = c["function"]["arguments"].as_str() {
                            slot.arguments.push_str(args);
                        }
                    }
                }
            }
        }
    }

    out.tool_calls = by_index
        .into_iter()
        .filter(|c| !c.name.is_empty())
        .map(|mut c| {
            // Some providers omit the id entirely for a single call; the
            // follow-up request still needs one to pair the result against.
            if c.id.is_empty() {
                c.id = format!("call_{}", c.name);
            }
            if c.arguments.trim().is_empty() {
                c.arguments = "{}".into();
            }
            c
        })
        .collect();

    Ok(out)
}

/* ------------------------------------------------------------------ tests --- */

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Listener;

    /// Every tool the model is offered must actually dispatch. A schema that
    /// names a tool `run_tool` doesn't handle would only surface as the model
    /// being told "unknown tool" mid-conversation, which is invisible here
    /// unless something checks the two lists against each other.
    #[test]
    fn every_offered_tool_dispatches() {
        let schemas = tool_schemas();
        let names: Vec<String> = schemas
            .as_array()
            .expect("tool schemas are an array")
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap().to_string())
            .collect();

        assert!(names.contains(&"nutrition".to_string()));
        assert!(names.contains(&"workouts".to_string()));
        assert!(names.contains(&"routes".to_string()));

        for name in &names {
            let out = run_tool(name, &json!({}));
            let err = out.get("error").and_then(|e| e.as_str()).unwrap_or("");
            assert!(
                !err.starts_with("unknown tool"),
                "{name} is offered to the model but has no dispatch arm"
            );
            // Every tool also needs a human-readable label for the UI.
            assert!(
                !describe(name, &json!({})).starts_with("Running "),
                "{name} has no description in `describe`"
            );
        }
    }

    /// Drives one real turn against the configured provider. Ignored by
    /// default because it spends the athlete's own API credit and needs the
    /// keyring unlocked:
    ///
    ///   cargo test -p app --lib -- --ignored --nocapture chat_turn
    #[tokio::test]
    #[ignore = "hits the live model provider and needs a key in the keyring"]
    async fn chat_turn_streams_an_answer() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();

        let received = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = received.clone();
        handle.listen("chat:test", move |ev| {
            sink.lock().unwrap().push_str(ev.payload());
        });

        let history = vec![HistoryMessage {
            role: "user".into(),
            content: "In one sentence, how many activities are in my cache? \
                      Use a tool to find out."
                .into(),
        }];

        run_turn(handle, "test".into(), history)
            .await
            .expect("the turn should complete");

        let got = received.lock().unwrap().clone();
        println!("--- stream events ---\n{got}\n---------------------");
        assert!(got.contains("delta"), "the model streamed no text");
        assert!(got.contains("done"), "the turn never finished");
    }
}
