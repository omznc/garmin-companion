//! The "OpenCode" coach: one turn runs the `opencode` command of the user.
//!
//! It works like `claude_code.rs`: the command reads the app's tools over MCP
//! and has no other tool. The turn is stateless and uses no cloud credits.
//!
//! OpenCode has no flag for a per-run MCP server. The app makes a temporary
//! config folder and points `XDG_CONFIG_HOME` at it. That hides the user's own
//! plugins, MCP servers and agents. The login stays, because OpenCode keeps it
//! in its data folder (`auth.json` in OpenCode 1, `opencode.db` in OpenCode 2).
//! The last used model stays too, because it is in the state folder. A model
//! or a provider that only the user's config defines is not visible to a turn.
//! The free OpenCode models refuse a request with no `read` tool. The app keeps
//! `read` denied, so a turn needs a login for a paid model.
//!
//! `opencode run --format json` writes one JSON event per line. It sends whole
//! text parts, not token deltas.

use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use garmin_coach::external::transcript;
use garmin_coach::{Event, EventSink, HistoryMessage, Period};
use serde_json::{json, Value};

use crate::cli_agent::{self as cli, error, Answer, Cli, Run, Status, Stream};

pub const CLI: Cli = Cli {
    name: "OpenCode",
    bin: "opencode",
    login_hint: "Run `opencode auth login` once.",
    home_dirs: &[".opencode/bin", ".local/bin", ".bun/bin"],
    env_remove: &[],
};

const NOT_INSTALLED: &str =
    "OpenCode is not installed. Install it from https://opencode.ai and log in with `opencode auth login`.";
/// OpenCode names an MCP tool `<server>_<tool>`.
const TOOL_PREFIX: &str = "garmin_";
const AGENT: &str = "coach";
/// How many model steps one turn may take. The same limit as Claude Code.
const STEPS: u32 = 8;
/// OpenCode waits 5 seconds for the list of tools by default. A cold start of
/// the app can take longer.
const MCP_TIMEOUT_MS: u32 = 30_000;

/* ------------------------------------------------------------------ parser --- */

#[derive(Default)]
struct Parser {
    answer: Answer,
    sources: Vec<String>,
    finished: bool,
}

impl Stream for Parser {
    fn feed(&mut self, line: &Value) -> Vec<Event> {
        let part = &line["part"];
        match line["type"].as_str() {
            Some("text") => self.answer.add(part["text"].as_str().unwrap_or_default()),
            Some("tool_use") => self.tool(part),
            Some("step_finish") => self.step_finish(part),
            Some("error") => {
                self.finished = true;
                vec![error(&error_text(&line["error"]))]
            }
            _ => Vec::new(),
        }
    }

    fn finished(&self) -> bool {
        self.finished
    }

    /// OpenCode 2 can end a turn with no `step_finish` after the last text.
    fn end(&mut self) -> Vec<Event> {
        if self.finished {
            return Vec::new();
        }
        let events = self.answer.done(self.sources());
        // No text means no answer: `watch` then reports the failure of the command.
        if !matches!(events[..], [Event::Done { .. }]) {
            return Vec::new();
        }
        self.finished = true;
        events
    }

    fn sources(&self) -> Vec<String> {
        self.sources.clone()
    }
}

impl Parser {
    /// A tool call comes once, already finished. A denied tool has the status `error`.
    fn tool(&mut self, part: &Value) -> Vec<Event> {
        let mut events = self.answer.note();
        // OpenCode 2 has no `callID`: the part `id` is the call id.
        let call = part["callID"]
            .as_str()
            .or_else(|| part["id"].as_str())
            .unwrap_or_default();
        let name = part["tool"].as_str().unwrap_or_default();
        let name = name.strip_prefix(TOOL_PREFIX).unwrap_or(name);
        let ok = part["state"]["status"] != "error";
        let event = cli::tool_event(call, name, &part["state"]["input"], true, ok);
        if let Event::Tool { label, .. } = &event {
            self.sources.push(label.clone());
        }
        events.push(event);
        events
    }

    /// A step that ends with tool calls is not the last one. Any other reason is.
    fn step_finish(&mut self, part: &Value) -> Vec<Event> {
        if matches!(part["reason"].as_str(), Some("tool-calls" | "unknown")) {
            return self.answer.note();
        }
        self.finished = true;
        self.answer.done(self.sources())
    }
}

fn error_text(err: &Value) -> String {
    [&err["data"]["message"], &err["message"], &err["name"]]
        .into_iter()
        .find_map(|v| v.as_str().filter(|s| !s.is_empty()))
        .unwrap_or("OpenCode reported an error.")
        .into()
}

/* ----------------------------------------------------------------- command --- */

/// The files of the temporary config folder: `opencode.json` and the prompt.
/// Only the tools of the app are allowed.
fn config(app: &Path, db: Option<&Path>, period: Option<Period>) -> Value {
    let mut command = vec![app.to_string_lossy().into_owned()];
    command.extend(cli::mcp_args(db, period));
    json!({
        "$schema": "https://opencode.ai/config.json",
        "mcp": { "garmin": {
            "type": "local",
            "command": command,
            "enabled": true,
            "timeout": MCP_TIMEOUT_MS,
        } },
        // OpenCode 2 gives the model the MCP tools only through its `execute`
        // tool. That tool runs code with no file access, but its `fetch` can
        // make HTTP requests. OpenCode 2 has no setting to block it.
        "permission": { "*": "deny", "garmin_*": "allow", "execute": "allow" },
        "agent": { AGENT: {
            "mode": "primary",
            "description": "Running coach",
            // The file sits next to `opencode.json`.
            "prompt": "{file:./coach.md}",
            "steps": STEPS,
        } },
    })
}

/// The text of `opencode run --help` on stdout. The flags differ between
/// versions. The call is short, so each turn makes it again. An empty text
/// means the call failed, or the version writes its help to stderr.
fn run_help(bin: &Path) -> String {
    cli::read(bin, &CLI, &["run", "--help"], cli::PROBE_LIMIT)
        .map(|o| o.text)
        .unwrap_or_default()
}

/// `--auto` would approve more than the tools of the app, so it is not here.
/// OpenCode takes a model as `provider/model`, so another form is ignored.
/// The question goes on stdin: `run` with no message reads it there. The work
/// folder is the working directory of the command, so no `--dir` is needed.
///
/// OpenCode 2 sends a run to its background service, which ignores our config
/// folder. `--standalone` starts a private server that reads it. OpenCode 1
/// has no such flag and does not need it.
fn flags(help: &str, model: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["run", "--format", "json", "--agent", AGENT]
        .into_iter()
        .map(Into::into)
        .collect();
    if help.contains("--standalone") {
        args.push("--standalone".into());
    }
    if model.contains('/') {
        args.extend(["-m".into(), model.into()]);
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
    let home = cli::temp_dir("opencode").map_err(|e| format!("Could not make the config: {e}"))?;
    let folder = home.path().join("opencode");
    std::fs::create_dir(&folder).map_err(|e| format!("Could not make the config: {e}"))?;
    let write = |name: &str, text: String| {
        std::fs::write(folder.join(name), text)
            .map_err(|e| format!("Could not write the config: {e}"))
    };
    write("coach.md", setup.system.clone())?;
    write("opencode.json", config(&setup.app, db, period).to_string())?;

    let args = flags(&run_help(&setup.bin), model);
    let run = Run {
        cli: &CLI,
        bin: setup.bin,
        args,
        cwd: setup.cwd,
        env: vec![
            ("XDG_CONFIG_HOME", home.path().into()),
            ("OPENCODE_DISABLE_AUTOUPDATE", "1".into()),
            // OpenCode 1: no external plugins. The same as `--pure`, which
            // OpenCode 2 does not have.
            ("OPENCODE_PURE", "1".into()),
        ],
        stdin: Some(transcript(history)),
        limit: None,
    };
    cli::run_child(run, &mut Parser::default(), sink, stop)
}

/* ------------------------------------------------------------------ status --- */

/// Ask the command for its version. It blocks, so call it off the UI thread.
/// OpenCode has no cheap login check: keys can come from the environment too.
/// A missing login shows as an `error` event in the first turn.
pub fn probe() -> Status {
    match cli::find(&CLI) {
        Some(bin) => probe_with(&bin, cli::PROBE_LIMIT),
        None => Status::NotInstalled,
    }
}

fn probe_with(bin: &Path, limit: Duration) -> Status {
    match cli::read(bin, &CLI, &["--version"], limit) {
        Ok(v) if v.ok && !v.text.trim().is_empty() => Status::Ready {
            version: cli::version_of(&v.text),
            who: String::new(),
        },
        Ok(_) => Status::NotInstalled,
        Err(status) => status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(fixture: &str) -> Vec<Event> {
        let mut parser = Parser::default();
        fixture
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .flat_map(|v| parser.feed(&v))
            .collect()
    }

    /// Needs an `opencode` with a model. Run with `GC_LIVE_DB=<cache copy>`,
    /// `APPIMAGE=<binary>` and `GC_LIVE_MODEL=<provider>/<paid model>`. A free model refuses a turn with no
    /// `read` tool.
    #[test]
    #[ignore]
    fn a_real_turn_reads_the_copy_over_mcp() {
        let model = std::env::var("GC_LIVE_MODEL").unwrap_or_default();
        crate::cli_agent::test_support::live_turn(run_turn, &model);
    }

    #[test]
    fn a_tool_turn_gives_a_tool_event_then_the_answer_then_done() {
        let all = events(include_str!("../tests/opencode-tool-turn.jsonl"));
        let label = "Reading 2 days of recovery signals";
        assert_eq!(
            all,
            vec![
                Event::Tool {
                    call_id: "call_00_r0bc".into(),
                    label: label.into(),
                    running: false,
                    ok: true,
                },
                Event::Delta {
                    text: "Your resting HR yesterday was **48 bpm**.".into()
                },
                Event::Done {
                    sources: vec![label.into()]
                },
            ]
        );
    }

    #[test]
    fn text_before_a_tool_call_becomes_a_note() {
        let all = events(
            r#"{"type":"text","part":{"text":" Let me check. "}}
{"type":"tool_use","part":{"callID":"c1","tool":"garmin_recovery","state":{"status":"completed","input":{"days":2}}}}"#,
        );
        assert_eq!(
            all[..2],
            [
                Event::Delta {
                    text: " Let me check. ".into()
                },
                Event::Note {
                    text: "Let me check.".into()
                }
            ]
        );
    }

    #[test]
    fn a_denied_tool_is_a_failed_step() {
        let all = events(
            r#"{"type":"tool_use","part":{"callID":"c1","tool":"bash","state":{"status":"error","input":{}}}}"#,
        );
        assert!(matches!(
            &all[0],
            Event::Tool {
                ok: false,
                running: false,
                ..
            }
        ));
    }

    #[test]
    fn an_error_event_ends_the_turn_with_its_message() {
        let all = events(include_str!("../tests/opencode-error.jsonl"));
        assert_eq!(all.len(), 1);
        assert!(matches!(&all[0], Event::Error { text } if text.contains("free tier")));
        let mut p = Parser::default();
        p.feed(&json!({"type":"error","error":{"name":"UnknownError"}}));
        assert!(p.finished);
    }

    #[test]
    fn a_turn_with_no_text_is_an_error() {
        let all = events(r#"{"type":"step_finish","part":{"reason":"stop"}}"#);
        assert_eq!(all, vec![error(cli::NO_ANSWER)]);
    }

    #[test]
    fn the_config_allows_only_the_garmin_tools() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        let period = Some(Period {
            start: day,
            end: day,
        });
        let v = config(Path::new("/app"), Some(Path::new("/c.db")), period);
        assert_eq!(
            v["permission"],
            json!({ "*": "deny", "garmin_*": "allow", "execute": "allow" })
        );
        let server = &v["mcp"]["garmin"];
        assert_eq!(
            server["command"],
            json!([
                "/app",
                "mcp",
                "--db",
                "/c.db",
                "--focus",
                "2026-03-05..2026-03-05"
            ])
        );
        assert!(server["timeout"].as_u64().unwrap() > 5000);
        assert_eq!(v["agent"]["coach"]["steps"], 8);
    }

    #[test]
    fn the_flags_leave_out_auto_and_a_model_without_a_provider() {
        let text = |args: Vec<OsString>| -> Vec<String> {
            args.iter().map(|a| a.to_string_lossy().into()).collect()
        };
        let a = text(flags("", "sonnet"));
        assert!(!a.contains(&"-m".to_string()) && !a.contains(&"--auto".to_string()));
        let b = text(flags("", "anthropic/claude-x"));
        let at = b.iter().position(|x| x == "-m").unwrap();
        assert_eq!(b[at + 1], "anthropic/claude-x");
        assert!(!b.contains(&"--dir".to_string()));
    }

    #[test]
    fn a_flag_goes_in_only_when_the_help_lists_it() {
        let text = |help| -> Vec<String> {
            flags(help, "")
                .iter()
                .map(|a| a.to_string_lossy().into())
                .collect()
        };
        let v1 = text("  --dir  directory");
        assert!(!v1.contains(&"--standalone".into()) && !v1.contains(&"--pure".into()));
        let v2 = text("  --standalone   Run with a private server\n  --auto");
        assert!(v2.contains(&"--standalone".into()) && !v2.contains(&"--pure".into()));
        assert!(!v2.contains(&"--dir".into()) && !v2.contains(&"--auto".into()));
    }

    #[test]
    fn opencode_2_tool_parts_and_unknown_step_reasons_work() {
        let mut p = Parser::default();
        let events = p.feed(
            &json!({"type":"tool_use","part":{"id":"call_1","tool":"execute",
            "state":{"status":"completed","input":{}}}}),
        );
        assert!(matches!(&events[0], Event::Tool { call_id, .. } if call_id == "call_1"));
        p.feed(&json!({"type":"text","part":{"text":"Answer."}}));
        p.feed(&json!({"type":"step_finish","part":{"reason":"unknown"}}));
        assert!(!p.finished);
    }

    #[test]
    fn a_turn_that_ends_after_the_text_with_no_step_finish_is_done() {
        let mut p = Parser::default();
        p.feed(&json!({"type":"text","part":{"text":"Answer."}}));
        assert!(matches!(&p.end()[..], [Event::Done { .. }]));
        assert!(p.finished);
        assert!(Parser::default().end().is_empty());
    }
}
