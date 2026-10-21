//! The "Codex" coach: one turn runs the `codex` command of the user.
//!
//! It works like `claude_code.rs`: the command reads the app's tools over MCP
//! and has no other tool. The turn is stateless and uses no cloud credits.
//!
//! `codex exec` takes the MCP server and the other settings as `-c` overrides,
//! so the config file of the user stays as it is. `--ignore-user-config` skips
//! that file, and the login stays. The command has no limit on turns, so the
//! app stops a run that takes too long.
//!
//! Not tested with a real Codex install yet. The tests read sample events that
//! follow the documented event types.

use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use garmin_coach::external::transcript;
use garmin_coach::{Event, EventSink, HistoryMessage, Period};
use serde_json::{json, Value};

use crate::cli_agent::{self as cli, error, Answer, Cli, Run, Status, Stream};

pub const CLI: Cli = Cli {
    name: "Codex",
    bin: "codex",
    login_hint: "Run `codex login` once.",
    home_dirs: &[".local/bin"],
    env_remove: &[],
};

const NOT_INSTALLED: &str =
    "Codex is not installed. Install it from https://developers.openai.com/codex and run `codex login`.";
/// Codex has no limit on turns. This is the longest run that the app allows.
const RUN_LIMIT: Duration = Duration::from_secs(300);
/// Codex waits 10 seconds for the server to start by default.
const START_SECS: u32 = 30;

/* ------------------------------------------------------------------ parser --- */

#[derive(Default)]
struct Parser {
    answer: Answer,
    sources: Vec<String>,
    finished: bool,
    /// The last `error` event. Codex also sends it while it retries a request,
    /// and `will_retry` is not in the JSON. Only `turn.failed` ends the turn.
    last_error: Option<String>,
}

impl Stream for Parser {
    fn feed(&mut self, line: &Value) -> Vec<Event> {
        match line["type"].as_str() {
            Some("item.started") => self.item(&line["item"], false),
            Some("item.completed") => self.item(&line["item"], true),
            Some("turn.completed") => {
                self.finished = true;
                self.answer.done(self.sources())
            }
            Some("turn.failed") => self.fail(&line["error"]["message"]),
            Some("error") => {
                self.last_error = line["message"].as_str().map(Into::into);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn finished(&self) -> bool {
        self.finished
    }

    fn failure(&self) -> Option<String> {
        self.last_error.clone()
    }

    fn sources(&self) -> Vec<String> {
        self.sources.clone()
    }
}

impl Parser {
    fn item(&mut self, item: &Value, done: bool) -> Vec<Event> {
        match item["type"].as_str() {
            Some("agent_message") if done => {
                self.answer.add(item["text"].as_str().unwrap_or_default())
            }
            Some("mcp_tool_call") => self.tool(item, done),
            // With the shell tool off, a command or a file change should not come.
            _ => Vec::new(),
        }
    }

    fn tool(&mut self, item: &Value, done: bool) -> Vec<Event> {
        let mut events = if done { Vec::new() } else { self.answer.note() };
        // `ok` has no meaning while the call runs.
        let ok = !done || (item["status"] == "completed" && item["error"].is_null());
        let id = item["id"].as_str().unwrap_or_default();
        let name = item["tool"].as_str().unwrap_or_default();
        let event = cli::tool_event(id, name, &item["arguments"], done, ok);
        if let (true, Event::Tool { label, .. }) = (done, &event) {
            self.sources.push(label.clone());
        }
        events.push(event);
        events
    }

    fn fail(&mut self, message: &Value) -> Vec<Event> {
        self.finished = true;
        let text = message.as_str().filter(|m| !m.is_empty());
        vec![error(text.unwrap_or("Codex reported an error."))]
    }
}

/* ----------------------------------------------------------------- command --- */

/// The `-c key=value` overrides of a run: the app as the only MCP server, no
/// shell tool, no web search, and no question to the person. A JSON string,
/// number, bool or list of strings is also valid TOML.
fn overrides(app: &Path, args: Vec<String>, system: &str) -> Vec<String> {
    let server = "mcp_servers.garmin";
    let pairs = [
        (format!("{server}.command"), json!(app.to_string_lossy())),
        (format!("{server}.args"), json!(args)),
        (format!("{server}.required"), json!(true)),
        (format!("{server}.startup_timeout_sec"), json!(START_SECS)),
        (
            format!("{server}.default_tools_approval_mode"),
            json!("approve"),
        ),
        ("features.shell_tool".into(), json!(false)),
        ("web_search".into(), json!("disabled")),
        ("approval_policy".into(), json!("never")),
        ("developer_instructions".into(), json!(system)),
    ];
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect()
}

/// The prompt comes on stdin (`-`). `--ephemeral` keeps no session file.
/// Without a model, Codex uses the one of the user.
fn flags(cwd: &Path, model: &str, overrides: Vec<String>) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "exec",
        "--json",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--skip-git-repo-check",
        "--sandbox",
        "read-only",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    args.extend(["-C".into(), cwd.into()]);
    if !model.is_empty() {
        args.extend(["-m".into(), model.into()]);
    }
    for pair in overrides {
        args.extend(["-c".into(), pair.into()]);
    }
    args.push("-".into());
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
    let mcp = overrides(&setup.app, cli::mcp_args(db, period), &setup.system);
    let run = Run {
        cli: &CLI,
        bin: setup.bin,
        args: flags(&setup.cwd, model, mcp),
        cwd: setup.cwd,
        env: Vec::new(),
        stdin: Some(transcript(history)),
        limit: Some(RUN_LIMIT),
    };
    cli::run_child(run, &mut Parser::default(), sink, stop)
}

/* ------------------------------------------------------------------ status --- */

/// Ask the command for its version and login. It blocks, so call it off the UI thread.
pub fn probe() -> Status {
    let key = std::env::var_os("CODEX_API_KEY").is_some_and(|k| !k.is_empty());
    match cli::find(&CLI) {
        Some(bin) => probe_with(&bin, cli::PROBE_LIMIT, key),
        None => Status::NotInstalled,
    }
}

/// `codex login status` exits 1 when the user is not logged in. It does not see
/// `CODEX_API_KEY`, though `codex exec` accepts it. So a key skips that check.
fn probe_with(bin: &Path, limit: Duration, api_key: bool) -> Status {
    let version = match cli::read(bin, &CLI, &["--version"], limit) {
        Ok(v) if v.ok && !v.text.trim().is_empty() => v.text,
        Ok(_) => return Status::NotInstalled,
        Err(status) => return status,
    };
    if !api_key {
        match cli::read(bin, &CLI, &["login", "status"], limit) {
            Ok(login) if !login.ok => return Status::LoggedOut,
            Err(status) => return status,
            Ok(_) => {}
        }
    }
    Status::Ready {
        version: cli::version_of(&version),
        who: String::new(),
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

    #[test]
    fn a_tool_turn_gives_tool_events_then_the_answer_then_done() {
        let all = events(include_str!("../tests/codex-tool-turn.jsonl"));
        let label = "Reading 2 days of recovery signals";
        let tool = |running, ok| Event::Tool {
            call_id: "item_1".into(),
            label: label.into(),
            running,
            ok,
        };
        assert_eq!(
            all,
            vec![
                Event::Delta {
                    text: "Let me check.".into()
                },
                Event::Note {
                    text: "Let me check.".into()
                },
                tool(true, true),
                tool(false, true),
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
    fn a_failed_tool_call_is_a_failed_step() {
        let all = events(
            r#"{"type":"item.completed","item":{"id":"i","type":"mcp_tool_call","server":"garmin","tool":"recovery","arguments":{},"error":{"message":"x"},"status":"failed"}}"#,
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
    fn a_failed_turn_ends_the_turn() {
        let failed = events(r#"{"type":"turn.failed","error":{"message":"Not logged in"}}"#);
        assert_eq!(failed, vec![error("Not logged in")]);
        assert_eq!(
            events(r#"{"type":"turn.failed"}"#),
            vec![error("Codex reported an error.")]
        );
    }

    #[test]
    fn an_error_event_during_a_retry_does_not_end_the_turn() {
        let mut parser = Parser::default();
        let retry = json!({"type": "error", "message": "Reconnecting... 1/5"});
        assert!(parser.feed(&retry).is_empty());
        assert!(!parser.finished());
        assert_eq!(parser.failure().as_deref(), Some("Reconnecting... 1/5"));
    }

    #[test]
    fn a_turn_with_no_message_is_an_error() {
        let all = events(r#"{"type":"turn.completed","usage":{}}"#);
        assert_eq!(all, vec![error(cli::NO_ANSWER)]);
    }

    #[test]
    fn the_flags_lock_the_run_down_and_take_the_prompt_from_stdin() {
        let mcp = overrides(
            Path::new("C:\\app.exe"),
            vec!["mcp".into(), "--db".into(), "a \"b\".db".into()],
            "Be a coach.\nUse the tools.",
        );
        let text = |args: Vec<OsString>| -> Vec<String> {
            args.iter().map(|a| a.to_string_lossy().into()).collect()
        };
        let a = text(flags(Path::new("/w"), "", mcp.clone()));
        let at = |flag: &str| a.iter().position(|x| x == flag).unwrap();
        assert_eq!(a[0], "exec");
        assert_eq!(a[at("--sandbox") + 1], "read-only");
        assert!(a.contains(&"--ignore-user-config".into()) && !a.contains(&"-m".into()));
        assert_eq!(a.last().unwrap(), "-");
        for want in [
            r#"mcp_servers.garmin.command="C:\\app.exe""#,
            r#"mcp_servers.garmin.args=["mcp","--db","a \"b\".db"]"#,
            "mcp_servers.garmin.required=true",
            r#"mcp_servers.garmin.default_tools_approval_mode="approve""#,
            "features.shell_tool=false",
            r#"developer_instructions="Be a coach.\nUse the tools.""#,
        ] {
            assert!(a.contains(&want.to_string()), "{want}");
        }
        let b = text(flags(Path::new("/w"), "gpt-x", mcp));
        let m = b.iter().position(|x| x == "-m").unwrap();
        assert_eq!(b[m + 1], "gpt-x");
    }

    /// Needs a logged-in `codex`. Run with `GC_LIVE_DB=<cache copy>` and
    /// `APPIMAGE=<release binary>`.
    #[test]
    #[ignore]
    fn a_real_turn_reads_the_copy_over_mcp() {
        crate::cli_agent::test_support::live_turn(run_turn, "");
    }

    #[cfg(unix)]
    #[test]
    fn the_probe_reads_the_version_and_the_login_exit_code() {
        use crate::cli_agent::script_support::{script, serial};
        let _lock = serial();
        let run = |body: &str, key: bool| {
            let bin = script("codex-probe", body);
            let out = probe_with(&bin, Duration::from_secs(5), key);
            let _ = std::fs::remove_dir_all(bin.parent().unwrap());
            out
        };
        let out_in = "[ \"$1\" = login ] && exit 0; echo 'codex-cli 0.46.0'";
        let ready = Status::Ready {
            version: "0.46.0".into(),
            who: String::new(),
        };
        assert_eq!(run(out_in, false), ready);
        let out_out = "[ \"$1\" = login ] && exit 1; echo 'codex-cli 0.46.0'";
        assert_eq!(run(out_out, false), Status::LoggedOut);
        // An API key in the environment skips the login check.
        assert_eq!(run(out_out, true), ready);
        assert_eq!(run("exit 1", false), Status::NotInstalled);
    }
}
