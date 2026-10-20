//! The "Claude Code" coach: one turn runs the `claude` command of the user.
//!
//! The command reads the app's tools over MCP (`mcp.rs`, started as
//! `garmin-companion mcp`). It has no other tool. No cloud credits are used.
//! The turn is stateless: each turn sends the whole trimmed history.
//!
//! The command writes stream-json on stdout. [`Parser`] turns each line into
//! the same `garmin_coach::Event`s as the other providers, so Ask needs no change.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use garmin_coach::external;
use garmin_coach::{Event, EventSink, HistoryMessage, Period, DEFAULT_CLAUDE_MODEL};
use garmin_core::db::Db;
use serde_json::{json, Value};

const NOT_INSTALLED: &str = "Claude Code is not installed. Install it from https://code.claude.com and run `claude` once to log in.";
const NOT_LOGGED_IN: &str = "Claude Code is not logged in. Run `claude` in a terminal and log in.";
const TOOL_PREFIX: &str = "mcp__garmin__";
/// How much of the error output of the command the person sees.
const STDERR_LINES: usize = 5;

/* ------------------------------------------------------------------ parser --- */

/// A tool call whose input is still streaming.
struct Call {
    id: String,
    name: String,
    input: String,
}

/// Turns the lines of the stream into events. It holds the state between lines.
#[derive(Default)]
pub struct Parser {
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
    pub finished: bool,
}

impl Parser {
    pub fn sources(&self) -> Vec<String> {
        self.sources.clone()
    }

    pub fn feed(&mut self, line: &Value) -> Vec<Event> {
        match line["type"].as_str() {
            Some("stream_event") => self.stream(&line["event"]),
            Some("assistant") => {
                self.check_login(&line["message"]);
                Vec::new()
            }
            Some("user") => self.results(&line["message"]["content"]),
            Some("result") => self.result(line),
            _ => Vec::new(),
        }
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
        if self.logged_out || text.contains("Not logged in") {
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
            return vec![error(
                "The model finished without writing an answer. Ask again, or try a different model.",
            )];
        }
        vec![Event::Done {
            sources: self.sources(),
        }]
    }
}

fn error(text: &str) -> Event {
    Event::Error { text: text.into() }
}

/// A tool event. The label reads the finished input, when the call has one.
fn tool_event(call: &Call, done: bool, ok: bool) -> Event {
    let name = call.name.strip_prefix(TOOL_PREFIX).unwrap_or(&call.name);
    let args: Value = serde_json::from_str(&call.input).unwrap_or(json!({}));
    Event::Tool {
        call_id: call.id.clone(),
        label: external::tool_label(name, &args),
        running: !done,
        ok,
    }
}

/* ----------------------------------------------------------------- command --- */

/// Where the `claude` command can be. A GUI start has a small PATH, so the
/// usual install folders come first.
fn candidates(home: Option<PathBuf>, path: Option<OsString>) -> Vec<PathBuf> {
    let exe = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    let mut list = Vec::new();
    if let Some(home) = home {
        list.push(home.join(".local").join("bin").join(exe));
    }
    if !cfg!(windows) {
        list.push("/opt/homebrew/bin/claude".into());
        list.push("/usr/local/bin/claude".into());
    }
    if let Some(path) = path {
        list.extend(std::env::split_paths(&path).map(|dir| dir.join(exe)));
    }
    list
}

pub fn find_claude() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let home = std::env::var_os(var).map(PathBuf::from);
    candidates(home, std::env::var_os("PATH"))
        .into_iter()
        .find(|p| p.is_file())
}

/// The command for a run: no window on Windows, and none of the variables that
/// a Claude Code session sets for its children. `CLAUDE_CONFIG_DIR` stays.
fn command(bin: &Path) -> Command {
    let mut cmd = Command::new(bin);
    cmd.env_remove("CLAUDECODE");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("CLAUDE_CODE_") {
            cmd.env_remove(key);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// The app itself is the MCP server: the AppImage file when there is one.
fn app_path() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
}

fn mcp_config(app: &Path, db: Option<&Path>, period: Option<Period>) -> String {
    let mut args = vec!["mcp".to_string()];
    if let Some(db) = db {
        args.extend(["--db".into(), db.to_string_lossy().into()]);
    }
    if let Some(p) = period {
        args.extend(["--focus".into(), format!("{}..{}", p.start, p.end)]);
    }
    json!({ "mcpServers": { "garmin": {
        "type": "stdio",
        "command": app.to_string_lossy(),
        "args": args,
    } } })
    .to_string()
}

/// The flags of 1.1 in CLI-AGENTS.md. Each empty string is its own item.
fn flags(prompt_file: &Path, model: &str, mcp: String, question: String) -> Vec<OsString> {
    let model = if model.is_empty() {
        DEFAULT_CLAUDE_MODEL
    } else {
        model
    };
    let mut args: Vec<OsString> = vec!["-p".into(), question.into()];
    let pairs = [
        ("--mcp-config", mcp.as_str()),
        ("--tools", ""),
        ("--allowedTools", "mcp__garmin__*"),
        ("--permission-mode", "dontAsk"),
        ("--setting-sources", ""),
        ("--output-format", "stream-json"),
        ("--model", model),
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

/// A file that is removed when it goes out of scope.
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
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
    let bin = find_claude().ok_or(NOT_INSTALLED)?;
    let app = app_path().ok_or("Could not find this app's own path.")?;
    let open = || match db {
        Some(path) => Db::open(path),
        None => Db::open_default(),
    };
    let system = external::mcp_system_prompt(&open().map_err(|e| format!("{e:#}"))?, period)
        .map_err(|e| format!("{e:#}"))?;
    let prompt_file = TempFile(
        std::env::temp_dir().join(format!("garmin-companion-coach-{}.md", std::process::id())),
    );
    std::fs::write(&prompt_file.0, system)
        .map_err(|e| format!("Could not write the prompt: {e}"))?;
    let cwd = work_dir(db).map_err(|e| format!("Could not make the work folder: {e}"))?;

    let mut cmd = command(&bin);
    cmd.args(flags(
        &prompt_file.0,
        model,
        mcp_config(&app, db, period),
        external::transcript(history),
    ))
    .current_dir(cwd)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start Claude Code: {e}"))?;
    watch(&mut child, sink, stop)
}

/// An empty folder that the app owns. The command finds no project files in it.
fn work_dir(db: Option<&Path>) -> std::io::Result<PathBuf> {
    let base = match db {
        None => garmin_core::paths::ensure_base_dir().map_err(std::io::Error::other)?,
        // A demo copy must not write in the data folder of the real cache.
        Some(_) => std::env::temp_dir(),
    };
    let dir = base.join("claude-code-cwd");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Read the stream until the turn ends, the person stops it, or the command dies.
fn watch(child: &mut Child, sink: &dyn EventSink, stop: &AtomicBool) -> Result<(), String> {
    let stdout = child.stdout.take().ok_or("The command has no output.")?;
    let (tx, rx) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
    });
    let stderr = tail_of(child.stderr.take());

    let mut parser = Parser::default();
    while !parser.finished {
        if stop.load(Ordering::Relaxed) {
            end(child);
            sink.emit(Event::Done {
                sources: parser.sources(),
            });
            return Ok(());
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => parser.feed(&line).into_iter().for_each(|e| sink.emit(e)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = reap(child);
    if parser.finished {
        return Ok(());
    }
    let tail = stderr.lock().map(|s| s.clone()).unwrap_or_default();
    let code = status.map_or("none".into(), |c| c.to_string());
    Err(if tail.trim().is_empty() {
        format!("Claude Code stopped without an answer (exit code {code}).")
    } else {
        tail
    })
}

/// Collect the last lines of the error output on a thread.
fn tail_of(stream: Option<impl Read + Send + 'static>) -> Arc<Mutex<String>> {
    let tail = Arc::new(Mutex::new(String::new()));
    if let Some(stream) = stream {
        let tail = tail.clone();
        std::thread::spawn(move || {
            let mut lines: Vec<String> = Vec::new();
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                lines.push(line);
                let from = lines.len().saturating_sub(STDERR_LINES);
                if let Ok(mut t) = tail.lock() {
                    *t = lines[from..].join("\n");
                }
            }
        });
    }
    tail
}

/// Kill the command. Its MCP child ends when its stdin closes.
fn end(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Wait a moment for a clean exit, then kill. Returns the exit code.
fn reap(child: &mut Child) -> Option<i32> {
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        if let Ok(Some(status)) = child.try_wait() {
            return status.code();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    end(child);
    None
}

/* ------------------------------------------------------------------ status --- */

/// What Settings shows about the local command.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    NotInstalled,
    LoggedOut,
    LoggedIn { version: String, email: String },
}

impl Status {
    pub fn line(&self) -> String {
        match self {
            Self::NotInstalled => "Claude Code is not installed.".into(),
            Self::LoggedOut => "Claude Code: not logged in. Run `claude` once.".into(),
            Self::LoggedIn { version, email } => {
                format!("Claude Code {version}, logged in as {email}")
            }
        }
    }

    pub fn ready(&self) -> bool {
        matches!(self, Self::LoggedIn { .. })
    }
}

fn parse_status(version: &str, auth_json: &str) -> Status {
    let auth: Value = serde_json::from_str(auth_json).unwrap_or_default();
    if auth["loggedIn"] != true {
        return Status::LoggedOut;
    }
    Status::LoggedIn {
        version: version.split_whitespace().next().unwrap_or_default().into(),
        email: auth["email"].as_str().unwrap_or("your account").into(),
    }
}

fn output_of(bin: &Path, args: &[&str]) -> Option<String> {
    let out = command(bin)
        .args(args.iter().map(OsStr::new))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Ask the command for its version and login. It blocks, so call it off the UI thread.
pub fn probe() -> Status {
    let Some(bin) = find_claude() else {
        return Status::NotInstalled;
    };
    let version = output_of(&bin, &["--version"]).unwrap_or_default();
    let auth = output_of(&bin, &["auth", "status"]).unwrap_or_default();
    parse_status(&version, &auth)
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
        let args = flags(Path::new("p.md"), "", "{}".into(), "User: hi".into());
        let text: Vec<String> = args.iter().map(|a| a.to_string_lossy().into()).collect();
        let at = |flag: &str| text.iter().position(|a| a == flag).unwrap();
        assert_eq!(text[at("--tools") + 1], "");
        assert_eq!(text[at("--setting-sources") + 1], "");
        assert_eq!(text[at("--model") + 1], "sonnet");
        assert_eq!(text[at("--allowedTools") + 1], "mcp__garmin__*");
        assert_eq!(&text[..2], ["-p", "User: hi"]);
        assert!(text.contains(&"--strict-mcp-config".to_string()));
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

    #[test]
    fn the_install_folders_come_before_the_path() {
        let list = candidates(Some("/h".into()), Some("/usr/bin:/bin".into()));
        assert_eq!(list[0], Path::new("/h/.local/bin/claude"));
        let path_at = list.iter().position(|p| p == Path::new("/usr/bin/claude"));
        assert!(path_at.unwrap() > 0);
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
        assert_eq!(ok.line(), "Claude Code 2.1.304, logged in as a@b.c");
        assert_eq!(
            parse_status("2.1.304", r#"{"loggedIn":false}"#),
            Status::LoggedOut
        );
        assert_eq!(parse_status("", ""), Status::LoggedOut);
    }
}
