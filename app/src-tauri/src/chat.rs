//! Coaching chat over an OpenAI-compatible endpoint.
//!
//! Three providers, all speaking the same wire format: the hosted proxy this
//! project runs (nothing to configure, and it picks up the bill), OpenRouter
//! (hosted, needs the user's own key) and Ollama (local, needs nothing). The
//! key lives in the OS keyring; the chosen provider and model live in the
//! cache's key-value table alongside the sync state.
//!
//! The model answers by calling tools that read the local SQLite cache. It
//! never receives the cache wholesale, and it never gets a network path to
//! Garmin — the only data that leaves this machine is whatever a tool returned
//! for the question actually asked.
//!
//! That holds for `draft_workout` too, which is the one tool that isn't a
//! question. It writes nothing: it validates a proposed session and hands it
//! back for the athlete to look at. The workout reaches Garmin only when they
//! press the button on it, through a command the model cannot call.

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use garmin_core::{db::Db, query, store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::oneshot;

const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";
const OLLAMA_BASE: &str = "http://localhost:11434/v1";

/// The hosted proxy, which is a Cloudflare Worker in `worker/` holding the
/// project's own OpenRouter key. Overridable at compile time so a build can be
/// pointed at a `wrangler dev` on localhost:
///
/// ```sh
/// GARMIN_CLOUD_BASE=http://localhost:8787/v1 cargo tauri dev
/// ```
const CLOUD_BASE: &str = match option_env!("GARMIN_CLOUD_BASE") {
    Some(url) => url,
    None => "https://coach.omznc.workers.dev/v1",
};

/// Cap on tool round trips in one turn. Every tool here reads a bounded slice
/// of a local table, so a model that keeps calling them is looping, not working.
const MAX_TOOL_ROUNDS: usize = 6;

/// How many questions the model may put to the athlete in one turn.
///
/// Two, because the thing being prevented is an interrogation: a model that can
/// ask freely will happily spend a turn confirming what it could have read from
/// the cache, and the athlete asked a question rather than volunteering to
/// answer a form. Past this the tool returns an error telling it to decide.
const MAX_ASKS_PER_TURN: usize = 2;

/// How long a question waits before the turn gives up on it and carries on.
///
/// Ten minutes is long enough that a question you walked away from is still
/// answerable when you come back, and short enough that a turn nobody is
/// watching doesn't hold a provider connection open all day.
const ASK_TIMEOUT_SECS: u64 = 600;

/// How much of a conversation goes back to the model.
///
/// The screen keeps the whole transcript — it's the record of what was said.
/// The model doesn't need it: the entire array is re-sent on every round of
/// every turn, so an unbounded history makes a long session cost roughly the
/// square of its length. The opening question is kept regardless, because it's
/// what the conversation is about; everything between it and the tail is not.
const HISTORY_MESSAGES: usize = 12;
const HISTORY_CHARS: usize = 20_000;

/// Ceilings on how much a tool may read in one call.
///
/// The model picks these numbers and nothing in the schema stops it picking
/// 5000. The cache holds thousands of activities, and whatever a tool returns
/// is re-sent on every remaining round of the turn, so one careless argument is
/// paid for six more times. Every window is a slice, not the table.
const MAX_ACTIVITIES: u64 = 50;
const MAX_DAYS: u64 = 365;
const MAX_WEIGHT_DAYS: u64 = 730;

const SYSTEM_PROMPT: &str = "\
You are a running and training coach with direct read access to this athlete's \
Garmin history, held in a local cache on their machine.

Answer from the tools, not from general knowledge. Call a tool before making \
any claim about a number. If a tool returns nothing, say so — never estimate a \
figure to fill a gap.

You have no clock of your own. The turn after this one gives you today's date \
and how current the cache is, and those are the only source for both — a date \
on a row tells you when that row happened, never how long ago it was. Before \
calling anything recent, or judging whether they are recovered enough to train \
today, check the data actually reaches today. When it doesn't, say where it \
stops and answer about the period you have. A confident answer about this week \
built on a reading from two months ago is the worst thing you can produce here, \
and it is indistinguishable from a correct one unless you check.

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

Not every recorded number is a measurement, and the ones that aren't say so. \
Read these before quoting a figure they apply to:

- `hr_confidence.level` is `good`, `caution` or `poor` for the session's zone \
split, with `hr_confidence.notes` giving the reason in full. Never drop a \
caveated session from a total — the athlete did the work, and a silent gap is \
worse than a flagged number — but say the caveat the first time you lean on \
that session, and don't build an argument about drift on a `poor` one alone.
- `hr_confidence.cadenceLock` at `likely` means heart rate shadowed step rate \
for most of the session. That is the wrist sensor reading arm swing as pulse, and it matters \
most exactly where this athlete's coaching lives: a locked reading at 170 steps \
per minute reports 170 bpm, which is their Z4. If several hard-looking runs \
carry this flag, say plainly that the drift may be an artefact and that a chest \
strap is the only way to settle it. Do not soften it into a training \
observation.
- `pace_estimated: true` means distance and pace came off the arm accelerometer \
rather than GPS or the treadmill belt, and can be well out. Cadence and heart \
rate on those sessions are fine. Comparing two estimated paces compares two \
estimates — say so rather than reporting a improvement of a few seconds per \
kilometre as though it were measured.
- `moving_pace_min_per_km` excludes walk breaks and `pace_min_per_km` doesn't. \
On a deliberate run/walk session the first is the one about the running.
- `resting_hr_source` is `overnight`, `daytimeEstimate` or `unverified`. Only \
`overnight` days are Garmin's real resting heart rate; the others are a rough \
figure from the waking day. Never put them on one trend line without saying so, \
and never read a jump between two kinds as a change in fitness.

All of that is about running and other continuous aerobic work. It does not \
transfer to strength training, jump rope, circuits or a tactical session: there \
the heart rate climbs and falls because sets and rests do, so the zone split \
describes the work-to-rest ratio rather than a target that was hit or missed. \
Never prescribe a heart-rate ceiling for one of those, never read time above Z2 \
in one as drift, and keep them out of any easy/hard split you compute for the \
running.

The same care applies to food: a day with `logged: false` has no food log, \
which is not a day of eating nothing. Never average those in as zero, and say \
plainly when the log is too thin to draw a conclusion from.

Weight is sparser still: weigh-ins are irregular, so quote `trendKg` rather \
than a single reading, treat a point marked `outlier` as a mis-entry, and never \
describe a direction when `rateKgPerWeek` is null.

Be direct and quantitative. Flag overreaching when the data shows it, without \
catastrophising, and say plainly what is going well. Prefer short paragraphs \
over bullet lists.

When a session is what's being asked for, call `draft_workout` and build it. \
Read the recent data first — a session prescribed without looking at the last \
few runs and this morning's recovery is a guess. Say in prose why the workout \
is shaped the way it is; the athlete sees the steps themselves on a card and \
does not need them listed back.

If the data doesn't reach today, a session built on it is a session for the \
athlete they were then. Still build one when they ask, but pitch it off the \
gap: someone returning after weeks away needs less than their old numbers \
suggest, and the honest reason for that belongs in the prose.

`draft_workout` does not save anything. It proposes, and the athlete presses a \
button to send it to Garmin or edits it first. Never tell them a workout has \
been created, added to their watch, or scheduled — none of that has happened \
when the tool returns. If it returns an error, the draft was rejected: fix what \
it names and call it again rather than describing the session in prose instead.

`ask_athlete` puts one question on their screen, with answers they can tap, and \
waits for the one they pick. It is the only way you have of asking them \
anything. A question written into your prose is not asking — it ends the turn \
with a question mark on it, and leaves them typing out a reply you could have \
handed them as a button. So: if any part of your answer would put a question to \
the athlete, including the last line of it, that question goes through \
`ask_athlete` instead. Never both, and never the prose version alone.

Ask before doing the work, not after: a question that arrives under a finished \
answer is too late to have been worth asking. Ask when the answer would change \
what you advise and no tool can supply it — how long they have today, whether \
yesterday's niggle has settled, whether they want this one easy or hard. \
Everything the cache holds you read; you never ask for it. You never ask \
permission to answer, and you never meet a factual question about their own \
numbers with a question back. If they don't answer, decide anyway and say what \
you assumed.

You can also make this app's colour themes. When they ask for one, call \
`save_theme` and build it — do not answer with a list of hex codes for them to \
type in. Judge contrast honestly: `fg` on `bg` is body text and has to be \
comfortable to read for a long time, and `faint` is the quietest step that is \
still legible, not an invisible one. `bg2` is the sidebar and sits one small \
step away from the page — on a dark theme that means lighter than `bg`, because \
a step further down from a near-black page reads as a hole rather than as a \
surface. Commit to a real point of view; a theme that is the default with the \
accent moved is not worth saving.

When they asked you to make a theme, call `use_theme` with the slug straight \
after saving it. Describing a colour scheme in prose is not showing it to them, \
and switching costs them one click to undo. Don't apply one they didn't ask \
for. To revise a theme, call `list_themes` first and save under the same name, \
which replaces it rather than making a second.";

/* ----------------------------------------------------------------- config --- */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// The project's own proxy. No key, no account, no model to pick — and the
    /// bill lands on whoever runs the worker rather than on the athlete.
    Cloud,
    Openrouter,
    Ollama,
}

impl Provider {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cloud" => Some(Self::Cloud),
            "openrouter" => Some(Self::Openrouter),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::Openrouter => "openrouter",
            Self::Ollama => "ollama",
        }
    }

    fn base(self) -> &'static str {
        match self {
            Self::Cloud => CLOUD_BASE,
            Self::Openrouter => OPENROUTER_BASE,
            Self::Ollama => OLLAMA_BASE,
        }
    }

    /// Whether requests leave this machine at all.
    pub fn hosted(self) -> bool {
        !matches!(self, Self::Ollama)
    }

    /// The model to use when none has been chosen, where that has an answer.
    /// Ollama's depends on what's been pulled, so it hasn't got one here.
    fn default_model(self) -> Option<&'static str> {
        match self {
            Self::Cloud => Some(CLOUD_MODEL),
            Self::Openrouter => Some(DEFAULT_OPENROUTER_MODEL),
            Self::Ollama => None,
        }
    }
}

/// What OpenRouter gets pointed at when nothing has been chosen.
///
/// Cheap, fast, long-context, and it calls tools — which is the one thing this
/// app cannot work without. It does not advertise structured outputs, so the
/// follow-up call falls back to parsing prose from it; see [`followups`].
pub const DEFAULT_OPENROUTER_MODEL: &str = "inclusionai/ling-3.0-flash";

/// The only model the hosted proxy will serve.
///
/// Not a preference — the worker rejects anything else, because an endpoint
/// that forwards whatever model id it is handed is an endpoint whose cost per
/// request is set by its callers. Widening this is a worker-side change and a
/// change here; the two have to agree, so they are both one constant.
pub const CLOUD_MODEL: &str = DEFAULT_OPENROUTER_MODEL;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatConfig {
    pub provider: Option<&'static str>,
    pub model: Option<String>,
    pub has_key: bool,
    /// Whether the chosen model takes a JSON schema, recorded when it was
    /// picked so no request has to go and ask.
    pub structured: bool,
    pub ollama_reachable: bool,
    pub ollama_models: Vec<String>,
}

pub fn load_config(db: &Db) -> Result<(Option<Provider>, Option<String>)> {
    let provider = db
        .sync_state("chat_provider")?
        .and_then(|s| Provider::parse(&s));

    // A hosted setup with no model named is the default model, not a broken
    // config — for OpenRouter the key is the part only you can supply, and for
    // the proxy there is nothing to supply at all.
    //
    // Cloud takes its model rather than reading one: the proxy serves exactly
    // one, so a stale `chat_model` left behind by an earlier provider would
    // otherwise be sent to it and bounced.
    let model = match provider {
        Some(Provider::Cloud) => Some(CLOUD_MODEL.to_string()),
        Some(p) => db
            .sync_state("chat_model")?
            .or_else(|| p.default_model().map(str::to_string)),
        None => db.sync_state("chat_model")?,
    };
    Ok((provider, model))
}

/// What the proxy answers with when it doesn't recognise an install id.
///
/// Matched on rather than the sentence beside it, so the wording stays free to
/// change without stranding a released build. A block is a 403 and never this:
/// re-enrolling past one is not something to do automatically.
const UNKNOWN_INSTALL: &str = "unknown_install";

/// The bearer token a provider's requests carry, if any.
///
/// Three answers, and the difference between them is the whole point of having
/// three providers: OpenRouter takes the athlete's own key, the proxy takes an
/// id that identifies this install and nothing about the person using it, and
/// Ollama is on localhost and takes nothing.
///
/// Async for the one case that can't be answered from disk: the first hosted
/// question on a new install, where the id doesn't exist yet and has to be
/// asked for.
async fn auth_token(http: &reqwest::Client, provider: Provider) -> Result<Option<String>> {
    Ok(match provider {
        Provider::Cloud => Some(match store::stored_install_id()? {
            Some(id) => id,
            None => enroll(http).await?,
        }),
        Provider::Openrouter => Some(
            store::load_openrouter_key()?
                .context("No OpenRouter key stored. Add one in Settings.")?,
        ),
        Provider::Ollama => None,
    })
}

/// Ask the proxy for an id for this install, and keep it.
///
/// The id used to be generated here, which was simpler and meant nothing: the
/// proxy's per-install limits counted a number this side could pick again. It
/// is issued there now, rate-limited by address, so it is finite rather than
/// free. Nothing about this machine or the person on it is sent to get one —
/// the request has no body.
async fn enroll(http: &reqwest::Client) -> Result<String> {
    let resp = http
        .post(format!("{CLOUD_BASE}/install"))
        .header("X-Client-Version", env!("CARGO_PKG_VERSION"))
        .send()
        .await
        .context("could not reach the hosted coach")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        // The server's own sentence, because it is the one that knows which
        // limit was hit — this network's, or everyone's for the day. `refusal`
        // is not used here: its answers are about a question that couldn't be
        // asked, and nothing has been asked yet.
        let said = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
            .unwrap_or_else(|| format!("it returned {status}"));
        let alternative = if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            " Settings will point this at your own OpenRouter key or a local \
             Ollama, which have no such ceiling."
        } else {
            ""
        };
        return Err(anyhow!(
            "The shared coach couldn't set this install up: {said}.{alternative}"
        ));
    }

    let id = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| v["id"].as_str().map(str::to_owned))
        .context("the hosted coach answered without an id")?;

    store::save_install_id(&id)?;
    Ok(id)
}

/// Get this install's id before anything needs it.
///
/// Called when the hosted coach is chosen — at the end of setup, or when
/// Settings switches to it. Without this the first question pays for an extra
/// round trip before it can even be sent, which is a strange thing to spend on
/// the one provider whose whole pitch is that there is nothing to set up.
///
/// It only ever fills a gap. An install that already has an id makes no request
/// and gets nothing new, so this is not a way to trade one id for another no
/// matter how often it is called.
///
/// Failing is survivable and deliberately not fatal to setup: the coach may be
/// unreachable, or out of new installs for the day, and neither is a reason to
/// stand between someone and the rest of the app. The verdict is recorded where
/// the health banner reads it, and the next question asks again.
pub async fn prepare_cloud() -> Result<()> {
    if store::stored_install_id()?.is_some() {
        return Ok(());
    }

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    let enrolled = enroll(&http).await;
    note_call(
        Provider::Cloud,
        enrolled.as_ref().map(|_| ()).map_err(|e| {
            e.chain()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(": ")
        }),
    );
    enrolled.map(|_| ())
}

/// One completion request, enrolling again if the proxy has forgotten this
/// install.
///
/// The retry exists because the alternative to it is a dead end: an id the
/// server no longer holds is refused on every request, and nothing the athlete
/// can do from inside the app would change that. It costs an issue slot on the
/// server, which is what keeps it from being a way around the daily count.
async fn post_completion(
    http: &reqwest::Client,
    provider: Provider,
    key: &mut Option<String>,
    body: &Value,
) -> Result<reqwest::Response> {
    let url = format!("{}/chat/completions", provider.base());
    let send = |token: Option<&str>| authorize(http.post(&url).json(body), provider, token).send();

    let resp = send(key.as_deref())
        .await
        .context("could not reach the model")?;

    if provider != Provider::Cloud || resp.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(resp);
    }

    let status = resp.status();
    let detail = resp.text().await.unwrap_or_default();
    if !detail.contains(UNKNOWN_INSTALL) {
        return Err(anyhow!(refusal(provider, status, &detail)));
    }

    store::forget_install_id()?;
    *key = Some(enroll(http).await?);
    send(key.as_deref())
        .await
        .context("could not reach the model")
}

/// Put the bearer token and whatever else a provider wants on a request.
///
/// Shared by the streaming turn and the one-shot calls, because a header that
/// only one of them sets is a difference nobody intended — the follow-up call
/// going out unattributed while the answer above it went out attributed.
fn authorize(
    req: reqwest::RequestBuilder,
    provider: Provider,
    key: Option<&str>,
) -> reqwest::RequestBuilder {
    let Some(k) = key else { return req };
    let req = req.header("Authorization", format!("Bearer {k}"));
    match provider {
        // OpenRouter attributes requests by these; they're optional but it's
        // rude to show up anonymous on someone else's rate limit. The proxy
        // sets its own when it forwards, so sending them there says nothing.
        Provider::Openrouter => req
            .header("HTTP-Referer", "https://github.com/omznc/garmin-companion")
            .header("X-Title", "Garmin Companion"),
        // So the worker can tell which builds are still calling it, and refuse
        // one whose bug is costing money. It is a version, not a fingerprint.
        Provider::Cloud => req.header("X-Client-Version", env!("CARGO_PKG_VERSION")),
        Provider::Ollama => req,
    }
}

/* ----------------------------------------------------------------- health --- */

/// The outcome of the last request that actually went to a provider.
///
/// "Is the model working" has no cheap honest answer other than this. An active
/// probe would cost a request every time something wanted to know, and for a
/// hosted provider that is real money spent to find out whether money can be
/// spent. What the athlete needs to be told is that the thing they just asked
/// for failed and why, and that is exactly what this records.
///
/// A process-global rather than a row in the cache: it describes this run of
/// the app, not the account, and a failure that doesn't survive a restart
/// shouldn't be reported after one.
static LAST_CALL: std::sync::Mutex<Option<AiHealth>> = std::sync::Mutex::new(None);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiHealth {
    pub ok: bool,
    /// Why it failed, ready to show. `None` when the last call worked.
    pub message: Option<String>,
    /// Which provider the verdict is about — it may not be the one configured
    /// now, and a banner about a provider you have since switched away from
    /// would be worse than none.
    pub provider: &'static str,
    pub at: String,
}

fn note_call(provider: Provider, result: Result<(), String>) {
    let health = AiHealth {
        ok: result.is_ok(),
        message: result.err(),
        provider: provider.as_str(),
        at: chrono::Utc::now().to_rfc3339(),
    };
    // A poisoned lock means another thread panicked mid-write. The health of
    // the model connection is not worth propagating that panic through.
    if let Ok(mut slot) = LAST_CALL.lock() {
        *slot = Some(health);
    }
}

/// The last verdict, dropped when it is about a provider no longer selected.
pub fn health(configured: Option<Provider>) -> Option<AiHealth> {
    let last = LAST_CALL.lock().ok()?.clone()?;
    let still_current = configured.is_some_and(|p| p.as_str() == last.provider);
    still_current.then_some(last)
}

/// Whether the configured model was recorded as taking a JSON schema.
pub fn load_structured(db: &Db) -> Result<bool> {
    Ok(db.sync_state("chat_structured")?.as_deref() == Some("1"))
}

pub fn save_config(db: &Db, provider: Provider, model: &str, structured: bool) -> Result<()> {
    db.set_sync_state("chat_provider", provider.as_str())?;
    db.set_sync_state("chat_model", model)?;
    db.set_sync_state("chat_structured", if structured { "1" } else { "0" })?;
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

/* ------------------------------------------------------------------ usage --- */

/// What the model has cost so far, for one provider.
///
/// Kept because the alternative is guessing. Every answer here can take up to
/// seven requests, each re-sending the system prompt, the tool schemas and the
/// conversation, and none of that is visible from the outside — you find out
/// what a question cost by looking at a bill weeks later. These are running
/// totals in the cache's key-value table, shown in Settings and resettable
/// there, so the number is answerable at the time it's being spent.
///
/// Counted per provider, and not for tidiness: the whole question these totals
/// answer is whose money this is. OpenRouter bills the athlete, the proxy bills
/// whoever runs it, Ollama bills nobody. One shared counter meant a dollar spent
/// on your own key still read as a dollar after switching to the built-in coach
/// — the app telling you it had charged you for something it hadn't.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// Whose spending this is.
    pub provider: &'static str,
    /// Requests, not questions. One question is several of these.
    pub requests: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// Prompt tokens the provider served from its own cache, where it says so.
    /// These are billed at a fraction of the rest; see [`system_message`].
    pub cached_tokens: u64,
    /// USD, as reported by OpenRouter. Ollama reports nothing and costs
    /// nothing, so a local-only history leaves this at zero.
    pub cost_usd: f64,
    /// When counting started, so the totals mean something.
    pub since: Option<String>,
}

/// The totals as Settings needs them: whose bill is running now, and what the
/// providers you are not using ran up while you were.
///
/// `others` exists so switching provider doesn't quietly swallow the number.
/// Money spent on your own OpenRouter key stays spent after you move to the
/// built-in coach, and a panel that showed only the current provider would
/// answer "what has this cost?" with a fresh zero.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    /// The configured provider's totals. `None` before one has been chosen.
    pub current: Option<Usage>,
    /// Every other provider that has ever sent a request, in the order above.
    pub others: Vec<Usage>,
}

const PROVIDERS: [Provider; 3] = [Provider::Cloud, Provider::Openrouter, Provider::Ollama];

/// Where one provider's running total for `field` lives.
///
/// The un-suffixed `ai_*` keys an earlier version wrote are left where they
/// are and not read: a single number covering an unknown mix of providers
/// cannot be split up after the fact, and guessing which one to credit it to is
/// the bug this replaces. Counting starts again, per provider, from the next
/// request.
fn usage_key(field: &str, provider: Provider) -> String {
    format!("ai_{field}:{}", provider.as_str())
}

fn state_num<T: std::str::FromStr + Default>(db: &Db, key: &str) -> T {
    db.sync_state(key)
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default()
}

pub fn load_usage(db: &Db, provider: Provider) -> Result<Usage> {
    let n = |field: &str| -> u64 { state_num(db, &usage_key(field, provider)) };
    Ok(Usage {
        provider: provider.as_str(),
        requests: n("requests"),
        prompt_tokens: n("prompt_tokens"),
        completion_tokens: n("completion_tokens"),
        cached_tokens: n("cached_tokens"),
        cost_usd: state_num(db, &usage_key("cost_usd", provider)),
        since: db.sync_state(&usage_key("since", provider))?,
    })
}

pub fn usage_report(db: &Db) -> Result<UsageReport> {
    let configured = load_config(db)?.0;
    let mut report = UsageReport::default();
    for p in PROVIDERS {
        let totals = load_usage(db, p)?;
        if Some(p) == configured {
            report.current = Some(totals);
        } else if totals.requests > 0 {
            report.others.push(totals);
        }
    }
    Ok(report)
}

/// Clear one provider's totals. The others are somebody else's money and are
/// left alone — resetting what you can see is what the button says it does.
pub fn reset_usage(db: &Db, provider: Provider) -> Result<()> {
    for field in [
        "requests",
        "prompt_tokens",
        "completion_tokens",
        "cached_tokens",
        "cost_usd",
    ] {
        db.set_sync_state(&usage_key(field, provider), "0")?;
    }
    db.set_sync_state(
        &usage_key("since", provider),
        &chrono::Utc::now().to_rfc3339(),
    )?;
    Ok(())
}

/// Add one request's usage block to the running totals.
///
/// Opens its own connection rather than borrowing one: rusqlite's is not
/// `Sync`, and every caller is mid-`async`. Failure is swallowed — a bookkeeping
/// write is not worth losing an answer that has already been streamed to the
/// screen, and a provider that reports no usage at all is a missing number
/// rather than a broken turn.
fn record_usage(provider: Provider, usage: Option<&Value>) {
    let Some(u) = usage else { return };
    let _ = (|| -> Result<()> {
        let db = Db::open_default()?;
        let mut totals = load_usage(&db, provider)?;

        let n = |key: &str| u[key].as_u64().unwrap_or(0);
        totals.requests += 1;
        totals.prompt_tokens += n("prompt_tokens");
        totals.completion_tokens += n("completion_tokens");
        // OpenAI's spelling, which OpenRouter passes through for the providers
        // that report it. Absent means "not cached", not "unknown".
        totals.cached_tokens += u["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0);
        totals.cost_usd += u["cost"].as_f64().unwrap_or(0.0);

        let Usage {
            requests,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            cost_usd,
            since,
            ..
        } = totals;
        let put =
            |field: &str, value: String| db.set_sync_state(&usage_key(field, provider), &value);
        put("requests", requests.to_string())?;
        put("prompt_tokens", prompt_tokens.to_string())?;
        put("completion_tokens", completion_tokens.to_string())?;
        put("cached_tokens", cached_tokens.to_string())?;
        put("cost_usd", cost_usd.to_string())?;
        if since.is_none() {
            put("since", chrono::Utc::now().to_rfc3339())?;
        }
        Ok(())
    })();
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
                        "limit": { "type": "integer", "description": "How many to return. Default 10, capped at 50." },
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
                "name": "list_tags",
                "description": "The labels the athlete has put on their own sessions, with how many carry each. Call this before tagged_activities so you use a tag that exists.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "tagged_activities",
                "description": "Activities carrying one of the athlete's own tags, newest first, with the same fields as recent_activities. Use when they ask about a group they named themselves, e.g. 'how do my tempo sessions compare'.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tag": { "type": "string", "description": "The tag, exactly as list_tags reported it." },
                        "limit": { "type": "integer", "description": "How many to return. Default 20, capped at 50." }
                    },
                    "required": ["tag"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "zone_drift",
                "description": "Hard-effort drift across recent runs, plus the time-weighted easy/hard split for the window. Use for whether easy runs are staying easy. Cross-check the runs behind a drift verdict with recent_activities: if they carry cadenceLock or a poor hrConfidence, the drift may be the wrist sensor rather than the training.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "description": "How many recent runs. Default 10, capped at 50." }
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
                        "count": { "type": "integer", "description": "How many recent runs. Default 10, capped at 50." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "recovery",
                "description": "Resting HR, HRV, training readiness, sleep, stress and body battery by day. Use before advising a hard session. The window is by date and ends today, so days the watch wasn't worn are simply absent — an empty result means nothing was recorded, not that recovery was poor. Check restingHrSource before trending resting HR, and count the days carrying hrvLastNight before trusting an HRV status: Garmin needs about four nights a week to keep the personal baseline that 'Balanced' is measured against.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "days": { "type": "integer", "description": "How many days back from today. Default 14, capped at 365." }
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
                        "days": { "type": "integer", "description": "How many days back. Default 30, capped at 365." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "weight",
                "description": "Body weight over time: every weigh-in, a smoothed trend line, rate of change per week, BMI, and a comparison of the logged calorie balance against what the scale actually did. Weigh-ins are irregular — most days have none. A point with outlier:true is a mis-entry, excluded from the trend.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "days": { "type": "integer", "description": "How many days back. Default 180, capped at 730." }
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
                "name": "strength_sessions",
                "description": "Strength sessions set by set: work sets, reps, time working, rest between sets and the work:rest ratio. There is NO load in this data — the watch cannot know the weight on the bar — so never discuss volume in kilograms or progression by weight. Exercise names are the watch guessing from wrist motion, are absent for most sets, and must be described as guesses.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "How many sessions. Default 10, capped at 50." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "personal_records",
                "description": "Garmin's personal records across every sport: fastest distances, longest run, step records. A record with a null label is one the app doesn't recognise — skip it rather than guessing what it measures.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "fitness",
                "description": "Garmin's own verdict: training status, acute and chronic load, the acute:chronic ratio, the monthly aerobic/anaerobic load balance against Garmin's target ranges, VO2 max and race predictions. Use alongside zone_drift — this is Garmin answering 'is the balance right', and the zone numbers are a second opinion on the same question. A null VO2 max means no outdoor GPS run exists, not poor fitness.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "days": { "type": "integer", "description": "How many days of history. Default 30, capped at 365." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "coach",
                "description": "What the app's coach is saying unprompted today, and the week against the athlete's goals. Each nudge carries its numbers in evidence and how many days it has been standing. Empty means nothing is worth raising. Call this to find out what they have already been told, so a check-in builds on it rather than repeating it.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "findings",
                "description": "The deep findings — what a year of history says when asked properly, rather than what one session says. Whether fitness is moving (pace at a fixed heart rate, this account's stand-in for the VO2 max Garmin will never compute indoors), what cadence has been worth in seconds per kilometre, which recorded metric moves overnight HRV most, the easy/hard share over time, which weekday gets skipped, how rest days differ from training days, and whether a block of training has quietly stopped. Call this for 'am I getting fitter', 'what should I change', or any question about a trend rather than a session. Each finding carries `claim`, `detail`, `basis` and usually `estimate` — a bootstrap interval with `low`, `high` and `n`. Quote the interval whenever you quote the number. A finding only appears if its interval excludes zero, so an empty list means nothing cleared that bar — a normal result, not an error.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "days": { "type": "integer", "description": "How far back to look. Defaults to 365, which is what most of these need." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "cache_status",
                "description": "What's in the local cache and how current it is. `lastSync` is when the app last asked Garmin — it moves even when a watch left in a drawer had nothing to give — while `daysSinceDaily` and `stale` say how old the data itself is. The grounding turn already carries these numbers; call this only to re-check after a sync.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "draft_workout",
                "description": "Propose a structured workout for the athlete to review. This SAVES NOTHING — it returns the session as a card they can edit and then send to Garmin themselves. Call it when they ask for a session, a workout, or what to run. Check recent activities and recovery first.",
                "parameters": workout_schema()
            }
        },
        {
            "type": "function",
            "function": {
                "name": "ask_athlete",
                "description": "Put one short question to the athlete and wait for their answer, which comes back as this tool's result. This is the ONLY way to ask them anything — a question written into your reply instead is not a question, it is a turn that ends with a question mark and nobody waiting for it. Call this when the answer changes what you would advise and no tool can supply it: how much time they have today, whether a niggle is still there, whether they want this session easy or hard. Never ask for anything the cache holds: read it. Never ask permission, never ask them to confirm something you already decided, and never answer a factual question about their data with a question back. At most two per turn.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "question": { "type": "string", "description": "The question itself, one sentence, in the second person." },
                        "header": { "type": "string", "description": "A two-word label for what is being chosen, e.g. 'Time today'. Shown as a small chip above the question." },
                        "options": {
                            "type": "array",
                            "description": "Two to four answers to choose between. Concrete and distinguishable — 'about 20 minutes' and 'about 45 minutes', not 'short' and 'long'. They can always type their own instead, so there is no need for a catch-all option.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "label": { "type": "string", "description": "The button's words. A few, not a sentence." },
                                    "description": { "type": "string", "description": "Optional line under the label, where two options need telling apart." }
                                },
                                "required": ["label"]
                            }
                        },
                        "multi": { "type": "boolean", "description": "True when more than one answer can apply at once. Default false." }
                    },
                    "required": ["question", "options"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_themes",
                "description": "The custom colour themes already saved on this machine. Call before saving one, so a new theme doesn't silently overwrite an existing theme of the same name, and so 'make it warmer' can start from what is actually there.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "save_theme",
                "description": "Write a colour theme for the app. Call this when they ask for a theme, a palette, or a colour scheme. Saving does NOT switch the app to it — the theme appears in Settings > Appearance and they pick it themselves, so an experiment is free. Saving with the name of an existing theme replaces it, which is how to revise one.",
                "parameters": theme_schema()
            }
        },
        {
            "type": "function",
            "function": {
                "name": "use_theme",
                "description": "Switch the app to a saved theme, so they can see it rather than read about it. Call this straight after save_theme when they asked you to make one. Pass 'default' to put the built-in palette back.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string", "description": "As returned by save_theme or list_themes, or the literal 'default'." }
                    },
                    "required": ["slug"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "delete_theme",
                "description": "Remove a saved custom theme by its slug, as returned by list_themes. Only when they ask.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string" }
                    },
                    "required": ["slug"]
                }
            }
        }
    ])
}

/// The parameters for `save_theme`.
///
/// Seven colours, and the reason it is only seven is in `garmin_core::theme`:
/// the hairlines, the selection tint and the elevation are derived from these
/// on the frontend, because those have a correct answer and asking for them
/// only creates ways to be wrong.
///
/// The descriptions carry the one thing a model cannot infer from a field name
/// — which way round the contrast goes, and how far apart two neighbours are
/// meant to sit. Written tersely: this rides along with every question asked in
/// this app, including the ones about sleep.
fn theme_schema() -> Value {
    let hex = |what: &str| json!({ "type": "string", "description": what });
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "Short, evocative, one or two words. Reusing an existing name replaces that theme." },
            "note": { "type": "string", "description": "Up to about five words, shown beside the name, e.g. 'Cold midnight, soft blue'." },
            "appearance": {
                "type": "string",
                "enum": ["light", "dark"],
                "description": "Which one this is. A theme is one or the other; it does not follow the system."
            },
            "colors": {
                "type": "object",
                "description": "All seven required, each as #rrggbb.",
                "properties": {
                    "bg": hex("The page."),
                    "bg2": hex("The sidebar's ground. One small step from bg — away from it, so on a dark theme this is LIGHTER than bg, not darker."),
                    "fg": hex("Body text. Must be far from bg: aim for a contrast ratio of at least 12:1."),
                    "muted": hex("Secondary text. Roughly halfway between fg and bg, but still readable — at least 4.5:1 against bg."),
                    "faint": hex("Captions and asides. The quietest step that is still readable, around 3:1 against bg."),
                    "acc": hex("The one colour that isn't a grey — links, the selected marker, the tint behind icons. Must read as itself against bg AND stay legible as text on it, so around 4.5:1."),
                    "warn": hex("Warnings only. A different hue from acc, at similar contrast.")
                },
                "required": ["bg", "bg2", "fg", "muted", "faint", "acc", "warn"]
            },
            "iconTintAlpha": {
                "type": "number",
                "description": "Optional, 0-1. How strongly acc tints the back layer of the duotone icons. Omit unless asked; the default is picked from appearance. A light accent on a dark theme wants less, a dark accent on a light theme wants less."
            }
        },
        "required": ["name", "appearance", "colors"]
    })
}

/// The parameters for `draft_workout`, which is the one tool whose arguments
/// are a document rather than a couple of scalars.
///
/// Written as one flat step object instead of a `oneOf` over "step" and
/// "repeat". A union here is where small models reliably come apart, and the
/// looser schema costs nothing: `garmin_core::workout` parses strictly and
/// validates afterwards, so a malformed step comes back as a sentence saying
/// which step and what was wrong, which is a better teacher than a schema the
/// model half-followed.
///
/// The prose is kept tight on purpose. This is the largest thing in
/// [`tool_schemas`] by some way, and it is attached to every round of every
/// question, including the ones about sleep. What survives is the part a model
/// cannot infer — the units, the zone range, that a repeat holds plain steps —
/// while the explaining is left to [`repair`] and `validate`, which act on what
/// was actually sent rather than hoping it was read.
fn workout_schema() -> Value {
    // Both the top-level steps and the ones inside a repeat use this, minus the
    // fields that only make sense at the top.
    let exec = json!({
        "kind": {
            "type": "string",
            "enum": ["warmup", "interval", "recovery", "rest", "cooldown"]
        },
        "end": {
            "type": "object",
            "description": "When the step ends.",
            "properties": {
                "type": { "type": "string", "enum": ["time", "distance", "lap_button"] },
                "seconds": { "type": "number", "description": "For 'time'. 10 minutes is 600." },
                "metres": { "type": "number", "description": "For 'distance'. 5k is 5000." }
            },
            "required": ["type"]
        },
        "target": {
            "type": "object",
            "description": "What to hold. Prefer hr_zone — a zone number stays right when the zones are retuned. Omit for none.",
            "properties": {
                "type": { "type": "string", "enum": ["none", "hr_zone", "bpm"] },
                "zone": { "type": "integer", "description": "1-5, for 'hr_zone'." },
                "low": { "type": "number", "description": "For 'bpm'." },
                "high": { "type": "number", "description": "For 'bpm'." }
            },
            "required": ["type"]
        },
        "note": {
            "type": "string",
            "description": "One short instruction for the watch, under 200 characters."
        }
    });

    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "Short and specific, e.g. '4 x 3min Z4'." },
            "sport": { "type": "string", "enum": ["running", "cycling", "cardio", "strength_training"] },
            "description": { "type": "string", "description": "One line on what the session is for." },
            "steps": {
                "type": "array",
                "description": "The workout in order. Use a repeat for anything done more than once — never write out every rep.",
                "items": {
                    "type": "object",
                    "properties": {
                        "type": {
                            "type": "string",
                            "enum": ["exec", "repeat"],
                            "description": "'exec' is one step, 'repeat' a block."
                        },
                        "times": { "type": "integer", "description": "For 'repeat'. At least 2." },
                        "steps": {
                            "type": "array",
                            "description": "For 'repeat': the steps in the block, each untagged. No nested repeats.",
                            "items": { "type": "object", "properties": exec, "required": ["kind", "end"] }
                        },
                        "kind": exec["kind"],
                        "end": exec["end"],
                        "target": exec["target"],
                        "note": exec["note"]
                    },
                    "required": ["type"]
                }
            }
        },
        "required": ["name", "sport", "steps"]
    })
}

/// A human-readable label for what a tool call is about to read, shown in the
/// UI while the model works. Users should be able to see which of their data
/// a question actually touched.
///
/// The window arguments are read through [`window`] here as well as in
/// [`run_tool`], so the label says what was actually read rather than what was
/// asked for — "Reading your last 50 activities" under a request for 5000.
fn describe(name: &str, args: &Value) -> String {
    match name {
        "recent_activities" => {
            let n = window(args, "limit", 10, MAX_ACTIVITIES);
            match args["sport"].as_str() {
                Some(s) => format!("Reading your last {n} {s} sessions"),
                None => format!("Reading your last {n} activities"),
            }
        }
        "activity_zones" => "Reading one activity's zone breakdown".into(),
        "list_tags" => "Reading the tags you've used".into(),
        "tagged_activities" => match args["tag"].as_str() {
            Some(t) => format!("Reading your “{t}” sessions"),
            None => "Reading tagged sessions".into(),
        },
        "zone_drift" => "Checking hard-effort drift across recent runs".into(),
        "cadence_trend" => "Checking cadence across recent runs".into(),
        "recovery" => {
            let d = window(args, "days", 14, MAX_DAYS);
            format!("Reading {d} days of recovery signals")
        }
        "nutrition" => {
            let d = window(args, "days", 30, MAX_DAYS);
            format!("Reading {d} days of food and hydration")
        }
        "weight" => {
            let d = window(args, "days", 180, MAX_WEIGHT_DAYS);
            format!("Reading {d} days of weigh-ins")
        }
        "workouts" => "Reading your saved Garmin workouts".into(),
        "routes" => "Reading your cached GPS routes".into(),
        "strength_sessions" => "Reading your strength sessions set by set".into(),
        "personal_records" => "Reading your personal records".into(),
        "fitness" => "Reading Garmin's training status and load balance".into(),
        "coach" => "Checking what the coach is already saying".into(),
        "findings" => "Reading a year of it properly".into(),
        "cache_status" => "Checking what's cached".into(),
        "draft_workout" => "Putting a session together".into(),
        "ask_athlete" => "Waiting on your answer".into(),
        "list_themes" => "Reading your saved themes".into(),
        "save_theme" => match args["name"].as_str() {
            Some(n) => format!("Mixing “{n}”"),
            None => "Mixing a theme".into(),
        },
        "use_theme" => "Putting a theme on".into(),
        "delete_theme" => "Removing a theme".into(),
        other => format!("Running {other}"),
    }
}

/// What one tool call produced.
///
/// The two fields go to different places. `result` is what the model reads
/// next; `draft` is what the athlete gets offered. Keeping the proposed workout
/// out of the model's return path is the reason this is a struct — the card the
/// athlete confirms is built from the draft that was validated here, not from
/// prose the model wrote about it afterwards.
struct ToolOutput {
    result: Value,
    draft: Option<garmin_core::workout::WorkoutDraft>,
    /// Set by the tools that touch the themes folder, so the turn can tell the
    /// rest of the app. Without it a theme the model just wrote is on disk and
    /// nowhere else until something reloads the window — which is not a thing
    /// anyone should have to know to do.
    themes: Option<ThemeChange>,
}

/// What changed about themes, as broadcast to every screen.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeChange {
    /// The palette to switch to. `None` means the folder changed but the
    /// selection didn't; `Some("")` means go back to the built-in one.
    apply: Option<String>,
}

impl From<Value> for ToolOutput {
    fn from(result: Value) -> Self {
        Self {
            result,
            draft: None,
            themes: None,
        }
    }
}

/// Straighten out the shapes models reach for instead of the documented one.
///
/// The same problem `parse_followups` has, one level deeper: a schema is a
/// strong hint, not a constraint, and the smaller the model the more it drifts.
/// Every repair here is something observed rather than imagined — a tagged
/// union sent without its tag, an array sent as a string of JSON, `meters` for
/// `metres`. All of them are unambiguous, because the tag is recoverable from
/// which fields are present.
///
/// This is not the same thing as being lax about what a workout is. Repair only
/// re-labels; `validate` still runs afterwards and still rejects anything that
/// would land wrong on a watch. The schema keeps asking for the correct shape,
/// because a better model reads it and gets it right the first time.
fn repair(args: &Value) -> Value {
    let mut v = args.clone();

    if let Some(steps) = v.get_mut("steps") {
        unstring(steps);
        // A single step sent bare rather than as a list of one.
        if steps.is_object() {
            *steps = Value::Array(vec![steps.clone()]);
        }
        if let Some(arr) = steps.as_array_mut() {
            for step in arr {
                repair_step(step);
            }
        }
    }
    v
}

/// A JSON array or object that arrived as a string of JSON. Models do this to
/// any argument whose type they've decided is "text".
fn unstring(v: &mut Value) {
    if let Some(s) = v.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Value>(s) {
            if parsed.is_array() || parsed.is_object() {
                *v = parsed;
            }
        }
    }
}

fn repair_step(step: &mut Value) {
    let Some(obj) = step.as_object_mut() else {
        return;
    };

    // `times` is the field only a repeat has, so its presence is the tag.
    if !obj.contains_key("type") {
        let tag = if obj.contains_key("times") {
            "repeat"
        } else {
            "exec"
        };
        obj.insert("type".into(), json!(tag));
    }

    if obj.get("type").and_then(Value::as_str) == Some("repeat") {
        if let Some(inner) = obj.get_mut("steps") {
            unstring(inner);
            if let Some(arr) = inner.as_array_mut() {
                // Children are plain steps: no tag of their own to infer.
                for child in arr {
                    repair_exec(child);
                }
            }
        }
        return;
    }
    repair_exec(step);
}

fn repair_exec(step: &mut Value) {
    let Some(obj) = step.as_object_mut() else {
        return;
    };
    if let Some(end) = obj.get_mut("end") {
        repair_end(end);
    }
    if let Some(target) = obj.get_mut("target") {
        repair_target(target);
    }
}

fn repair_end(end: &mut Value) {
    // A bare number where an object was asked for. Seconds, per the schema.
    if let Some(n) = end.as_f64() {
        *end = json!({ "type": "time", "seconds": n });
        return;
    }
    let Some(obj) = end.as_object_mut() else {
        return;
    };

    // The spelling repairs run whether or not the tag is present: a step tagged
    // `distance` carrying `meters` is just as unreadable as an untagged one.
    if let Some(m) = obj.remove("meters") {
        obj.entry("metres").or_insert(m);
    }
    // Minutes are what intervals are spoken in, so a model writes them even
    // where the field says seconds. Converting is safe — the two units are
    // never both present, and 3 can only mean three minutes.
    if let Some(mins) = obj.remove("minutes").and_then(|m| m.as_f64()) {
        obj.entry("seconds").or_insert(json!(mins * 60.0));
    }

    if obj.contains_key("type") {
        return;
    }
    let tag = if obj.contains_key("seconds") {
        "time"
    } else if obj.contains_key("metres") {
        "distance"
    } else {
        "lap_button"
    };
    obj.insert("type".into(), json!(tag));
}

fn repair_target(target: &mut Value) {
    // A bare zone number, or "Z4" / "hr_zone" / "none" as a string.
    if let Some(n) = target.as_u64() {
        *target = json!({ "type": "hr_zone", "zone": n });
        return;
    }
    if let Some(s) = target.as_str() {
        let zone = s
            .strip_prefix(['z', 'Z'])
            .and_then(|n| n.parse::<u64>().ok());
        *target = match zone {
            Some(z) => json!({ "type": "hr_zone", "zone": z }),
            None => json!({ "type": s }),
        };
        return;
    }

    let Some(obj) = target.as_object_mut() else {
        return;
    };
    if obj.contains_key("type") {
        return;
    }
    let tag = if obj.contains_key("zone") {
        "hr_zone"
    } else if obj.contains_key("low") && obj.contains_key("high") {
        "bpm"
    } else {
        "none"
    };
    obj.insert("type".into(), json!(tag));
}

/// Turn the model's arguments into a workout, or into a complaint it can act on.
///
/// Both failure modes end up as an `error` string in the tool result rather
/// than as a Rust error, because both are recoverable by the model calling the
/// tool again. The serde message is passed through as-is: "missing field
/// `times`" is genuinely the most useful sentence available.
fn draft_workout(args: &Value) -> ToolOutput {
    let draft: garmin_core::workout::WorkoutDraft = match serde_json::from_value(repair(args)) {
        Ok(d) => d,
        Err(e) => {
            return json!({
                "error": format!(
                    "that isn't a workout this app can build: {e}. Every step needs \
                     `kind` and `end`; `end` is one of {{\"type\":\"time\",\"seconds\":N}}, \
                     {{\"type\":\"distance\",\"metres\":N}} or {{\"type\":\"lap_button\"}}. \
                     Send `steps` as a JSON array, not as a string.",
                ),
            })
            .into()
        }
    };

    if let Err(e) = draft.validate() {
        return json!({ "error": e.to_string() }).into();
    }

    // Deliberately thin. The model does not need the steps read back — it just
    // wrote them — and a full echo is tokens spent to tempt it into listing the
    // session in prose next to the card that already shows it.
    let result = json!({
        "ok": true,
        "name": draft.name,
        "steps": draft.flat_count(),
        "estimatedMinutes": draft.est_duration_secs().map(|s| (s / 60.0).round()),
        "status": "Shown to the athlete for review. Nothing has been saved to \
                   Garmin, and nothing will be unless they press the button on it.",
    });
    ToolOutput {
        result,
        draft: Some(draft),
        themes: None,
    }
}

/* --------------------------------------------------------------- asking --- */

/// One answer the athlete can pick, as the model wrote it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskOption {
    /// What the button says. Short — it is a button, not a sentence.
    pub label: String,
    /// The line under it, where the difference between two options needs
    /// spelling out. Optional, and usually should be.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The arguments `ask_athlete` takes, once they are shaped like a question.
struct Ask {
    header: Option<String>,
    question: String,
    options: Vec<AskOption>,
    multi: bool,
}

/// Read a question out of whatever the model sent.
///
/// Forgiving in the same places [`repair`] is, and for the same reason: the
/// small models this app is pointed at send `options` as a JSON-encoded string
/// about as often as they send an array, and as bare strings about as often as
/// objects. A question that arrives in the wrong shape and is refused costs a
/// round trip to be told something the parser could have worked out.
fn parse_ask(args: &Value) -> Result<Ask, String> {
    let question = args["question"]
        .as_str()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or("ask_athlete needs a `question` to put to them")?
        .to_string();

    // A string here is a nested JSON array that arrived encoded.
    let raw = match &args["options"] {
        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        other => other.clone(),
    };

    let options: Vec<AskOption> = raw
        .as_array()
        .map(|xs| {
            xs.iter()
                .filter_map(|x| match x {
                    Value::String(s) => Some(AskOption {
                        label: s.clone(),
                        description: None,
                    }),
                    Value::Object(_) => serde_json::from_value(x.clone()).ok(),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    if options.len() < 2 {
        return Err(
            "ask_athlete needs at least two `options` to choose between. \
                    If there is nothing to choose, don't ask — decide, and say \
                    what you assumed."
                .into(),
        );
    }

    Ok(Ask {
        header: args["header"]
            .as_str()
            .map(str::trim)
            .filter(|h| !h.is_empty())
            // A chip, not a label. Anything longer than this is the question
            // repeating itself in a space that can't hold it.
            .map(|h| h.chars().take(16).collect()),
        question,
        // Four is what the card lays out; past that the model is offering a
        // menu, and the two it thinks matter are the ones worth showing.
        options: options.into_iter().take(4).collect(),
        multi: args["multi"].as_bool().unwrap_or(false),
    })
}

/// Put a question on screen and park the turn until it comes back.
///
/// This is the one tool that waits on a person. Everything else here reads a
/// local table and returns in microseconds; this emits an event, hands its end
/// of a channel to [`LIVE`] where the `chat_answer` command can find it, and
/// awaits.
///
/// Three ways it stops waiting, and the model is told which: an answer, the
/// timeout, or the sender being dropped — which is Stop, or the turn ending
/// underneath it. The last two return `answered: false` rather than an error,
/// because a question nobody answered is not a failure the model should retry.
/// It should write the answer it would have written anyway and say what it
/// assumed.
async fn ask_athlete<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    channel: &str,
    call_id: &str,
    args: &Value,
    used: &mut usize,
) -> Value {
    if *used >= MAX_ASKS_PER_TURN {
        return json!({
            "error": format!(
                "you have already asked {MAX_ASKS_PER_TURN} questions this turn, \
                 which is the limit. Decide from what you have and state the \
                 assumption you made in your answer.",
            ),
        });
    }

    let ask = match parse_ask(args) {
        Ok(a) => a,
        Err(e) => return json!({ "error": e }),
    };
    *used += 1;

    let (tx, rx) = oneshot::channel();
    // Registered before the event goes out. The other order is a race the
    // frontend wins on a fast machine: an answer arriving for a question with
    // nowhere to deliver it is dropped, and the turn then waits ten minutes for
    // a click that already happened.
    let registered = with_live(|live| match live.get_mut(id) {
        Some(turn) => {
            turn.asks.insert(call_id.to_string(), tx);
            true
        }
        None => false,
    });
    if !registered {
        return json!({ "answered": false, "reason": "this turn is no longer live" });
    }

    emit(
        app,
        channel,
        Event::Ask {
            call_id,
            header: ask.header.as_deref(),
            question: &ask.question,
            options: ask.options.clone(),
            multi: ask.multi,
        },
    );

    let waited = tokio::time::timeout(std::time::Duration::from_secs(ASK_TIMEOUT_SECS), rx).await;

    // However it ended, this question is no longer open. Cheap when the answer
    // came through `answer_ask`, which already took it out.
    with_live(|live| {
        if let Some(turn) = live.get_mut(id) {
            turn.asks.remove(call_id);
        }
    });

    match waited {
        Ok(Ok(answers)) => json!({
            "answered": true,
            "question": ask.question,
            "answers": answers,
            "note": "This is the athlete's own answer. Use it, and don't ask it again.",
        }),
        Ok(Err(_)) => json!({
            "answered": false,
            "reason": "they stopped the answer or closed it before choosing",
        }),
        Err(_) => json!({
            "answered": false,
            "reason": "they didn't answer in time",
        }),
    }
}

/// The theme tools, or `None` when `name` isn't one of them.
///
/// `save_theme` is the only tool in this app that writes anything, and it is
/// worth being precise about what that means. It writes a JSON file into a
/// folder the athlete can open, and nothing else: the app does not switch to
/// the theme, no screen changes, and the file is deleted by dragging it to the
/// bin. The reason `draft_workout` stops short of saving is that a workout
/// lands on a watch and shapes a session; a theme that nobody selects is an
/// unopened file. So the model gets to finish the job.
///
/// Every failure comes back as an `error` string rather than as a Rust error,
/// because every one of them is recoverable by calling again with better
/// arguments — and `Theme::validate` already says which field and why.
fn run_theme_tool(name: &str, args: &Value) -> Option<ToolOutput> {
    use garmin_core::theme;

    // What to broadcast afterwards. Set by the arms that changed something;
    // `list_themes` leaves it alone, because reading is not a change.
    let mut themes: Option<ThemeChange> = None;

    let result: Result<Value> = match name {
        "list_themes" => theme::list().map(|themes| {
            json!({
                // Deliberately without the colours. The model is choosing a
                // name to revise or delete, and seven hex strings per theme is
                // a page of tokens spent on a decision none of them inform.
                "themes": themes
                    .iter()
                    .map(|t| json!({
                        "slug": t.slug,
                        "name": t.name,
                        "appearance": t.appearance,
                        "note": t.note,
                    }))
                    .collect::<Vec<_>>(),
            })
        }),

        "save_theme" => (|| {
            let theme: theme::Theme = serde_json::from_value(args.clone()).map_err(|e| {
                anyhow!(
                    "that isn't a theme this app can build: {e}. Send \
                     `colors` as an object with all seven of bg, bg2, fg, \
                     muted, faint, acc and warn, each a #rrggbb string."
                )
            })?;
            let saved = theme::save(theme)?;
            // The list changed, so every screen showing it needs to know — but
            // not the selection. Applying is `use_theme`, one call away.
            themes = Some(ThemeChange { apply: None });
            Ok(json!({
                "ok": true,
                "slug": saved.slug,
                "name": saved.name,
                "status": "Saved, and now listed under Settings > Appearance. \
                           The app has NOT switched to it — call use_theme with \
                           this slug if they should see it.",
            }))
        })(),

        // The one that changes what's on screen. Checked against the folder
        // rather than taken on trust: a slug the model half-remembered would
        // otherwise leave the app pointing at a theme that doesn't exist.
        "use_theme" => (|| {
            let slug = args["slug"]
                .as_str()
                .ok_or_else(|| anyhow!("use_theme needs a `slug`"))?;

            if slug == "default" {
                themes = Some(ThemeChange {
                    apply: Some(String::new()),
                });
                return Ok(
                    json!({ "ok": true, "status": "Switched back to the built-in palette." }),
                );
            }

            let found = theme::list()?.into_iter().find(|t| t.slug == slug);
            let Some(found) = found else {
                return Ok(json!({
                    "error": format!(
                        "no theme with slug {slug:?}. Call list_themes for the \
                         ones that exist, or `default` to go back to the built-in one."
                    ),
                }));
            };
            themes = Some(ThemeChange {
                apply: Some(found.slug.clone()),
            });
            Ok(json!({
                "ok": true,
                "name": found.name,
                "status": "The app is wearing it now. One click in Settings > \
                           Appearance puts back whatever they had.",
            }))
        })(),

        "delete_theme" => match args["slug"].as_str() {
            Some(slug) => theme::delete(slug).map(|()| {
                themes = Some(ThemeChange { apply: None });
                json!({ "ok": true })
            }),
            None => Ok(json!({ "error": "delete_theme needs the theme's `slug`" })),
        },

        _ => return None,
    };

    // A failed call changed nothing, whatever the arm managed to set first.
    let ok = result.is_ok();
    Some(ToolOutput {
        result: result.unwrap_or_else(|e| json!({ "error": e.to_string() })),
        draft: None,
        themes: themes.filter(|_| ok),
    })
}

/// How much of a table one tool call may read.
///
/// Missing means the default; anything else is held between 1 and `max`. A
/// model asking for a thousand days is not asking a different question, it is
/// guessing at a number, and the cost of the guess lands on every remaining
/// round of the turn. Clamping silently is deliberate: an error here would burn
/// a round teaching the model an argument it did not need.
fn window(args: &Value, key: &str, default: u64, max: u64) -> u32 {
    args[key].as_u64().unwrap_or(default).clamp(1, max) as u32
}

/// Runs one tool against the cache. Errors come back as JSON rather than
/// aborting the turn — the model can say "that failed" far better than a
/// stack trace can.
fn run_tool(name: &str, args: &Value) -> ToolOutput {
    // Handled before the cache is opened: it is the one tool that reads nothing.
    if name == "draft_workout" {
        return draft_workout(args);
    }

    // Same, for a different reason — themes are files in a folder, and none of
    // the athlete's data is involved either way.
    if let Some(out) = run_theme_tool(name, args) {
        return out;
    }

    let db = match Db::open_default() {
        Ok(db) => db,
        Err(e) => return json!({ "error": format!("could not open the cache: {e}") }).into(),
    };

    let result: Result<Value> = (|| {
        Ok(match name {
            "recent_activities" => serde_json::to_value(query::recent_activities(
                &db,
                window(args, "limit", 10, MAX_ACTIVITIES),
                args["sport"].as_str(),
            )?)?,
            "activity_zones" => match query::activity_zones(&db, args["activity_id"].as_i64())? {
                Some(v) => serde_json::to_value(v)?,
                None => json!({ "error": "no such activity in the cache" }),
            },
            "zone_drift" => serde_json::to_value(query::zone_drift(
                &db,
                window(args, "count", 10, MAX_ACTIVITIES),
            )?)?,
            "cadence_trend" => serde_json::to_value(query::cadence_trend(
                &db,
                window(args, "count", 10, MAX_ACTIVITIES),
            )?)?,
            "recovery" => {
                serde_json::to_value(query::recovery(&db, window(args, "days", 14, MAX_DAYS))?)?
            }
            "nutrition" => {
                serde_json::to_value(query::nutrition(&db, window(args, "days", 30, MAX_DAYS))?)?
            }
            "weight" => serde_json::to_value(query::weight(
                &db,
                window(args, "days", 180, MAX_WEIGHT_DAYS),
            )?)?,
            "list_tags" => serde_json::to_value(
                db.all_tags()?
                    .into_iter()
                    .map(|(tag, count)| json!({ "tag": tag, "activities": count }))
                    .collect::<Vec<_>>(),
            )?,
            "tagged_activities" => match args["tag"].as_str() {
                Some(tag) => {
                    let activities =
                        db.activities_with_tag(tag, window(args, "limit", 20, MAX_ACTIVITIES))?;
                    // Rendered through the same view `recent_activities`
                    // returns, so the model reads one activity format and not
                    // two that happen to describe the same rows.
                    serde_json::to_value(
                        activities
                            .iter()
                            .map(query::ActivityView::from)
                            .collect::<Vec<_>>(),
                    )?
                }
                None => json!({ "error": "tagged_activities needs a tag" }),
            },
            "workouts" => serde_json::to_value(db.workouts()?)?,
            "routes" => serde_json::to_value(query::route_summaries(&db)?)?,
            "strength_sessions" => serde_json::to_value(query::strength_trend(
                &db,
                window(args, "limit", 10, MAX_ACTIVITIES),
            )?)?,
            "personal_records" => serde_json::to_value(query::personal_records(&db)?)?,
            "fitness" => {
                serde_json::to_value(query::fitness(&db, window(args, "days", 30, MAX_DAYS))?)?
            }
            "coach" => serde_json::to_value(garmin_core::coach::for_today(
                &db,
                chrono::Local::now().date_naive(),
            )?)?,
            "findings" => {
                let days = window(args, "days", 365, MAX_DAYS);
                let from = garmin_core::days_ago(days);
                serde_json::to_value(garmin_core::findings::all(
                    &db.daily_since(&from)?,
                    &db.activities_since(&from)?,
                    chrono::Local::now().date_naive(),
                ))?
            }
            "cache_status" => serde_json::to_value(query::cache_status(&db)?)?,
            other => json!({ "error": format!("unknown tool {other}") }),
        })
    })();

    result
        .unwrap_or_else(|e| json!({ "error": e.to_string() }))
        .into()
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
    /// One tool call, twice: once when it starts and once when it comes back.
    ///
    /// Two events rather than the single replaceable status line this used to
    /// be, because a turn reads three or four things and the interesting part is
    /// the sequence — which of your data it went to, in what order, and how long
    /// each took. A line that is overwritten by the next one can't show that,
    /// and it can't show a call that finished either.
    Tool {
        call_id: &'a str,
        label: &'a str,
        /// `false` once the call has come back; `ok` is meaningless until then.
        running: bool,
        ok: bool,
    },
    Delta {
        text: &'a str,
    },
    /// A workout the model has proposed. Sent the moment it validates rather
    /// than held for `Done`, so the card is on screen while the paragraph
    /// explaining it is still arriving.
    Draft {
        draft: garmin_core::workout::WorkoutDraft,
    },
    /// A question the model has put to the athlete. The turn is parked from the
    /// moment this goes out until [`answer_ask`] resolves it — see `ask`.
    Ask {
        call_id: &'a str,
        header: Option<&'a str>,
        question: &'a str,
        options: Vec<AskOption>,
        multi: bool,
    },
    Done {
        sources: Vec<String>,
    },
    Error {
        text: String,
    },
}

fn emit<R: Runtime>(app: &AppHandle<R>, channel: &str, event: Event<'_>) {
    // A failed emit means the window went away mid-turn; there is nobody left
    // to tell, so dropping it is the whole correct response.
    let _ = app.emit(channel, event);
}

/* ----------------------------------------------------------- live turns --- */

/// Every turn currently running, by the id the frontend opened it with.
///
/// Two things reach into a turn from outside it: Stop, and the answer to a
/// question the model asked. Both arrive as separate Tauri commands on separate
/// tasks while `run_turn` is still awaiting somewhere inside itself, so there
/// has to be somewhere to address — hence a registry rather than state threaded
/// through the call.
///
/// `std::sync::Mutex` and not tokio's: nothing here is held across an await, and
/// the two operations are a map lookup and a channel send.
static LIVE: std::sync::Mutex<Option<HashMap<String, TurnHandle>>> = std::sync::Mutex::new(None);

/// The outside world's handle on a running turn.
#[derive(Default)]
struct TurnHandle {
    /// Set by [`cancel`]. Read between rounds, between tool calls and between
    /// stream chunks — the three places a turn can be interrupted without
    /// leaving the provider mid-request.
    stop: Arc<AtomicBool>,
    /// Questions put to the athlete and not yet answered, by tool call id.
    /// Dropping a sender is how an unanswerable question ends: the parked
    /// receiver errors and the tool reports that nothing came back.
    asks: HashMap<String, oneshot::Sender<Vec<String>>>,
}

fn with_live<T>(f: impl FnOnce(&mut HashMap<String, TurnHandle>) -> T) -> T {
    let mut guard = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// Answer a question the model asked. False if it has already been answered,
/// timed out, or belongs to a turn that is over — all of which are races the
/// frontend can lose by a click, and none of which are errors worth showing.
pub fn answer_ask(id: &str, call_id: &str, answers: Vec<String>) -> bool {
    let sender = with_live(|live| live.get_mut(id).and_then(|t| t.asks.remove(call_id)));
    match sender {
        Some(tx) => tx.send(answers).is_ok(),
        None => false,
    }
}

/// Stop a turn. It ends at the next interruption point with whatever prose has
/// already been streamed, which the frontend keeps — a stopped answer is a short
/// answer, not a discarded one.
pub fn cancel(id: &str) {
    with_live(|live| {
        if let Some(t) = live.get_mut(id) {
            t.stop.store(true, Ordering::Relaxed);
            // Unblocks a turn parked on a question. The senders are dropped
            // rather than sent an empty answer, so the tool can tell "they
            // pressed Stop" from "they chose nothing".
            t.asks.clear();
        }
    });
}

/// Runs one assistant turn: tool rounds until the model stops asking for them,
/// then streams the answer. Progress and text both arrive on `chat:{id}`.
pub async fn run_turn<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    history: Vec<HistoryMessage>,
    context: Option<&garmin_core::ActivityAnalysis>,
) -> Result<()> {
    let channel = format!("chat:{id}");

    // Registered before the first request and removed on every exit path below,
    // including the error one — a stale entry would let Stop and an answer
    // address a turn that is no longer listening.
    let stop = Arc::new(AtomicBool::new(false));
    with_live(|live| {
        live.insert(
            id.clone(),
            TurnHandle {
                stop: Arc::clone(&stop),
                asks: HashMap::new(),
            },
        )
    });
    let _guard = LiveGuard(id.clone());

    // The provider this turn was aimed at, for the health note below. Resolved
    // separately because a turn that fails before it reads the config has no
    // provider to blame, and blaming the wrong one is worse than saying nothing.
    let provider = Db::open_default()
        .ok()
        .and_then(|db| load_config(&db).ok())
        .and_then(|(p, _)| p);

    match turn_inner(&app, &id, &channel, &stop, history, context).await {
        Ok(sources) => {
            if let Some(p) = provider {
                note_call(p, Ok(()));
            }
            emit(&app, &channel, Event::Done { sources });
            Ok(())
        }
        Err(e) => {
            let text = e
                .chain()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(": ");
            if let Some(p) = provider {
                note_call(p, Err(text.clone()));
            }
            emit(&app, &channel, Event::Error { text: text.clone() });
            Err(anyhow!(text))
        }
    }
}

/// Takes the turn out of [`LIVE`] however it ends, including a panic on the way
/// through — this is the one piece of bookkeeping where being wrong is silent.
struct LiveGuard(String);

impl Drop for LiveGuard {
    fn drop(&mut self) {
        with_live(|live| live.remove(&self.0));
    }
}

/// The system turn, carrying a cache breakpoint where one is worth having.
///
/// The system prompt and the tool schemas are the same bytes on every round of
/// every question — about two thousand tokens of fixed prefix, re-sent up to
/// seven times a turn. Anthropic models cache a prefix only when asked to, and
/// OpenRouter passes `cache_control` straight through; tools are rendered ahead
/// of the system prompt there, so this one breakpoint covers both.
///
/// It is a hint, not a guarantee. A cached read is billed at about a tenth of
/// the input price and a write at rather more than one, so it pays from the
/// second request onward and this turn makes at least two. Models that cache
/// automatically (OpenAI, DeepSeek, Grok) ignore the marker and cache anyway,
/// and models whose minimum cacheable prefix is longer than ours ignore it and
/// don't — the request is correct either way, which is why nothing here checks.
///
/// Ollama gets the plain string. It is local and free, so there is nothing to
/// save, and its OpenAI shim has no reason to be handed a content array.
///
/// The proxy gets what OpenRouter gets, because it is OpenRouter one hop later
/// — and there the saving is the project's rather than the athlete's, which if
/// anything makes it matter more.
fn system_message(provider: Provider) -> Value {
    match provider {
        Provider::Cloud | Provider::Openrouter => json!({
            "role": "system",
            "content": [{
                "type": "text",
                "text": SYSTEM_PROMPT,
                "cache_control": { "type": "ephemeral" },
            }],
        }),
        Provider::Ollama => json!({ "role": "system", "content": SYSTEM_PROMPT }),
    }
}

/// How much of the conversation the model is given.
///
/// The screen holds the whole transcript and always will — it is the record of
/// what was said, and reopening a conversation should show all of it. What goes
/// back to the model is a different question: the array is re-sent on every
/// round, so a session that never forgets costs roughly the square of its own
/// length, and the middle of a long conversation is the part least likely to be
/// load-bearing.
///
/// So: the opening question, which is what the conversation is about, and then
/// as much of the tail as fits. The question just asked is never dropped, no
/// matter how long it is.
fn trim_history(history: Vec<HistoryMessage>) -> Vec<HistoryMessage> {
    let total: usize = history.iter().map(|m| m.content.len()).sum();
    if history.len() <= HISTORY_MESSAGES && total <= HISTORY_CHARS {
        return history;
    }

    let mut start = history.len();
    let mut chars = 0usize;
    for (i, m) in history.iter().enumerate().rev() {
        chars += m.content.len();
        if history.len() - i > HISTORY_MESSAGES || chars > HISTORY_CHARS {
            break;
        }
        start = i;
    }
    // One message longer than the whole budget still gets sent: it's the thing
    // being asked, and a turn with no question in it is worse than a big one.
    start = start.min(history.len() - 1);

    // A tail that opens on an answer reads as a reply to nothing, and some
    // providers reject a conversation whose first turn isn't the user's.
    while start < history.len() - 1 && history[start].role != "user" {
        start += 1;
    }

    let mut out = Vec::with_capacity(history.len() - start + 1);
    if start > 0 {
        out.push(history[0].clone());
    }
    out.extend(history[start..].iter().cloned());
    out
}

/// Today's date, as one line, for the one-shot prompts.
///
/// They get data handed to them rather than tools to fetch it, so they can't
/// fall for a stale window — but every row they're given is dated, and without
/// this they have no way to tell last night's session from one in March. The
/// tense of the sentence they write depends on it.
fn now_line() -> String {
    format!(
        "Today is {}. Every date in the data below is real; work out how long \
         ago it was before writing about it as though it were recent.\n\n",
        chrono::Local::now().format("%A %-d %B %Y"),
    )
}

/// Where and when the conversation is happening, and what the cache can
/// currently support.
///
/// A third system turn, for the same reason [`context_message`] is a second
/// one: [`SYSTEM_PROMPT`] carries the cache breakpoint and is the same bytes on
/// every request in the app, and this changes every minute. Splicing it in
/// there would miss the prompt cache on every single turn.
///
/// It exists because a model has no clock. Everything the tools return is
/// stamped with a real date, which reads as current to something with no idea
/// what today is — so a watch that spent two months in a drawer produced
/// answers about "this week" built on data from spring, in the model's own
/// confident present tense. Nothing was invented; it simply had no way to
/// notice. The freshness lines are the fix, and they lead because that is the
/// failure they exist to prevent.
///
/// The rest is the standing context a coach would already know and shouldn't
/// have to spend a tool round asking for.
fn grounding_message(db: &Db) -> anyhow::Result<Value> {
    let now = chrono::Local::now();
    let status = query::cache_status(db)?;
    let goals = garmin_core::Goals::load(db)?;

    let mut s = format!(
        "Current date and time, which you have no other way of knowing: {}, \
         local time {} (UTC{}). Any question about \"today\", \"this week\" or \
         \"recently\" is anchored here, not to whatever the newest row in the \
         cache happens to be.\n\n",
        now.format("%A %-d %B %Y"),
        now.format("%H:%M"),
        now.format("%:z"),
    );

    s.push_str("How current the cached data is:\n");
    let age = |label: &str, date: &Option<String>, days: Option<i64>| match (date, days) {
        (Some(d), Some(0)) => format!("- {label}: through {d}, which is today.\n"),
        (Some(d), Some(1)) => format!("- {label}: through {d}, which is yesterday.\n"),
        (Some(d), Some(n)) => format!("- {label}: through {d} — {n} days ago.\n"),
        _ => format!("- {label}: nothing cached.\n"),
    };
    s.push_str(&age(
        "Wellness (resting HR, HRV, sleep, readiness)",
        &status.newest_daily_date,
        status.days_since_daily,
    ));
    s.push_str(&age(
        "Activities",
        &status.newest_activity_date,
        status.days_since_activity,
    ));

    if status.stale {
        s.push_str(
            "\nThe wellness data has stopped. That is the watch not being worn, \
             not a sync fault — syncing an account whose watch is in a drawer \
             succeeds and returns nothing. Say plainly that the data stops on \
             that date, quote figures with the date they came from, and do not \
             describe the newest reading as \"this morning\" or use it to judge \
             whether they are recovered enough to train hard today. Their \
             readiness right now is unknown, and unknown is the honest answer.\n",
        );
    }

    s.push_str(
        "\nEvery tool that takes a day count windows by date, so an empty result \
         means nothing was recorded in that window — not that the athlete did \
         nothing. Those two are told apart by the dates above.\n",
    );

    let mut targets = Vec::new();
    if let Some(v) = goals.long_run_minutes {
        targets.push(format!("one long easy run of {v:.0}+ minutes a week"));
    }
    if let Some(v) = goals.easy_share_pct {
        targets.push(format!("{v:.0}% of weekly HR time easy (Z1+Z2)"));
    }
    if let Some(v) = goals.cadence_spm {
        targets.push(format!("cadence around {v:.0} spm"));
    }
    if let Some(v) = goals.weekly_minutes {
        targets.push(format!("{v:.0} training minutes a week"));
    }
    if let Some(v) = goals.weekly_sessions {
        targets.push(format!("{v} sessions a week"));
    }
    if !targets.is_empty() {
        s.push_str(&format!(
            "\nThe athlete's own standing goals, which are the app's and not \
             Garmin's: {}. Call `coach` for how the current week is going \
             against them.\n",
            targets.join(", "),
        ));
    }

    Ok(json!({ "role": "system", "content": s }))
}

/// The session a conversation is about, put in front of the model.
///
/// A second system turn rather than an addition to the first: the first is the
/// same bytes on every request in the app and carries the cache breakpoint, and
/// splicing a different activity into it would miss that cache on every turn.
///
/// It says "already been read" because the alternative is a model that opens
/// every answer by calling `activity_zones` for numbers it was handed a
/// paragraph ago.
fn context_message(analysis: &garmin_core::ActivityAnalysis) -> Value {
    json!({
        "role": "system",
        "content": format!(
            "The athlete is looking at one specific session and their questions are \
             about it unless they say otherwise. Its full analysis has already been \
             read for you and follows as JSON — do not call a tool to re-read \
             anything that is already in it. Do call your tools for anything about \
             other sessions, trends, or recovery.\n\n{}",
            for_model(analysis)
        ),
    })
}

/// What a rejected request says on screen.
///
/// The proxy's two refusals are the ones an athlete can do something about, and
/// "cloud returned 429: {}" is not the sentence that tells them what. Everything
/// else keeps the raw status and body, because an unrecognised failure is worth
/// showing verbatim rather than paraphrasing into something reassuring.
fn refusal(provider: Provider, status: reqwest::StatusCode, body: &str) -> String {
    if provider == Provider::Cloud {
        match status.as_u16() {
            429 => {
                return "The shared coach is over its limit for now — it's one \
                        pot of credit for everyone using this app. Try again in \
                        a few minutes, or point Settings at your own OpenRouter \
                        key or a local Ollama, which have no such ceiling."
                    .into()
            }
            // Both the worker's own budget ceiling and OpenRouter's, which it
            // passes through with the status intact. 503 is deliberately not
            // here: that is the proxy being down, and telling someone it is out
            // of money would send them to wait for a tomorrow that fixes
            // nothing.
            402 => {
                return "The shared coach is out of credit for today. It tops \
                        back up tomorrow — or Settings will point this at your \
                        own OpenRouter key or a local Ollama in the meantime."
                    .into()
            }
            _ => {}
        }
    }
    format!(
        "{} returned {status}: {}",
        provider.as_str(),
        body.chars().take(300).collect::<String>()
    )
}

async fn turn_inner<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    channel: &str,
    stop: &AtomicBool,
    history: Vec<HistoryMessage>,
    context: Option<&garmin_core::ActivityAnalysis>,
) -> Result<Vec<String>> {
    let db = Db::open_default()?;
    let (provider, model) = load_config(&db)?;
    let provider = provider.context("No model provider chosen yet.")?;
    let model = model.context("No model chosen yet.")?;

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;

    // After the client, because a hosted install with no id yet gets one here,
    // over the network. Mutable for the same reason: if the proxy turns out not
    // to know this id, the turn enrols again rather than failing.
    let mut key = auth_token(&http, provider).await?;

    let mut messages = vec![system_message(provider), grounding_message(&db)?];
    if let Some(a) = context {
        messages.push(context_message(a));
    }
    for m in trim_history(history) {
        messages.push(json!({ "role": m.role, "content": m.content }));
    }

    let mut sources: Vec<String> = Vec::new();
    // Whether the model has written a single character of prose this turn.
    //
    // Tracked because a turn that produced no answer and a turn that produced
    // an empty one arrive here as the same thing, and only one of them should
    // look like nothing happening. A reasoning model that spends its whole
    // token budget thinking returns exactly that — `content: null` with
    // `finish_reason: "length"` — and without this the screen shows a blank
    // reply and no reason for it.
    let mut said_anything = false;
    // Questions put to the athlete so far, against `MAX_ASKS_PER_TURN`.
    let mut asks_used = 0usize;

    for round in 0..=MAX_TOOL_ROUNDS {
        // Stop, caught between rounds. Whatever has streamed already stands as
        // the answer — a stopped turn is a short one, not a discarded one, so
        // this returns the sources it collected rather than an error.
        if stop.load(Ordering::Relaxed) {
            return Ok(sources);
        }

        // On the last permitted round, drop the tools so the model is forced to
        // answer from what it already has rather than asking for more.
        let offer_tools = round < MAX_TOOL_ROUNDS;

        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": true,
            // Without this the final chunk carries no usage block and the turn
            // costs an unknown amount, which is the state this app was in.
            "stream_options": { "include_usage": true },
        });
        if offer_tools {
            body["tools"] = tool_schemas();
        }
        if provider.hosted() {
            // OpenRouter's own accounting, which reports what the request
            // actually cost in dollars rather than leaving it to be inferred
            // from a token count and a price list that may have moved. The
            // proxy passes it back through, so a hosted question says what it
            // cost whoever paid for it.
            body["usage"] = json!({ "include": true });
        }

        let resp = post_completion(&http, provider, &mut key, &body).await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(refusal(provider, status, &text)));
        }

        let stream = stream_completion(app, channel, stop, resp).await?;
        record_usage(provider, stream.usage.as_ref());
        said_anything |= !stream.content.trim().is_empty();

        // Before the branch below, which would otherwise read a turn stopped
        // mid-sentence as a model that finished without saying anything.
        if stop.load(Ordering::Relaxed) {
            return Ok(sources);
        }

        if stream.tool_calls.is_empty() {
            // Nothing asked for and nothing said. The turn is over either way,
            // but a blank reply with no explanation is the worst version of it
            // — the athlete is left unable to tell a broken app from a model
            // with no opinion, and the honest answer is that it ran out of
            // room. Asking again is a real fix, so it is worth saying so.
            if !said_anything {
                return Err(anyhow!(
                    "The model finished without writing an answer — it can \
                     spend its whole budget thinking before it starts. Ask \
                     again, or try a different model in Settings."
                ));
            }
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
                Event::Tool {
                    call_id: &call.id,
                    label: &label,
                    running: true,
                    ok: true,
                },
            );
            // `sources` is shown under the answer as what was read, so only the
            // tools that read belong in it. Drafting a workout appears as its
            // own card, and a question appears as its own card, and neither is
            // a source the answer came from.
            if !matches!(call.name.as_str(), "draft_workout" | "ask_athlete") {
                sources.push(label.clone());
            }

            // The one tool that waits on a person, and the one that therefore
            // can't go through `run_tool` — it needs the channel to ask on and
            // the turn id to be answered against.
            let out = if call.name == "ask_athlete" {
                ask_athlete(app, id, channel, &call.id, &args, &mut asks_used)
                    .await
                    .into()
            } else {
                run_tool(&call.name, &args)
            };

            emit(
                app,
                channel,
                Event::Tool {
                    call_id: &call.id,
                    label: &label,
                    running: false,
                    ok: out.result.get("error").is_none(),
                },
            );

            if let Some(draft) = out.draft {
                emit(app, channel, Event::Draft { draft });
            }
            // Not on the chat channel: the theme list is on screen in Settings
            // and the palette's name is in the sidebar, and neither of those is
            // listening to this turn. A global event reaches whatever is open.
            if let Some(change) = out.themes {
                let _ = app.emit("themes:changed", change);
            }
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call.id,
                "name": call.name,
                "content": out.result.to_string(),
            }));

            // Stop pressed while a tool was running — including while a question
            // sat unanswered, which is the long one. The remaining calls in this
            // round are skipped and the turn ends at the top of the next.
            if stop.load(Ordering::Relaxed) {
                return Ok(sources);
            }
        }
    }

    Ok(sources)
}

/* ------------------------------------------------------------- follow-ups --- */

/// What to ask next, offered after an answer.
///
/// A separate, tiny, tool-free call rather than something squeezed out of the
/// answering turn: asking the model to end every answer with suggestions would
/// put them in the transcript, where they'd be saved, re-read on the next turn,
/// and eventually answered as if you'd asked them.
const FOLLOWUP_PROMPT: &str = "\
You suggest what someone might ask next about their own running and recovery \
data. Given the conversation so far, return exactly three short follow-up \
questions they could ask, each answerable from Garmin activity, heart-rate \
zone, cadence, sleep or recovery data.

Rules:
- Return a JSON array of three strings and nothing else.
- Each under 60 characters, phrased as the user would type it.
- Ask about something the conversation has not already answered.
- No preamble, no numbering, no markdown.";

/// One non-streaming completion, for the side calls that aren't a conversation.
///
/// `schema` is offered only when the configured model was recorded as taking
/// one. That capability is noted when the model is picked, so it can be stale —
/// a provider that has since dropped schema support costs one retry rather than
/// the whole answer.
/// Everything a one-shot call needs, resolved from the cache up front.
///
/// Taken by value rather than as a `&Db`: the connection is not `Sync`, so a
/// borrow held across the await would make the whole future non-`Send` and
/// Tauri could not spawn the command.
/// The token is not in here. Resolving it can mean a request of its own — a
/// hosted install with no id yet has to ask for one — so it is settled in
/// `one_shot`, where there is a client to ask with.
struct Creds {
    provider: Provider,
    model: String,
    structured: bool,
}

fn creds(db: &Db) -> Result<Creds> {
    let (provider, model) = load_config(db)?;
    let provider = provider.context("No model provider chosen yet.")?;
    let model = model.context("No model chosen yet.")?;
    Ok(Creds {
        provider,
        model,
        // The proxy serves one model and that model takes no schema, so asking
        // it for one would cost a rejected request and a retry on every
        // follow-up. `load_structured` reads whatever the last picker wrote,
        // which for a cloud install is not about this model at all.
        structured: provider != Provider::Cloud && load_structured(db)?,
    })
}

/// `max_tokens` is a required argument rather than an option, because both of
/// these calls have a known shape and neither has any business running long: a
/// reasoning model handed no ceiling will happily think its way through several
/// thousand tokens before writing three questions.
async fn one_shot(
    creds: Creds,
    messages: Vec<Value>,
    schema: Option<Value>,
    max_tokens: u32,
) -> Result<String> {
    let Creds {
        provider,
        model,
        structured,
    } = creds;

    let http = reqwest::Client::builder()
        // 20s was short enough that a reasoning model missed it every time, and
        // no answer at all is the failure people notice.
        .timeout(std::time::Duration::from_secs(45))
        .build()?;

    let key = auth_token(&http, provider).await?;

    let schema = schema.filter(|_| structured);

    let send = |with_schema: bool| {
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": false,
            "max_tokens": max_tokens,
        });
        if with_schema {
            if let Some(s) = &schema {
                body["response_format"] = s.clone();
            }
        }
        if provider.hosted() {
            body["usage"] = json!({ "include": true });
            // Neither of these calls is a thinking job — three questions about a
            // conversation that already happened, and three sentences about a
            // report that has already been computed. Left on, the reasoning is
            // billed, waited for, and taken out of `max_tokens`, which is how a
            // 200-token ceiling produces `content: null` and an empty
            // suggestions row. Ignored by models that don't reason.
            body["reasoning"] = json!({ "enabled": false });
        }
        authorize(
            http.post(format!("{}/chat/completions", provider.base()))
                .json(&body),
            provider,
            key.as_deref(),
        )
        .send()
    };

    // Reaching the provider and being served by it is what the banner reports,
    // so that verdict is settled here and the completion is judged afterwards.
    // The two are different questions: a provider that answers with an odd
    // empty completion is working, and telling the whole app it is down on the
    // strength of one strange reply would cry wolf on every screen.
    let reached = one_shot_reach(send, schema.is_some(), provider).await;
    note_call(
        provider,
        reached.as_ref().map(|_| ()).map_err(|e| {
            e.chain()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(": ")
        }),
    );

    let body = reached?;
    record_usage(provider, body.get("usage").filter(|u| !u.is_null()));

    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // An empty completion is a failure that looks like a success. It happens
    // when a model spends the whole `max_tokens` budget reasoning, and the
    // caller's "the model returned nothing" gave no hint which of the two it
    // was — so say which, and say it here where the reason is still in scope.
    if content.trim().is_empty() {
        let reason = body["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("no reason given");
        if let Some(err) = body.get("error").and_then(|e| e["message"].as_str()) {
            return Err(anyhow!("{} refused the request: {err}", provider.as_str()));
        }
        return Err(anyhow!(
            "{} returned an empty answer (finish_reason: {reason})",
            provider.as_str()
        ));
    }

    Ok(content)
}

/// Send, retry once without the schema if that was refused, and return the
/// decoded body. Everything that can go wrong in here is the provider being
/// unreachable, unauthorised, out of credit or unhappy with the request — which
/// is exactly the set of failures worth putting a banner up for.
async fn one_shot_reach<F, Fut>(send: F, wanted: bool, provider: Provider) -> Result<Value>
where
    F: Fn(bool) -> Fut,
    Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
{
    let mut resp = send(wanted).await.context("could not reach the model")?;
    if wanted && resp.status().is_client_error() {
        resp = send(false).await.context("could not reach the model")?;
    }
    if !resp.status().is_success() {
        let status = resp.status();
        // The body, not just the code. "openrouter returned 402" is a puzzle;
        // the sentence underneath it usually names the actual problem.
        let detail = resp.text().await.unwrap_or_default();

        // These calls don't retry the way a turn does — a follow-up row that
        // fails is a row that isn't there, which is survivable, and a second
        // request to rescue it is not obviously worth the money. But an id the
        // proxy has forgotten would refuse every future one too, so the id goes
        // and the next call enrols on its way past.
        if provider == Provider::Cloud
            && status == reqwest::StatusCode::UNAUTHORIZED
            && detail.contains(UNKNOWN_INSTALL)
        {
            let _ = store::forget_install_id();
        }

        return Err(anyhow!("{}", refusal(provider, status, &detail)));
    }

    Ok(resp.json().await?)
}

/// Below this, an answer is an error, a refusal, or a single sentence, and
/// there is nothing in it to ask three further questions about. Suggestions
/// cost a whole request, so the cheap check happens before the call.
const FOLLOWUP_MIN_ANSWER: usize = 160;

/// A ceiling for three questions of under sixty characters each, with room for
/// the JSON around them and for a model that thinks before it answers.
///
/// The headroom is the point. `one_shot` asks for reasoning to be switched off,
/// but that is a request a model is free to ignore, and reasoning is drawn from
/// this same budget — `ling-3.0-flash` was measured spending 129 tokens
/// thinking about a four-word prompt. At 200 the thinking finished the budget
/// and the answer came back null, which reaches the screen as a silently empty
/// suggestions row. Three short questions cost a fraction of this; the rest is
/// there so the failure needs two things to go wrong rather than one.
const FOLLOWUP_MAX_TOKENS: u32 = 500;

pub async fn followups(history: Vec<HistoryMessage>) -> Result<Vec<String>> {
    let substantial = history
        .last()
        .is_some_and(|m| m.role == "assistant" && m.content.trim().len() >= FOLLOWUP_MIN_ANSWER);
    if !substantial {
        return Ok(vec![]);
    }

    // Scoped so the connection is closed before the await below.
    let creds = creds(&Db::open_default()?)?;

    let mut messages = vec![json!({ "role": "system", "content": FOLLOWUP_PROMPT })];
    // Only the tail matters, and suggestions aren't worth re-sending a long
    // conversation for.
    for m in history.iter().rev().take(4).rev() {
        messages.push(json!({ "role": m.role, "content": m.content }));
    }

    // A schema where the model takes one: three strings, no prose, no fences,
    // nothing to dig out afterwards. Without it, `parse_followups` earns its keep.
    let text = one_shot(
        creds,
        messages,
        Some(followup_schema()),
        FOLLOWUP_MAX_TOKENS,
    )
    .await?;
    Ok(parse_followups(&text))
}

/* --------------------------------------------------------- weight summary --- */

const WEIGHT_PROMPT: &str = "\
You are this athlete's coach, writing the short opening paragraph of their own \
weight page. They are reading it. You will be given the computed report as JSON.

Address them as \"you\", never as \"the athlete\" and never in the third \
person. The page belongs to them: \"you're trending at 86.1 kg\", not \"the \
athlete's trend weight is 86.1\".

Write two or three sentences of plain prose. No headings, no bullets, no \
markdown, no preamble — the text is dropped straight onto the page.

Write about the recent past first. `windows` holds the last 7 days and the \
last 30, and one of those is what someone opening this page wants to know. The \
report's own `trendKg`, `changeKg` and `rateKgPerWeek` describe the whole \
window — half a year — which answers a different and slower question: it is \
context for the recent figures, worth a clause near the end, and it is not the \
thing to open with. \"Down 2 kg since February\" is a sentence that stays true \
on the day someone gains a kilo.

So: lead with the week if it has anything in it, otherwise the month, then set \
that against the longer trend. Each window carries its own `count`, and that \
count is the whole story about how much it can bear:
- `count` of 0 — the window is empty. Say nothing happened in it; do not reach \
  back and describe an older reading as though it were this week's.
- `count` of 1 — one reading, no direction. `changeKg` is null and there is no \
  trend to report. Quote the reading if it is useful, and say plainly that one \
  weigh-in is not a direction.
- `count` of 2 or more — `changeKg` is the move across the window, and \
  `lowKg`/`highKg` are its range. With only two or three readings, say the \
  number and say it rests on two or three readings; do not dress it as a rate.

When the two windows carry the same figures, they are the same readings — \
every weigh-in in the month happens to fall inside the week. Say it once, as \
the month, and note that nothing was recorded in the weeks before it. Reporting \
the same number twice under two headings reads as two pieces of evidence when \
there is one.

Read the rest of the fields carefully before you write:
- `trendKg` is the smoothed trend and is the figure to treat as the current \
  weight. `latestKg` is a single reading and is noisier; don't quote it as if \
  it were the truth.
- A point with `outlier: true` is a reading that disagrees with both its \
  neighbours by more than a body can move — a mis-entry. Mention it only if \
  there is one, and say it looks like a typo worth fixing in Garmin.
- `rateKgPerWeek` is null when there is not enough history to support a \
  direction. When it is null, say the history is too thin rather than \
  describing a trend.
- In `energy`, `predictedChangeKg` is computed from the `loggedDays` days that \
  have a food log — NOT from the whole span. If `coveragePct` is low, the gap \
  between it and `actualChangeKg` is mostly missing log, not a broken \
  metabolism, and you must say so.
- `daysSinceLatest` above about 21 means the data is stale; lead with that.

Never invent a number that is not in the JSON. Be direct and specific, the way \
a coach who has actually looked at the numbers would be. Do not moralise about \
their weight, do not give diet advice, and do not congratulate or console — \
describe what the data shows and what would make it more trustworthy.

Written to them, not about them.";

/// Three sentences of prose, with headroom for a model that reasons first.
///
/// Same arithmetic as [`FOLLOWUP_MAX_TOKENS`], and it fails louder here: an
/// empty return is an error on the Weight screen rather than a quiet gap, so a
/// ceiling the thinking can exhaust shows up as "the model returned nothing"
/// under someone's weight chart.
const WEIGHT_MAX_TOKENS: u32 = 800;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightSummary {
    pub text: String,
    /// When it was written. The summary is kept until the data behind it moves,
    /// so this can be days old and the screen says as much.
    pub generated_at: String,
    /// True when this came from the cache rather than the model just now.
    pub cached: bool,
}

/// What the summary was written about.
///
/// A weigh-in is very nearly the only thing that can change what this paragraph
/// should say, so it is very nearly the only thing in here. `latestDate` and
/// `count` together catch every case: a new entry moves one or both, and a
/// correction to an existing day moves the weight under a date already counted.
///
/// It used to carry `trendKg`, `rateKgPerWeek` and `energy.loggedDays` too,
/// which is why it rewrote so often. All three are derived: the first two shift
/// a digit whenever anything upstream does, and `loggedDays` moves every time a
/// meal is logged — most days on this account, and nothing to do with the
/// scale. The paragraph was being rebilled for a sandwich.
///
/// `goal.targetKg` stays. It isn't a weigh-in, but it is a deliberate and rare
/// edit that makes the standing prose wrong rather than merely older, and a
/// paragraph describing the old target under the new one is the kind of error
/// nobody thinks to check for.
///
/// The leading version is part of that set. The prose depends on the prompt as
/// much as on the figures, and without it a fix to the prompt reaches nobody
/// whose weight happens to be steady — they keep being served the sentence the
/// old prompt wrote until they weigh in again. Bump it when `WEIGHT_PROMPT`
/// changes in a way that should show.
fn weight_fingerprint(report: &Value) -> String {
    format!(
        "v3|{}|{}|{}",
        report["latestDate"], report["count"], report["goal"]["targetKg"],
    )
}

/// The prose at the top of the Weight screen.
///
/// `force` regenerates even when the cached summary still matches the data,
/// which is what the screen's regenerate control does.
pub async fn weight_summary(days: u32, force: bool) -> Result<WeightSummary> {
    // The connection is opened, used and dropped inside this block: rusqlite's
    // is not `Sync`, so holding one across the await below would make this
    // future non-`Send` and Tauri could not spawn the command.
    let (report, fingerprint, cached, creds) = {
        let db = Db::open_default()?;
        let report = serde_json::to_value(query::weight(&db, days)?)?;
        let fingerprint = weight_fingerprint(&report);

        let cached = match (
            db.sync_state("weight_summary_text")?,
            db.sync_state("weight_summary_at")?,
            db.sync_state("weight_summary_key")?,
        ) {
            (Some(text), Some(at), Some(seen))
                if !force && seen == fingerprint && !text.is_empty() =>
            {
                Some(WeightSummary {
                    text,
                    generated_at: at,
                    cached: true,
                })
            }
            _ => None,
        };

        // Resolved even when the cache hits, because it's cheap and the borrow
        // has to end here either way.
        (report, fingerprint, cached, creds(&db))
    };

    if let Some(hit) = cached {
        return Ok(hit);
    }

    let messages = vec![
        json!({ "role": "system", "content": now_line() + WEIGHT_PROMPT }),
        json!({ "role": "user", "content": report.to_string() }),
    ];
    // Two or three sentences, which is what the prompt asks for and what the
    // page has room for.
    let text = one_shot(creds?, messages, None, WEIGHT_MAX_TOKENS)
        .await?
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(anyhow!("the model returned nothing"));
    }

    let generated_at = chrono::Utc::now().to_rfc3339();
    let db = Db::open_default()?;
    db.set_sync_state("weight_summary_text", &text)?;
    db.set_sync_state("weight_summary_at", &generated_at)?;
    db.set_sync_state("weight_summary_key", &fingerprint)?;

    Ok(WeightSummary {
        text,
        generated_at,
        cached: false,
    })
}

/* ----------------------------------------------------------- today summary --- */

const TODAY_PROMPT: &str = "\
You are this athlete's coach, writing the two or three sentences that open \
their home screen. They are reading it, first thing, probably on a phone. You \
will be given a JSON bundle of everything that screen knows.

Address them as \"you\". Write plain prose — no headings, no bullets, no \
markdown, no preamble, no greeting. The text is dropped straight under the \
greeting the app has already written.

This is the hardest part: **the screen below you already shows the numbers.** \
Sleep, body battery, readiness, HRV, resting heart rate and the week's chart \
are all rendered as tiles inches beneath your text. Reciting them is the one \
way to waste this paragraph. Say the thing the tiles cannot: what these numbers \
mean *together*, what changed, or what today should be. One number, quoted \
because your point needs it, is right; four in a row is a worse version of the \
tiles.

The coach panel directly below you carries today's nudges, each with its \
evidence. Do not repeat one. If `coach.nudges` is non-empty, the athlete is \
about to read it — you may lead into it, never restate it.

Read `cacheStatus` first, and read it correctly. `daysSinceActivity` and \
`daysSinceDaily` are the **age of the newest record** — how long ago the last \
one was, not how much history exists. The cache holds years. \
`daysSinceActivity: 2` means the most recent session was two days ago, which is \
an ordinary gap between sessions and not a fact about the data at all.

So: when both are three or under, say nothing whatsoever about data, coverage \
or how much you have to work with — just write about the training. Only when \
one is well past three does the gap become the story, and then it is your \
opening sentence: describe the period you actually have rather than \"this \
week\". A confident paragraph about today built on a reading from two months \
ago is the worst thing you can produce here.

The caveats that apply everywhere in this app apply here:
- `has_hr_data: false` means the session recorded no heart rate. That is not an \
  easy session — leave it out of any easy/hard reading rather than counting it \
  as zero.
- `hr_confidence.level` of `caution` or `poor`, and \
  `hr_confidence.cadenceLock` of `likely`, mean the zone split may be the wrist \
  sensor reading arm swing as pulse. `hr_confidence.notes` gives the reason in \
  full. Do not build a point about hard-effort drift on one of those without \
  saying so.
- `resting_hr_source` is only a real resting heart rate when it reads \
  `overnight`. Never read a jump between two different sources as fitness.
- Strength, jump rope and circuits are not continuous aerobic work. Their zone \
  split describes work-to-rest, not a target hit or missed — keep them out of \
  any easy/hard split, and never prescribe a heart-rate ceiling for one.

Never invent a number that is not in the JSON, and never estimate one to fill \
a gap. Be direct, specific and calm. Flag overreaching when the data shows it \
without catastrophising, and say plainly when something is going well — the \
recovery signal on this account usually is.";

/// Two or three sentences, with headroom for a model that reasons first.
///
/// Same arithmetic as [`WEIGHT_MAX_TOKENS`]. This one is the first text on the
/// first screen, so an empty return is conspicuous.
const TODAY_MAX_TOKENS: u32 = 800;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodaySummary {
    pub text: String,
    /// When it was written. Regenerated at most once a day, so on a screen
    /// opened at breakfast and again at six this can be hours old.
    pub generated_at: String,
    /// True when this came from the cache rather than the model just now.
    pub cached: bool,
}

/// How many days of each series the opening paragraph is written from.
///
/// Wider than the paragraph can use, deliberately: "your resting heart rate is
/// up" is a claim about a baseline, and a model handed three days has no
/// baseline to make it against.
const TODAY_RECOVERY_DAYS: u32 = 14;
const TODAY_ACTIVITIES: u32 = 10;
const TODAY_DRIFT_RUNS: u32 = 8;

/// What the paragraph was written about.
///
/// The date is in here, so the summary rewrites once a day even when nothing
/// synced — "you slept well" stops being true at midnight, and staleness grows
/// on its own while the data sits still. Everything else is the figures the
/// prose actually leans on, so a fresh sync during the day rewrites it too and
/// merely reopening the screen does not.
/// The recovery rows come back newest first, so `[0]` is this morning.
///
/// These keys are `snake_case` because `query::RecoveryDay` carries no
/// `rename_all` — unlike `CacheStatus` and `CoachReport` beside it, which do.
/// A key that doesn't exist reads as `null` here rather than failing, so the
/// mixed casing has to be checked against the wire format rather than assumed:
/// spelt wrong, every field is null, the fingerprint still looks plausible, and
/// the paragraph silently stops rewriting when the numbers move.
/// The leading version works as it does in [`weight_fingerprint`]: the prose
/// depends on the prompt as much as on the figures. This one self-heals at
/// midnight because the date is in the key, but a prompt fix shipped in the
/// morning should not wait until then to show.
fn today_fingerprint(bundle: &Value, today: &str) -> String {
    let day = &bundle["recovery"][0];
    format!(
        "v2|{today}|{}|{}|{}|{}|{}|{}|{}",
        bundle["cacheStatus"]["newestDailyDate"],
        bundle["cacheStatus"]["newestActivityDate"],
        bundle["cacheStatus"]["activitiesCached"],
        day["sleep_hours"],
        day["hrv_last_night"],
        day["training_readiness"],
        bundle["coach"]["nudges"].as_array().map_or(0, Vec::len),
    )
}

/// The prose at the top of the Today screen.
///
/// `force` regenerates even when the cached paragraph still matches, which is
/// what the screen's rewrite control does.
pub async fn today_summary(force: bool) -> Result<TodaySummary> {
    // The connection is opened, used and dropped inside this block: rusqlite's
    // is not `Sync`, so holding one across the await below would make this
    // future non-`Send` and Tauri could not spawn the command.
    let (bundle, fingerprint, cached, creds) = {
        let db = Db::open_default()?;
        let today = chrono::Local::now().date_naive();

        // Every one of these is the same function the screens and the chat
        // tools call. The paragraph is written from the app's own numbers, so
        // it cannot disagree with the tiles underneath it.
        let bundle = json!({
            "cacheStatus": query::cache_status(&db)?,
            "recovery": query::recovery(&db, TODAY_RECOVERY_DAYS)?,
            "recentActivities": query::recent_activities(&db, TODAY_ACTIVITIES, None)?,
            "zoneDrift": query::zone_drift(&db, TODAY_DRIFT_RUNS)?,
            "coach": garmin_core::coach::for_today(&db, today)?,
        });
        let fingerprint = today_fingerprint(&bundle, &today.to_string());

        let cached = match (
            db.sync_state("today_summary_text")?,
            db.sync_state("today_summary_at")?,
            db.sync_state("today_summary_key")?,
        ) {
            (Some(text), Some(at), Some(seen))
                if !force && seen == fingerprint && !text.is_empty() =>
            {
                Some(TodaySummary {
                    text,
                    generated_at: at,
                    cached: true,
                })
            }
            _ => None,
        };

        (bundle, fingerprint, cached, creds(&db))
    };

    if let Some(hit) = cached {
        return Ok(hit);
    }

    let messages = vec![
        json!({ "role": "system", "content": now_line() + TODAY_PROMPT }),
        json!({ "role": "user", "content": bundle.to_string() }),
    ];
    let text = one_shot(creds?, messages, None, TODAY_MAX_TOKENS)
        .await?
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(anyhow!("the model returned nothing"));
    }

    let generated_at = chrono::Utc::now().to_rfc3339();
    let db = Db::open_default()?;
    db.set_sync_state("today_summary_text", &text)?;
    db.set_sync_state("today_summary_at", &generated_at)?;
    db.set_sync_state("today_summary_key", &fingerprint)?;

    Ok(TodaySummary {
        text,
        generated_at,
        cached: false,
    })
}

/* ------------------------------------------------------ activity critique --- */

const CRITIQUE_PROMPT: &str = "\
You are this athlete's coach. They are looking at one session and have pressed \
a button asking you what they got wrong. You will be given the computed \
analysis of that session as JSON.

They already know what they did. The distance, the pace, the heart rate and the \
zone breakdown are on the screen beside your text, and narrating them back \
spends the only paragraph you get. Write the part they cannot read off the \
page: what went wrong, what to have done instead, and what to carry into the \
next one.

Write three or four sentences of plain prose. No headings, no bullets, no \
markdown, no preamble — the text is dropped straight onto the page above the \
charts. Address them as \"you\".

First, read `discipline`. It governs everything else, and getting it wrong is \
the one mistake that makes the whole paragraph worthless:
- `paced` — running, walking, hiking.
- `endurance` — cycling, swimming, rowing, the cardio machines.
- `interval` — strength work, jump rope, circuits, climbing. Sets and rests.
- `other` — anything Garmin wouldn't classify: a tactical session, a sport with \
  no template. Treat it like `interval` unless the numbers say otherwise.

On `paced` and `endurance`, `zones.percent` — the share of tracked heart-rate \
time in Z1..Z5, where Z3 and up is hard effort — is the number that matters, \
and on an easy session that was meant to be easy it is most of the verdict.

Before you build the verdict on it, check `hrConfidence.level`. On `poor` — and \
above all when `hrConfidence.cadenceLock` is `likely`, meaning heart rate \
tracked step rate and the sensor was probably reading arm swing rather than \
pulse — the zone split cannot carry the paragraph. Say that plainly, in the \
athlete's terms, and criticise what is left: duration, cadence, how the effort \
was structured. Criticising someone for a Z5 that a sensor artefact invented is \
the worst thing this button can do. On `caution`, use the split but name the \
reason once.

If `zones.maxDisagreementPct` is above about 10, this app's own reading of the \
heart-rate trace and Garmin's totals disagree by more than rounding, and no \
number from either belongs in a confident sentence.

Where `paceEstimated` is true, pace and distance were estimated from arm \
movement, so do not criticise a pace to the second. Prefer \
`movingPaceMinKm` when the session had walk breaks in it.

On `interval` and `other` it is not, and this is not a nuance you may skip. The \
heart rate in a session of sets and rounds climbs and falls because the work \
does; the zone split describes the ratio of work to rest and nothing else. Time \
at Z2 there is the rest between sets, not a well-judged easy effort. Time above \
Z2 there is the work, not drift. So on those sessions: never tell them to hold \
a heart rate, never set a bpm ceiling, never praise or fault the zone \
distribution, and never use the words easy run, pace, or drift. Judge them \
instead on what is actually in the JSON — how long the session ran against \
their recent ones of the same sport, how the rounds in `laps` held up across \
it, what the effort peaked at, and what `aerobicTe` and `anaerobicTe` say was \
trained.

The rest of the JSON:
- `highlights` are already computed and already true. Build the paragraph out \
  of the two or three that matter most; do not list them all, and do not \
  contradict one. `tone` says whether a highlight is good, neutral, or worth \
  watching.
- `comparison` is this session against their recent ones of the same sport. A \
  null field means there was nothing to compare against, which is not the same \
  as no change.
- `laps` are the splits — the rounds or the sets on an `interval` session. A \
  single lap means the watch was never lapped, not that the session had no \
  structure: you know how long it ran and what the heart rate did, and nothing \
  about the sets. Say less rather than inventing them.
- `indoor: true` means no GPS was recorded — a treadmill, or a gym floor — so \
  there is no route, and on a run it earns no VO2 max.
- A null is missing data, never a zero. No heart-rate data means no strap, not \
  a session spent in Z1.

What to say:
- Open with the verdict, not the recap: whether this was executed the way a \
  session of this kind should be.
- Name the one thing that mattered most, say what it cost, and say what to have \
  done instead as an instruction with a number in it. On a run that is \"hold \
  the first ten minutes under 136\"; on a strength or interval session it is \
  something like \"the last four rounds ran 15 bpm hotter than the first — put \
  the rest back to where it started\" or \"this ran twelve minutes shorter than \
  your usual\". Never \"go easier\". One correction they will act on beats three \
  they will skim past.
- Quote the figure you are reasoning from, so the correction can be argued with \
  rather than only believed.
- If the session was executed well, say so plainly, name the decision that made \
  it work, and tell them to keep doing exactly that. Never manufacture a fault \
  to have something to correct.
- Close on the next session of this kind: the one thing to change, or the one \
  thing to repeat.

Never invent a number that is not in the JSON. Do not moralise, do not \
catastrophise, and do not pad. A short paragraph that is entirely true beats a \
long one that hedges.";

/// Four sentences of prose, with headroom for a model that reasons first.
const CRITIQUE_MAX_TOKENS: u32 = 400;

/// Laps sent to the model. A session with two hundred of them is a set of
/// strength intervals, and the first thirty say everything the next hundred and
/// seventy would.
const MAX_LAPS_FOR_MODEL: usize = 30;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCritique {
    pub text: String,
    pub generated_at: String,
    /// True when this came from the cache rather than the model just now.
    pub cached: bool,
}

/// The analysis as the model sees it.
///
/// Two things are removed. The sampled series, because five hundred rows of
/// eight columns is most of a context window and every conclusion worth drawing
/// from it is already in `highlights`. And the coordinates with it — this app
/// tells the athlete on the Ask screen that raw GPS never leaves the machine,
/// and a route is the one piece of their data that says where they live.
fn for_model(analysis: &garmin_core::ActivityAnalysis) -> Value {
    let mut v = serde_json::to_value(analysis).unwrap_or_else(|_| json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.remove("series");
        if let Some(laps) = obj.get_mut("laps").and_then(|l| l.as_array_mut()) {
            laps.truncate(MAX_LAPS_FOR_MODEL);
        }
    }
    v
}

/// What the critique was written about.
///
/// The analysis already carries a fingerprint of the activity it was built
/// from, so the prose is invalidated by exactly the things that would change
/// what there is to say — a re-sync correcting a duration, or a tag being
/// added — and not by opening the page again.
///
/// The leading `v4` is the prompt's version. Rows written by an earlier one
/// sit in the same table under the same numbers — the first described the
/// session rather than criticising it, the second read every sport as if it
/// were a run, the third took every zone split as measurement and so could
/// criticise an athlete for a Z5 that a sensor artefact invented — and without
/// this they would surface under a button that promises something they aren't.
fn activity_fingerprint(analysis: &garmin_core::ActivityAnalysis) -> String {
    format!(
        "v4|{}|{:?}|{:?}|{:?}|{}|{}",
        analysis.activity_id,
        analysis.duration_s,
        analysis.distance_m,
        analysis.zones.percent,
        analysis.highlights.len(),
        analysis.tags.join(","),
    )
}

/// The critique already written about this session, if there is one.
///
/// A database read and nothing else. Opening an activity must not reach a
/// model — the athlete asks for the criticism by pressing a button — but a
/// critique they have already paid for should still be on the page when they
/// come back to it, which is what this returns.
pub fn cached_activity_critique(
    analysis: &garmin_core::ActivityAnalysis,
) -> Result<Option<ActivityCritique>> {
    let db = Db::open_default()?;
    Ok(db
        .activity_critique(analysis.activity_id, &activity_fingerprint(analysis))?
        .map(|(text, generated_at)| ActivityCritique {
            text,
            generated_at,
            cached: true,
        }))
}

/// Write the critique of one session.
///
/// Always calls the model. The button that reaches this is only offered when
/// nothing is stored, and pressing it a second time means rewrite — so there is
/// no cache to consult here, only one to overwrite.
pub async fn activity_critique(
    analysis: &garmin_core::ActivityAnalysis,
) -> Result<ActivityCritique> {
    // Scoped so the connection is dropped before the await below — rusqlite's
    // is not `Sync`, and holding one across it makes the future non-`Send`.
    let creds = {
        let db = Db::open_default()?;
        creds(&db)
    };

    let messages = vec![
        json!({ "role": "system", "content": now_line() + CRITIQUE_PROMPT }),
        json!({ "role": "user", "content": for_model(analysis).to_string() }),
    ];
    let text = one_shot(creds?, messages, None, CRITIQUE_MAX_TOKENS)
        .await?
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(anyhow!("the model returned nothing"));
    }

    let generated_at = chrono::Utc::now().to_rfc3339();
    let db = Db::open_default()?;
    db.save_activity_critique(
        analysis.activity_id,
        &activity_fingerprint(analysis),
        &generated_at,
        &text,
    )?;

    Ok(ActivityCritique {
        text,
        generated_at,
        cached: false,
    })
}

/// The `response_format` for a follow-up call.
///
/// An object with one array rather than a bare array: OpenAI-style structured
/// output requires the root to be an object, and every provider follows that.
/// `parse_followups` reads the questions back out either way.
fn followup_schema() -> Value {
    json!({
        "type": "json_schema",
        "json_schema": {
            "name": "followups",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "questions": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["questions"],
                "additionalProperties": false
            }
        }
    })
}

/// Models wrap JSON in prose and fences no matter how firmly they're asked not
/// to, so this digs the array out rather than trusting the response shape.
///
/// And when there's no array at all — a numbered list, one per line, which is
/// what a smaller model tends to answer with — it falls back to reading the
/// lines. Every shape that carries three questions should produce three
/// questions; the alternative is an empty row and no explanation.
fn parse_followups(text: &str) -> Vec<String> {
    let from_json = match (text.find('['), text.rfind(']')) {
        (Some(a), Some(b)) if b > a => json_questions(&text[a..=b]),
        _ => vec![],
    };
    let found = if from_json.is_empty() {
        loose_questions(text)
    } else {
        from_json
    };

    found
        .into_iter()
        .map(|s| s.trim().trim_matches('"').trim().to_string())
        .filter(|s| !s.is_empty() && s.chars().count() <= 90)
        .take(3)
        .collect()
}

/// `["a","b"]`, and also `[{"question":"a"}]`, which models produce about as
/// often despite being asked for strings.
fn json_questions(slice: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(slice) else {
        return vec![];
    };
    let Some(items) = v.as_array() else {
        return vec![];
    };
    items
        .iter()
        .filter_map(|i| match i {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => o.values().find_map(|v| v.as_str()).map(str::to_owned),
            _ => None,
        })
        .collect()
}

/// One question per line, with whatever bullet or number was stuck on the front.
fn loose_questions(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            l.trim()
                .trim_start_matches(|c: char| {
                    c.is_ascii_digit() || matches!(c, '-' | '*' | '.' | ')' | '#' | ' ')
                })
                .trim()
        })
        // A question mark is the one reliable signal that a line is a suggestion
        // rather than the preamble the model put above them.
        .filter(|l| l.ends_with('?'))
        .map(str::to_owned)
        .collect()
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
    /// The usage block from the final chunk, present because the request asked
    /// for `stream_options.include_usage`. Absent from a provider that doesn't
    /// send one, which costs the totals a request rather than the turn an error.
    usage: Option<Value>,
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
    stop: &AtomicBool,
    resp: reqwest::Response,
) -> Result<StreamResult> {
    let mut out = StreamResult::default();
    let mut by_index: Vec<ToolCall> = Vec::new();
    let mut buf = String::new();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        // Stop, caught between chunks — which is as fine-grained as this gets
        // and fine enough: chunks arrive several times a second. Dropping the
        // stream here is what closes the connection, so a stopped turn stops
        // costing money rather than finishing quietly in the background.
        if stop.load(Ordering::Relaxed) {
            // Any tool calls half-assembled at this point are deliberately
            // discarded. The turn is over; running them would be doing work for
            // an answer nobody is waiting for.
            out.tool_calls.clear();
            return Ok(out);
        }

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

                // The usage block arrives in a chunk of its own at the end,
                // with an empty `choices`, so it is read before the delta is.
                if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
                    out.usage = Some(u.clone());
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

    /// Resolve a dotted path against a JSON value.
    fn at<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
        path.split('.').try_fold(v, |acc, k| acc.get(k))
    }

    /// The prompts name fields. Nothing made them true.
    ///
    /// A system prompt that says "read `paceEstimated`" is a claim about the
    /// wire format, and it is the only kind of claim in this file that fails
    /// silently: the model looks for a key that isn't there, finds nothing, and
    /// writes a confident paragraph with the caveat quietly missing. Every
    /// caveat this app's honesty rests on was named in the wrong case — the
    /// tool types are `snake_case` except `hr_confidence`'s own fields, which
    /// are camel, and `CachedActivity`, which is camel and is *not* what the
    /// tools return.
    ///
    /// So the paths are pinned against real serialized output. Rename a field
    /// and this fails here rather than in a paragraph nobody can check.
    #[test]
    fn every_field_the_prompts_name_exists_in_the_tool_output() {
        let activity: garmin_core::CachedActivity = serde_json::from_value(json!({
            "activityId": 1_i64,
            "name": "Treadmill Running",
            "typeKey": "treadmill_running",
            "startTimeLocal": "2026-08-07 17:07:39",
            "localDate": "2026-08-07",
            "distanceM": 1210.0,
            "durationS": 648.0,
            "movingDurationS": 600.0,
            "avgHr": 149.0,
            "maxHr": 174.0,
            "avgCadence": 131.4,
            "calories": 90.0,
            "elevationGain": null,
            "steps": 1400_i64,
            "aerobicTe": 2.2,
            "anaerobicTe": 0.8,
            "zoneSecs": [24.0, 168.0, 126.0, 330.0, 0.0],
        }))
        .expect("activity fixture");

        let view = serde_json::to_value(query::ActivityView::from(&activity)).unwrap();
        for path in [
            "has_hr_data",
            "hr_confidence.level",
            "hr_confidence.cadenceLock",
            "hr_confidence.notes",
            "pace_estimated",
            "pace_min_per_km",
            "moving_pace_min_per_km",
        ] {
            assert!(
                at(&view, path).is_some(),
                "SYSTEM_PROMPT and TODAY_PROMPT both tell the model to read \
                 `{path}`, and an activity does not have it"
            );
        }

        let day = serde_json::to_value(query::RecoveryDay::from(garmin_core::DailyMetrics {
            date: "2026-08-09".into(),
            ..Default::default()
        }))
        .unwrap();
        for path in [
            "resting_hr_source",
            "hrv_last_night",
            "training_readiness",
            "sleep_hours",
        ] {
            assert!(
                at(&day, path).is_some(),
                "the prompts and `today_fingerprint` read `{path}` off a recovery day"
            );
        }
    }

    /// The casing that caused it, stated as an assertion so the next person to
    /// add a prompt can see the trap rather than fall into it.
    #[test]
    fn the_tool_types_are_snake_case_even_though_the_cached_row_is_not() {
        let cached = serde_json::to_value(garmin_core::CachedActivity {
            activity_id: 1,
            name: None,
            type_key: Some("running".into()),
            start_time_local: None,
            local_date: Some("2026-08-07".into()),
            distance_m: None,
            duration_s: None,
            moving_duration_s: None,
            avg_hr: None,
            max_hr: None,
            avg_cadence: None,
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: [0.0; 5],
        })
        .unwrap();

        assert!(
            cached.get("typeKey").is_some(),
            "the cached row is camelCase"
        );
        assert!(
            cached.get("type_key").is_none(),
            "and only camelCase — which is why a prompt written against it is wrong"
        );
    }

    /// Every tool the model is offered must actually dispatch. A schema that
    /// names a tool `run_tool` doesn't handle would only surface as the model
    /// being told "unknown tool" mid-conversation, which is invisible here
    /// unless something checks the two lists against each other.
    #[test]
    fn followups_survive_the_wrappers_models_add() {
        // Bare array.
        assert_eq!(parse_followups(r#"["a","b","c"]"#), vec!["a", "b", "c"]);
        // Fenced, which is the common case.
        assert_eq!(
            parse_followups("```json\n[\"one\", \"two\"]\n```"),
            vec!["one", "two"]
        );
        // Prose either side.
        assert_eq!(
            parse_followups("Sure! Here you go:\n[\"x\"]\nHope that helps."),
            vec!["x"]
        );
        // More than three gets trimmed to three.
        assert_eq!(parse_followups(r#"["1","2","3","4"]"#).len(), 3);
        // Nothing parseable is an empty list, not an error — the screen just
        // shows no suggestions.
        assert!(parse_followups("I could not think of any.").is_empty());
        assert!(parse_followups("").is_empty());
        // Blank and overlong entries are dropped.
        let long = "x".repeat(200);
        assert!(parse_followups(&format!(r#"["  ", "{long}"]"#)).is_empty());
    }

    /// The suggestion row was empty far more often than the model was actually
    /// failing — it just answered in a shape the strict array parse threw away.
    #[test]
    fn followups_survive_not_being_an_array_of_strings() {
        // Objects instead of strings.
        assert_eq!(
            parse_followups(r#"[{"question": "How was my week?"}, {"q": "And my sleep?"}]"#),
            vec!["How was my week?", "And my sleep?"]
        );
        // The object a strict JSON schema produces.
        assert_eq!(
            parse_followups(r#"{"questions": ["How was my week?", "And my sleep?"]}"#),
            vec!["How was my week?", "And my sleep?"]
        );
        // A numbered list, no JSON anywhere.
        assert_eq!(
            parse_followups("1. How was my week?\n2. Is my cadence improving?"),
            vec!["How was my week?", "Is my cadence improving?"]
        );
        // Bullets, with a preamble line that isn't a question.
        assert_eq!(
            parse_followups("You could ask:\n- Am I recovered?\n* What was my Z5 time?"),
            vec!["Am I recovered?", "What was my Z5 time?"]
        );
    }

    /// Every tool the model is actually offered, which is the only list worth
    /// asserting against — a tool dropped from `tool_schemas` should drop out
    /// of these tests with it.
    fn tool_names() -> Vec<String> {
        tool_schemas()
            .as_array()
            .expect("tool schemas are an array")
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn every_offered_tool_dispatches() {
        let names = tool_names();

        assert!(names.contains(&"nutrition".to_string()));
        assert!(names.contains(&"workouts".to_string()));
        assert!(names.contains(&"routes".to_string()));
        assert!(names.contains(&"draft_workout".to_string()));
        assert!(names.contains(&"ask_athlete".to_string()));

        for name in &names {
            // Every tool needs a human-readable label for the UI, this one
            // included — it is the row the timeline shows while the turn waits.
            assert!(
                !describe(name, &json!({})).starts_with("Running "),
                "{name} has no description in `describe`"
            );

            // `ask_athlete` is the exception, and deliberately so: it waits on a
            // person, so it is dispatched by the turn loop where the channel and
            // the turn id are, and `run_tool` has never heard of it. Routing it
            // here would be routing it nowhere.
            if name == "ask_athlete" {
                continue;
            }

            let out = run_tool(name, &json!({})).result;
            let err = out.get("error").and_then(|e| e.as_str()).unwrap_or("");
            assert!(
                !err.starts_with("unknown tool"),
                "{name} is offered to the model but has no dispatch arm"
            );
        }
    }

    /// The shape the schema documents, and the three sloppier ones the small
    /// models this app is pointed at actually send.
    ///
    /// A question that fails to parse doesn't fail loudly — the model is handed
    /// an error, and the usual recovery is to give up on asking and guess, which
    /// looks from the outside like the feature not existing.
    #[test]
    fn a_question_parses_from_every_shape_a_model_sends() {
        let documented = parse_ask(&json!({
            "question": "How long have you got today?",
            "header": "Time today",
            "options": [
                { "label": "About 20 minutes", "description": "Enough for a short easy one." },
                { "label": "About 45 minutes" }
            ],
        }))
        .expect("the documented shape should parse");
        assert_eq!(documented.header.as_deref(), Some("Time today"));
        assert_eq!(documented.options.len(), 2);
        assert_eq!(documented.options[1].description, None);
        assert!(!documented.multi);

        // Options as bare strings, which is what a model sends when it reads
        // "two to four answers" and stops there.
        let bare = parse_ask(&json!({
            "question": "Easy or hard?",
            "options": ["Easy", "Hard"],
        }))
        .expect("bare string options should parse");
        assert_eq!(bare.options[0].label, "Easy");

        // The whole array, JSON-encoded into a string. Same failure `repair`
        // exists for on `draft_workout`.
        let encoded = parse_ask(&json!({
            "question": "Easy or hard?",
            "options": "[{\"label\":\"Easy\"},{\"label\":\"Hard\"}]",
        }))
        .expect("an encoded array should parse");
        assert_eq!(encoded.options.len(), 2);

        // A header long enough to break the chip it is drawn in gets cut, not
        // rejected — the question is still worth asking.
        let long = parse_ask(&json!({
            "question": "Which?",
            "header": "How much time do you have available today",
            "options": ["A", "B"],
        }))
        .expect("a long header should parse");
        assert!(long.header.unwrap().chars().count() <= 16);
    }

    /// The two ways a question is not a question. Both come back as an error the
    /// model can act on rather than a card the athlete can't answer.
    #[test]
    fn a_question_with_nothing_to_choose_is_refused() {
        assert!(parse_ask(&json!({ "options": ["A", "B"] })).is_err());
        assert!(parse_ask(&json!({ "question": "Shall I?", "options": ["Yes"] })).is_err());
        assert!(parse_ask(&json!({ "question": "Shall I?" })).is_err());
    }

    /// Answering a question nobody asked, or one that has already been answered.
    ///
    /// Both are races the frontend can lose by a click — a Stop pressed as the
    /// button goes down, a turn that timed out — and neither may panic or block.
    #[test]
    fn answering_a_question_that_is_no_longer_open_is_harmless() {
        assert!(!answer_ask(
            "no-such-turn",
            "no-such-call",
            vec!["Easy".into()]
        ));

        with_live(|live| live.insert("turn-1".into(), TurnHandle::default()));
        assert!(
            !answer_ask("turn-1", "call-1", vec!["Easy".into()]),
            "a live turn with no open question still has nothing to answer"
        );

        // A sender whose receiver has gone — the turn moved on without it.
        let (tx, rx) = oneshot::channel();
        with_live(|live| {
            live.get_mut("turn-1")
                .unwrap()
                .asks
                .insert("call-1".into(), tx)
        });
        drop(rx);
        assert!(!answer_ask("turn-1", "call-1", vec!["Easy".into()]));

        // Cancelling drops whatever is still open, and says nothing about it.
        cancel("turn-1");
        cancel("no-such-turn");
        with_live(|live| live.remove("turn-1"));
    }

    /// The exact shape the tool schema tells the model to send. If this stops
    /// parsing, every workout the model drafts fails and the only symptom is it
    /// apologising in prose.
    #[test]
    fn the_documented_draft_shape_produces_a_draft() {
        let out = run_tool(
            "draft_workout",
            &json!({
                "name": "4 x 3min Z4",
                "sport": "running",
                "description": "Two easy bookends around the reps.",
                "steps": [
                    {
                        "type": "exec",
                        "kind": "warmup",
                        "end": { "type": "time", "seconds": 600 },
                        "target": { "type": "hr_zone", "zone": 2 },
                        "note": "conversational"
                    },
                    {
                        "type": "repeat",
                        "times": 4,
                        "steps": [
                            { "kind": "interval", "end": { "type": "time", "seconds": 180 },
                              "target": { "type": "hr_zone", "zone": 4 } },
                            { "kind": "recovery", "end": { "type": "time", "seconds": 120 },
                              "target": { "type": "hr_zone", "zone": 1 } }
                        ]
                    },
                    { "type": "exec", "kind": "cooldown", "end": { "type": "time", "seconds": 300 } }
                ]
            }),
        );

        let draft = out.draft.expect("the documented shape should draft");
        assert_eq!(draft.name, "4 x 3min Z4");
        assert_eq!(draft.steps.len(), 3, "the repeat stays one step");
        assert_eq!(out.result["ok"], true);
        assert_eq!(out.result["steps"], 10);
        assert_eq!(out.result["estimatedMinutes"], 35.0);
    }

    /// The arguments `ling-3.0-flash` actually sent on the first real attempt,
    /// verbatim. Two things are wrong with it and both are unambiguous: `steps`
    /// is a string of JSON rather than an array, and no `end` carries its
    /// `type` tag. Before `repair` this failed to deserialize, the model was
    /// told so, and it degraded into emitting tool calls as plain text.
    #[test]
    fn the_shape_a_small_model_actually_sends_still_drafts() {
        let out = run_tool(
            "draft_workout",
            &json!({
                "name": "4 x 3min Z4 Hard",
                "sport": "running",
                "description": "Short, sharp VO2-style session.",
                "steps": "[{\"type\": \"exec\", \"kind\": \"warmup\", \"end\": {\"seconds\": 300}, \
                            \"note\": \"easy jog, Z1-Z2\"}, \
                           {\"type\": \"repeat\", \"kind\": \"interval\", \"times\": 4, \"steps\": [\
                             {\"type\": \"exec\", \"kind\": \"interval\", \"end\": {\"seconds\": 180}, \
                              \"target\": {\"type\": \"hr_zone\", \"zone\": 4}, \"note\": \"Z4\"}, \
                             {\"type\": \"exec\", \"kind\": \"recovery\", \"end\": {\"seconds\": 90}, \
                              \"note\": \"slow jog\"}]}, \
                           {\"type\": \"exec\", \"kind\": \"cooldown\", \"end\": {\"seconds\": 300}, \
                            \"note\": \"easy jog Z1\"}]"
            }),
        );

        let draft = out.draft.expect("the model's own output should now draft");
        assert_eq!(draft.name, "4 x 3min Z4 Hard");
        assert_eq!(draft.steps.len(), 3);
        // 300 + 4 × (180 + 90) + 300 = 1680
        assert_eq!(draft.est_duration_secs(), Some(1680.0));
        // A stray `kind` on the repeat group is ignored rather than fatal.
        assert_eq!(out.result["steps"], 10);
    }

    /// Each repair on its own, so a failure names which one broke.
    #[test]
    fn repair_recovers_the_tag_from_the_fields_that_are_present() {
        let drafted = |v: Value| run_tool("draft_workout", &v).draft;

        // An untagged end, an untagged target, a bare number, minutes for
        // seconds, and the American spelling — none of which parse raw.
        let d = drafted(json!({
            "name": "Mixed", "sport": "running",
            "steps": [
                { "kind": "warmup", "end": { "minutes": 5 } },
                { "kind": "interval", "end": 120, "target": { "zone": 4 } },
                { "kind": "interval", "end": { "meters": 400 }, "target": "Z5" },
                { "kind": "recovery", "end": { "low": 1 }, "target": { "low": 110, "high": 125 } },
            ]
        }))
        .expect("every field is recoverable");

        let p = d.payload();
        let s = p["workoutSegments"][0]["workoutSteps"].as_array().unwrap();
        assert_eq!(s[0]["endConditionValue"], 300.0, "minutes became seconds");
        assert_eq!(s[1]["endConditionValue"], 120.0, "a bare number is seconds");
        assert_eq!(s[1]["zoneNumber"], 4);
        assert_eq!(s[2]["endCondition"]["conditionTypeKey"], "distance");
        assert_eq!(s[2]["endConditionValue"], 400.0, "meters became metres");
        assert_eq!(s[2]["zoneNumber"], 5, "\"Z5\" is zone 5");
        assert!(s[3]["zoneNumber"].is_null(), "a bpm target sets no zone");
        assert_eq!(s[3]["targetValueOne"], 110.0);
        // No recognisable end field at all is the lap button, not a failure.
        assert_eq!(s[3]["endCondition"]["conditionTypeKey"], "lap.button");

        // A repeat inferred from `times` alone, with its steps also stringified.
        let d = drafted(json!({
            "name": "Reps", "sport": "running",
            "steps": [{ "times": 3, "steps": "[{\"kind\":\"interval\",\"end\":{\"seconds\":60}}]" }]
        }))
        .expect("`times` alone marks a repeat");
        assert_eq!(d.flat_count(), 3);

        // One step sent bare instead of as a list of one.
        let d = drafted(json!({
            "name": "Single", "sport": "running",
            "steps": { "kind": "interval", "end": { "seconds": 600 } }
        }))
        .expect("a lone step is a list of one");
        assert_eq!(d.steps.len(), 1);
    }

    /// Repair re-labels; it must not rescue a workout that is actually wrong.
    #[test]
    fn repair_does_not_soften_validation() {
        let out = run_tool(
            "draft_workout",
            &json!({
                "name": "Nope", "sport": "running",
                "steps": [{ "kind": "interval", "end": { "minutes": 2000 } }]
            }),
        );
        assert!(out.draft.is_none(), "2000 minutes is still too long");
        assert!(out.result["error"].as_str().unwrap().contains("step 1"));
    }

    /// A rejected draft has to come back as something the model can act on,
    /// and must not reach the athlete as a card. Both halves matter: an invalid
    /// workout offered for confirmation is a button that fails when pressed.
    #[test]
    fn a_bad_draft_is_a_complaint_and_never_a_card() {
        // Minutes where seconds go — the units mistake, not a schema violation.
        let out = run_tool(
            "draft_workout",
            &json!({
                "name": "Long",
                "sport": "running",
                "steps": [{ "type": "exec", "kind": "interval",
                            "end": { "type": "time", "seconds": 90000 } }]
            }),
        );
        assert!(out.draft.is_none());
        let err = out.result["error"].as_str().unwrap_or_default();
        assert!(
            err.contains("step 1") && err.contains("seconds"),
            "got: {err}"
        );

        // A repeat missing the field that makes it one. serde catches this
        // before validation does, and the message still has to name the field.
        let out = run_tool(
            "draft_workout",
            &json!({
                "name": "Reps",
                "sport": "running",
                "steps": [{ "type": "repeat", "steps": [] }]
            }),
        );
        assert!(out.draft.is_none());
        assert!(out.result["error"].as_str().unwrap().contains("times"));

        // Empty arguments, which is what a model sends when it has decided to
        // call the tool and not what to put in it.
        let out = run_tool("draft_workout", &json!({}));
        assert!(out.draft.is_none());
        assert!(out.result["error"].is_string());
    }

    fn msg(role: &str, content: &str) -> HistoryMessage {
        HistoryMessage {
            role: role.into(),
            content: content.into(),
        }
    }

    /// A short conversation is sent whole; a long one keeps its subject and its
    /// tail. The failure this guards against is silent and expensive — history
    /// is re-sent on every round, so an unbounded transcript costs more each
    /// turn without anything on screen changing.
    #[test]
    fn history_is_trimmed_to_its_subject_and_its_tail() {
        let short: Vec<_> = (0..6)
            .map(|i| msg(if i % 2 == 0 { "user" } else { "assistant" }, "hi"))
            .collect();
        assert_eq!(trim_history(short.clone()).len(), short.len(), "left alone");

        // Twenty turns of nothing much: over the message cap, under the byte one.
        let long: Vec<_> = (0..20)
            .map(|i| {
                msg(
                    if i % 2 == 0 { "user" } else { "assistant" },
                    &format!("message {i}"),
                )
            })
            .collect();
        let cut = trim_history(long.clone());
        assert!(cut.len() <= HISTORY_MESSAGES + 1, "got {}", cut.len());
        assert_eq!(cut[0].content, "message 0", "the opening question stays");
        assert_eq!(
            cut.last().unwrap().content,
            "message 19",
            "the question just asked stays"
        );
        assert_eq!(cut[1].role, "user", "the tail opens on a user turn");

        // A few enormous answers: under the message cap, over the byte one.
        let fat: Vec<_> = (0..8)
            .map(|i| {
                msg(
                    if i % 2 == 0 { "user" } else { "assistant" },
                    &"x".repeat(6_000),
                )
            })
            .collect();
        let cut = trim_history(fat);
        let chars: usize = cut.iter().map(|m| m.content.len()).sum();
        assert!(chars <= HISTORY_CHARS + 6_000, "{chars} chars survived");

        // One message longer than the entire budget is still the question.
        let huge = vec![msg("user", &"y".repeat(HISTORY_CHARS * 2))];
        assert_eq!(trim_history(huge).len(), 1);
    }

    /// The model picks these numbers and the cache has thousands of rows behind
    /// them. Whatever comes back is re-sent on every remaining round, so an
    /// unclamped argument is billed six more times.
    #[test]
    fn tool_windows_are_clamped() {
        assert_eq!(window(&json!({}), "limit", 10, MAX_ACTIVITIES), 10);
        // An explicit value wins over a different default.
        assert_eq!(
            window(&json!({ "limit": 5 }), "limit", 10, MAX_ACTIVITIES),
            5
        );
        assert_eq!(
            window(&json!({ "limit": 5000 }), "limit", 10, MAX_ACTIVITIES),
            MAX_ACTIVITIES as u32
        );
        // Zero would return nothing at all, which reads to the model as "no
        // data" rather than "you asked for none".
        assert_eq!(window(&json!({ "days": 0 }), "days", 14, MAX_DAYS), 1);
        // Junk falls back to the default rather than to zero.
        assert_eq!(window(&json!({ "days": "lots" }), "days", 14, MAX_DAYS), 14);

        // And the label says what was read, not what was asked for.
        assert!(describe("recent_activities", &json!({ "limit": 5000 }))
            .contains(&MAX_ACTIVITIES.to_string()));
    }

    /// The breakpoint is what makes the fixed prefix free to re-send; Ollama is
    /// local, so it gets the plain string it expects.
    #[test]
    fn the_cache_breakpoint_rides_on_the_system_turn() {
        let hosted = system_message(Provider::Openrouter);
        assert_eq!(hosted["content"][0]["text"], SYSTEM_PROMPT);
        assert_eq!(hosted["content"][0]["cache_control"]["type"], "ephemeral");

        let local = system_message(Provider::Ollama);
        assert_eq!(local["content"], SYSTEM_PROMPT);

        // The proxy is OpenRouter one hop later, and the prefix it re-sends is
        // billed to whoever runs it rather than to the person asking.
        let proxied = system_message(Provider::Cloud);
        assert_eq!(proxied["content"][0]["cache_control"]["type"], "ephemeral");
    }

    /// The stored provider string is a database value — it outlives the build
    /// that wrote it, so the two directions have to keep agreeing.
    #[test]
    fn every_provider_survives_a_round_trip_through_the_cache() {
        for p in [Provider::Cloud, Provider::Openrouter, Provider::Ollama] {
            assert_eq!(Provider::parse(p.as_str()), Some(p));
        }
        assert_eq!(Provider::parse("anthropic"), None);

        // Only Ollama keeps the question on this machine. `hosted` gates the
        // usage accounting and the warning under the Ask box, so a new provider
        // defaulting to the wrong side of this is worth failing over.
        assert!(Provider::Cloud.hosted());
        assert!(Provider::Openrouter.hosted());
        assert!(!Provider::Ollama.hosted());

        // The proxy serves exactly one model, and the app has to name the same
        // one the worker allows.
        assert_eq!(Provider::Cloud.default_model(), Some(CLOUD_MODEL));
        assert_eq!(Provider::Ollama.default_model(), None);
    }

    /// A refused hosted request has to say what to do about it. "cloud returned
    /// 429" is the sentence this replaced, and it left people with nothing.
    #[test]
    fn the_proxy_refusals_say_what_to_do_next() {
        let over = refusal(
            Provider::Cloud,
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "rate limited",
        );
        assert!(over.contains("Settings"), "got: {over}");
        assert!(!over.contains("429"), "got: {over}");

        // Anything the worker didn't mean to say is shown as it arrived.
        let odd = refusal(
            Provider::Cloud,
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "upstream exploded",
        );
        assert!(odd.contains("upstream exploded"), "got: {odd}");

        // And the athlete's own key is their business, not the proxy's.
        let theirs = refusal(
            Provider::Openrouter,
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "slow down",
        );
        assert!(theirs.contains("slow down"), "got: {theirs}");
    }

    /// Suggestions cost a request each, and an error or a one-liner has nothing
    /// to suggest about. The gate is checked before anything is opened or sent,
    /// so this runs with no cache and no key.
    #[tokio::test]
    async fn followups_are_not_worth_a_request_for_every_answer() {
        assert!(followups(vec![]).await.unwrap().is_empty());
        assert!(followups(vec![msg("assistant", "No data for that.")])
            .await
            .unwrap()
            .is_empty());
        // A question with no answer after it isn't a turn that has ended.
        assert!(followups(vec![msg("user", &"why? ".repeat(60))])
            .await
            .unwrap()
            .is_empty());
    }

    /// The banner reports the provider that is selected now, not whichever one
    /// happened to fail last.
    ///
    /// Switching provider *is* what someone does about a broken one, so leaving
    /// the old failure on screen afterwards would say the fix hadn't worked.
    #[test]
    fn a_verdict_about_another_provider_is_not_reported() {
        note_call(Provider::Openrouter, Err("402 out of credit".into()));

        let mine = health(Some(Provider::Openrouter)).expect("about the current provider");
        assert!(!mine.ok);
        assert_eq!(mine.message.as_deref(), Some("402 out of credit"));

        assert!(
            health(Some(Provider::Ollama)).is_none(),
            "a failure at OpenRouter says nothing about a local Ollama"
        );
        assert!(
            health(None).is_none(),
            "with nothing configured there is nothing to be broken"
        );

        // And a success clears it rather than leaving the last failure standing.
        note_call(Provider::Openrouter, Ok(()));
        let after = health(Some(Provider::Openrouter)).expect("still a verdict");
        assert!(after.ok);
        assert!(after.message.is_none());
    }

    /// Only a tool that actually changed something may announce that it did.
    ///
    /// The event this rides on makes every open screen re-read the themes
    /// folder and, for `use_theme`, repaint the window. Firing it from a call
    /// that failed would repaint over whatever the athlete had chosen, on the
    /// strength of a tool call that did nothing.
    #[test]
    fn only_a_theme_change_that_happened_is_announced() {
        // Reads nothing, changes nothing.
        assert_eq!(run_tool("list_themes", &json!({})).themes, None);

        // Both of these are rejected before anything is written.
        assert_eq!(run_tool("save_theme", &json!({})).themes, None);
        assert_eq!(run_tool("delete_theme", &json!({})).themes, None);
        assert_eq!(run_tool("use_theme", &json!({})).themes, None);
        assert_eq!(
            run_tool("use_theme", &json!({ "slug": "no-such-theme-exists" })).themes,
            None,
            "a slug that names nothing must not repaint the window"
        );

        // Going back to the built-in palette needs no file to exist.
        assert_eq!(
            run_tool("use_theme", &json!({ "slug": "default" })).themes,
            Some(ThemeChange {
                apply: Some(String::new())
            }),
        );

        // And nothing else in the app may touch themes at all.
        for name in tool_names() {
            if name.ends_with("_theme") || name.ends_with("_themes") {
                continue;
            }
            assert_eq!(
                run_tool(&name, &json!({})).themes,
                None,
                "{name} announced a theme change"
            );
        }
    }

    /// Every other tool must keep leaving `draft` alone — a card appearing
    /// under an answer about last week's cadence would be inexplicable.
    #[test]
    fn only_draft_workout_produces_a_draft() {
        for name in ["cache_status", "workouts", "routes", "recovery"] {
            assert!(
                run_tool(name, &json!({})).draft.is_none(),
                "{name} produced a workout draft"
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

        run_turn(handle, "test".into(), history, None)
            .await
            .expect("the turn should complete");

        let got = received.lock().unwrap().clone();
        println!("--- stream events ---\n{got}\n---------------------");
        assert!(got.contains("delta"), "the model streamed no text");
        assert!(got.contains("done"), "the turn never finished");
    }
}
