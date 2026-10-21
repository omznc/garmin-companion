//! The "Claude Code" coach: one turn runs the `claude` command of the user.
//!
//! The command reads the app's tools over MCP (`mcp.rs`, started as
//! `garmin-companion mcp`). It has no other tool. No cloud credits are used.
//! The turn is stateless: each turn sends the whole trimmed history.
//!
//! The command writes stream-json on stdout. [`Parser`] turns each line into
//! the same `garmin_coach::Event`s as the other providers, so Ask needs no change.
//! The parts that every local command shares are in `cli_agent.rs`.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use garmin_coach::{Event, EventSink, HistoryMessage, Period, DEFAULT_CLAUDE_MODEL};
use serde_json::{json, Value};

use crate::cli_agent::{self as cli, error, is_batch, Cli, Run, Status, Stream, Temp};

pub const CLI: Cli = Cli {
    name: "Claude Code",
    bin: "claude",
    login_hint: "Run `claude` once.",
    home_dirs: &[".local/bin"],
    // The variables that mark a parent Claude Code session. The other
    // `CLAUDE_CODE_*` variables stay: a login setup can need them
    // (`CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CODE_USE_BEDROCK`). So does `CLAUDE_CONFIG_DIR`.
    env_remove: &[
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_SSE_PORT",
    ],
};

const NOT_INSTALLED: &str = "Claude Code is not installed. Install it from https://code.claude.com and run `claude` once to log in.";
const NOT_LOGGED_IN: &str = "Claude Code is not logged in. Run `claude` in a terminal and log in.";
const NO_TOOLS: &str =
    "Claude Code could not start the Garmin tools of this app. Restart the app and ask again.";
const TOOL_PREFIX: &str = "mcp__garmin__";

/* ------------------------------------------------------------------ parser --- */

/// A tool call whose input is still streaming.
struct Call {
    id: String,
    name: String,
    input: String,
}

/// Turns the lines of the stream into events. It holds the state between lines.
#[derive(Default)]
struct Parser {
    /// The text of the message that streams now. A tool-use message makes it a note.
    text: String,
    /// Tool blocks that stream, by block index.
    open: HashMap<u64, Call>,
    /// Tool calls that await their result, by tool-use id.
    waiting: HashMap<String, Call>,
    sources: Vec<String>,
    /// The `assistant` message of a failed login says so before the `result`.
    logged_out: bool,
    /// The `result` line came: the turn is over.
    finished: bool,
    /// The turn ended before the `result` line, and the process still runs.
    aborted: bool,
}

impl Stream for Parser {
    fn feed(&mut self, line: &Value) -> Vec<Event> {
        match line["type"].as_str() {
            Some("stream_event") => self.stream(&line["event"]),
            Some("assistant") => {
                self.check_login(&line["message"]);
                Vec::new()
            }
            Some("user") => self.results(&line["message"]["content"]),
            Some("result") => self.result(line),
            Some("system") if line["subtype"] == "init" => self.init(line),
            _ => Vec::new(),
        }
    }

    fn finished(&self) -> bool {
        self.finished
    }

    fn aborted(&self) -> bool {
        self.aborted
    }

    fn sources(&self) -> Vec<String> {
        self.sources.clone()
    }
}

impl Parser {
    /// Without the garmin server the model has no tools, and it writes tool
    /// calls as text. So stop the turn.
    fn init(&mut self, line: &Value) -> Vec<Event> {
        let servers = line["mcp_servers"].as_array().into_iter().flatten();
        let connected = servers
            .filter(|s| s["name"] == "garmin")
            .any(|s| s["status"] == "connected");
        if connected {
            return Vec::new();
        }
        self.finished = true;
        self.aborted = true;
        vec![error(NO_TOOLS)]
    }

    fn stream(&mut self, ev: &Value) -> Vec<Event> {
        match ev["type"].as_str() {
            Some("message_start") => {
                self.text.clear();
                Vec::new()
            }
            Some("content_block_start") => self.block_start(ev),
            Some("content_block_delta") => self.block_delta(ev),
            Some("content_block_stop") => {
                if let Some(call) = ev["index"].as_u64().and_then(|i| self.open.remove(&i)) {
                    self.waiting.insert(call.id.clone(), call);
                }
                Vec::new()
            }
            Some("message_delta") => self.message_delta(ev),
            _ => Vec::new(),
        }
    }

    fn block_start(&mut self, ev: &Value) -> Vec<Event> {
        let block = &ev["content_block"];
        let (Some(index), Some("tool_use")) = (ev["index"].as_u64(), block["type"].as_str()) else {
            return Vec::new();
        };
        let call = Call {
            id: block["id"].as_str().unwrap_or_default().into(),
            name: block["name"].as_str().unwrap_or_default().into(),
            input: String::new(),
        };
        let event = tool_event(&call, false, true);
        self.open.insert(index, call);
        vec![event]
    }

    fn block_delta(&mut self, ev: &Value) -> Vec<Event> {
        let delta = &ev["delta"];
        match delta["type"].as_str() {
            Some("text_delta") => {
                let text = delta["text"].as_str().unwrap_or_default();
                self.text.push_str(text);
                vec![Event::Delta { text: text.into() }]
            }
            Some("input_json_delta") => {
                let call = ev["index"].as_u64().and_then(|i| self.open.get_mut(&i));
                if let Some(call) = call {
                    call.input
                        .push_str(delta["partial_json"].as_str().unwrap_or_default());
                }
                Vec::new()
            }
            // `thinking_delta` and the rest stay hidden.
            _ => Vec::new(),
        }
    }

    /// Text before a tool call was narration, not the answer.
    fn message_delta(&mut self, ev: &Value) -> Vec<Event> {
        let narration = ev["delta"]["stop_reason"] == "tool_use" && !self.text.trim().is_empty();
        if !narration {
            return Vec::new();
        }
        let text = std::mem::take(&mut self.text);
        vec![Event::Note {
            text: text.trim().into(),
        }]
    }

    fn results(&mut self, content: &Value) -> Vec<Event> {
        let mut events = Vec::new();
        for item in content.as_array().into_iter().flatten() {
            if item["type"] != "tool_result" {
                continue;
            }
            let id = item["tool_use_id"].as_str().unwrap_or_default();
            let call = self.waiting.remove(id).unwrap_or_else(|| Call {
                id: id.into(),
                name: String::new(),
                input: String::new(),
            });
            let event = tool_event(&call, true, item["is_error"] != true);
            if let Event::Tool { label, .. } = &event {
                self.sources.push(label.clone());
            }
            events.push(event);
        }
        events
    }

    fn check_login(&mut self, message: &Value) {
        let text = message["content"][0]["text"].as_str().unwrap_or_default();
        if message["model"] == "<synthetic>" && text.contains("Not logged in") {
            self.logged_out = true;
        }
    }

    fn result(&mut self, line: &Value) -> Vec<Event> {
        self.finished = true;
        let text = line["result"].as_str().unwrap_or_default();
        // A normal answer can contain the phrase. Only an error result counts.
        if self.logged_out || (line["is_error"] == true && text.contains("Not logged in")) {
            return vec![error(NOT_LOGGED_IN)];
        }
        if line["is_error"] == true {
            let text = if text.is_empty() {
                "Claude Code reported an error."
            } else {
                text
            };
            return vec![error(text)];
        }
        if self.text.trim().is_empty() {
            return vec![error(cli::NO_ANSWER)];
        }
        vec![Event::Done {
            sources: self.sources(),
        }]
    }
}

/// A tool event. The label reads the finished input, when the call has one.
fn tool_event(call: &Call, done: bool, ok: bool) -> Event {
    let name = call.name.strip_prefix(TOOL_PREFIX).unwrap_or(&call.name);
    let args: Value = serde_json::from_str(&call.input).unwrap_or(json!({}));
    cli::tool_event(&call.id, name, &args, done, ok)
}

/* ----------------------------------------------------------------- command --- */

fn mcp_config(app: &Path, db: Option<&Path>, period: Option<Period>) -> String {
    json!({ "mcpServers": { "garmin": {
        "type": "stdio",
        "command": app.to_string_lossy(),
        "args": cli::mcp_args(db, period),
    } } })
    .to_string()
}

/// The flags of 1.1 in CLI-AGENTS.md. Each empty string is its own item.
/// Without a `question`, the command reads it from stdin.
fn flags(prompt_file: &Path, model: &str, mcp: String, question: Option<String>) -> Vec<OsString> {
    let model = if model.is_empty() {
        DEFAULT_CLAUDE_MODEL
    } else {
        model
    };
    let mut args: Vec<OsString> = vec!["-p".into()];
    args.extend(question.map(OsString::from));
    let pairs = [
        ("--mcp-config", mcp.as_str()),
        ("--tools", ""),
        ("--allowedTools", "mcp__garmin__*"),
        ("--permission-mode", "dontAsk"),
        ("--setting-sources", ""),
        ("--output-format", "stream-json"),
        ("--model", model),
        ("--max-turns", "8"),
    ];
    for (flag, value) in pairs {
        args.extend([flag.into(), value.into()]);
    }
    args.extend(["--system-prompt-file".into(), prompt_file.into()]);
    for flag in [
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--no-session-persistence",
        "--verbose",
        "--include-partial-messages",
    ] {
        args.push(flag.into());
    }
    args
}

/* -------------------------------------------------------------------- turn --- */

/// Run one turn. `db` is a cache copy, or `None` for the default cache. The
/// result goes to the sink as `Done` or `Error`. A stopped turn ends with `Done`.
pub fn run_turn(
    db: Option<&Path>,
    model: &str,
    history: Vec<HistoryMessage>,
    period: Option<Period>,
    sink: &dyn EventSink,
    stop: &AtomicBool,
) {
    if let Err(text) = turn(db, model, history, period, sink, stop) {
        sink.emit(error(&text));
    }
}

fn turn(
    db: Option<&Path>,
    model: &str,
    history: Vec<HistoryMessage>,
    period: Option<Period>,
    sink: &dyn EventSink,
    stop: &AtomicBool,
) -> Result<(), String> {
    let setup = cli::setup(&CLI, NOT_INSTALLED, db, period)?;
    let prompt_file = Temp::new("claude-prompt.md");
    std::fs::write(&prompt_file.0, &setup.system)
        .map_err(|e| format!("Could not write the prompt: {e}"))?;

    let mut mcp = mcp_config(&setup.app, db, period);
    let mut question = Some(garmin_coach::external::transcript(history));
    let batch = is_batch(&setup.bin);
    // A batch file cannot take the question or the JSON in its arguments.
    let config_file = Temp::new("claude-mcp.json");
    if batch {
        std::fs::write(&config_file.0, &mcp)
            .map_err(|e| format!("Could not write the tool config: {e}"))?;
        mcp = config_file.0.to_string_lossy().into();
    }
    let stdin = if batch { question.take() } else { None };
    let run = Run {
        cli: &CLI,
        bin: setup.bin,
        args: flags(&prompt_file.0, model, mcp, question),
        cwd: setup.cwd,
        env: Vec::new(),
        stdin,
        limit: None,
    };
    cli::run_child(run, &mut Parser::default(), sink, stop)
}

/* ------------------------------------------------------------------ status --- */

fn parse_status(version: &str, auth_json: &str) -> Status {
    let auth: Value = serde_json::from_str(auth_json).unwrap_or_default();
    if auth["loggedIn"] != true {
        return Status::LoggedOut;
    }
    Status::Ready {
        version: cli::version_of(version),
        who: auth["email"].as_str().unwrap_or("your account").into(),
    }
}

/// Ask the command for its version and login. It blocks, so call it off the UI thread.
pub fn probe() -> Status {
    match cli::find(&CLI) {
        Some(bin) => probe_with(&bin, cli::PROBE_LIMIT),
        None => Status::NotInstalled,
    }
}

fn probe_with(bin: &Path, limit: Duration) -> Status {
    let version = match cli::read(bin, &CLI, &["--version"], limit) {
        Ok(v) => v.text,
        Err(status) => return status,
    };
    match cli::read(bin, &CLI, &["auth", "status"], limit) {
        Ok(auth) => parse_status(&version, &auth.text),
        Err(status) => status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn events(fixture: &str) -> Vec<Event> {
        let mut parser = Parser::default();
        fixture
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .flat_map(|v| parser.feed(&v))
            .collect()
    }

    #[test]
    fn a_tool_turn_gives_tool_events_then_the_answer_then_done() {
        let all = events(include_str!("../tests/claude-tool-turn.jsonl"));
        let label = "Reading 2 days of recovery signals";
        assert!(matches!(&all[0], Event::Tool { running: true, call_id, .. }
            if call_id == "toolu_01HA5osNoYpCqK7NeeiFDTQ4"));
        assert!(
            matches!(&all[1], Event::Tool { running: false, ok: true, label: l, .. }
            if l == label)
        );
        let text: String = all
            .iter()
            .filter_map(|e| match e {
                Event::Delta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            text,
            "Your resting HR yesterday (2026-10-19) was **48 bpm**."
        );
        assert_eq!(
            all.last(),
            Some(&Event::Done {
                sources: vec![label.into()]
            })
        );
        assert!(!all
            .iter()
            .any(|e| matches!(e, Event::Note { .. } | Event::Error { .. })));
    }

    #[test]
    fn a_turn_without_the_garmin_server_stops() {
        let init = |status: &str| {
            json!({ "type": "system", "subtype": "init",
                    "mcp_servers": [{ "name": "garmin", "status": status }] })
        };
        let mut parser = Parser::default();
        assert!(parser.feed(&init("connected")).is_empty());
        assert!(!parser.finished);
        let mut parser = Parser::default();
        assert_eq!(parser.feed(&init("failed")), vec![error(NO_TOOLS)]);
        assert!(parser.finished && parser.aborted);
    }

    #[test]
    fn narration_before_a_tool_call_becomes_a_note() {
        let mut p = Parser::default();
        let mut feed = |v: Value| p.feed(&v);
        let stream = |e: Value| json!({ "type": "stream_event", "event": e });
        feed(stream(json!({"type":"message_start"})));
        feed(stream(json!({"type":"content_block_delta","index":0,
            "delta":{"type":"text_delta","text":" Let me check. "}})));
        let out = feed(stream(
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),
        ));
        assert_eq!(
            out,
            vec![Event::Note {
                text: "Let me check.".into()
            }]
        );
    }

    #[test]
    fn a_missing_login_gives_the_login_error() {
        let all = events(include_str!("../tests/claude-no-login.jsonl"));
        assert_eq!(all, vec![error(NOT_LOGGED_IN)]);
    }

    #[test]
    fn the_flags_keep_the_empty_values_as_items() {
        let args = flags(Path::new("p.md"), "", "{}".into(), Some("User: hi".into()));
        let text: Vec<String> = args.iter().map(|a| a.to_string_lossy().into()).collect();
        let at = |flag: &str| text.iter().position(|a| a == flag).unwrap();
        assert_eq!(text[at("--tools") + 1], "");
        assert_eq!(text[at("--setting-sources") + 1], "");
        assert_eq!(text[at("--model") + 1], "sonnet");
        assert_eq!(text[at("--allowedTools") + 1], "mcp__garmin__*");
        assert_eq!(text[at("--max-turns") + 1], "8");
        assert_eq!(&text[..2], ["-p", "User: hi"]);
        assert!(text.contains(&"--strict-mcp-config".to_string()));
    }

    #[test]
    fn a_reply_with_the_login_phrase_is_not_a_login_error() {
        let mut p = Parser {
            text: "Say \"Not logged in\" if you see it.".into(),
            ..Parser::default()
        };
        let out = p.result(&json!({ "type": "result", "result": "Not logged in" }));
        assert!(matches!(out[0], Event::Done { .. }));
    }

    #[test]
    fn the_mcp_config_names_the_server_garmin() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        let period = Some(Period {
            start: day,
            end: day,
        });
        let v: Value = serde_json::from_str(&mcp_config(Path::new("/app"), None, period)).unwrap();
        let server = &v["mcpServers"]["garmin"];
        assert_eq!(server["command"], "/app");
        assert_eq!(
            server["args"],
            json!(["mcp", "--focus", "2026-03-05..2026-03-05"])
        );
    }

    struct Rec(Mutex<Vec<Event>>);
    impl EventSink for Rec {
        fn emit(&self, e: Event) {
            self.0.lock().unwrap().push(e);
        }
    }

    /// Needs a logged-in `claude`. Run with `GC_LIVE_DB=<cache copy>` and
    /// `APPIMAGE=<release binary>`.
    #[test]
    #[ignore]
    fn a_real_turn_reads_the_copy_over_mcp() {
        let db = PathBuf::from(std::env::var("GC_LIVE_DB").unwrap());
        let rec = Rec(Mutex::default());
        let history = vec![HistoryMessage {
            role: "user".into(),
            content: "What was my resting heart rate in the last 3 days?".into(),
        }];
        run_turn(
            Some(&db),
            "haiku",
            history,
            None,
            &rec,
            &AtomicBool::new(false),
        );
        let all = rec.0.into_inner().unwrap();
        let tool = |r| {
            all.iter()
                .any(|e| matches!(e, Event::Tool { running, .. } if *running == r))
        };
        assert!(tool(true) && tool(false), "{all:?}");
        assert!(
            matches!(all.last(), Some(Event::Done { sources }) if !sources.is_empty()),
            "{all:?}"
        );
    }

    #[test]
    fn the_status_reads_the_login_json() {
        let ok = parse_status(
            "2.1.304 (Claude Code)\n",
            r#"{"loggedIn":true,"email":"a@b.c"}"#,
        );
        assert_eq!(ok.line(&CLI), "Claude Code 2.1.304, logged in as a@b.c");
        assert_eq!(
            parse_status("2.1.304", r#"{"loggedIn":false}"#),
            Status::LoggedOut
        );
        assert_eq!(parse_status("", ""), Status::LoggedOut);
    }
}
