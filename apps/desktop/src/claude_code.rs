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
use std::io::{BufRead, BufReader, Read, Write};
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
const NO_TOOLS: &str =
    "Claude Code could not start the Garmin tools of this app. Restart the app and ask again.";
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
    /// The turn ended before the `result` line, and the process still runs.
    aborted: bool,
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
            Some("system") if line["subtype"] == "init" => self.init(line),
            _ => Vec::new(),
        }
    }

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
fn candidates(home: Option<PathBuf>, npm: Option<PathBuf>, path: Option<OsString>) -> Vec<PathBuf> {
    // On Windows, an npm install makes `claude.cmd`, and Rust finds no file
    // without its extension.
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    let mut list = Vec::new();
    if let Some(home) = home {
        list.push(home.join(".local").join("bin").join(names[0]));
    }
    if !cfg!(windows) {
        list.push("/opt/homebrew/bin/claude".into());
        list.push("/usr/local/bin/claude".into());
    }
    // The npm global folder: the default prefix, `%APPDATA%\npm`.
    list.extend(npm.iter().flat_map(|dir| names.iter().map(|n| dir.join(n))));
    if let Some(path) = path {
        list.extend(
            std::env::split_paths(&path).flat_map(|dir| names.iter().map(move |n| dir.join(n))),
        );
    }
    list
}

pub fn find_claude() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let home = std::env::var_os(var).map(PathBuf::from);
    let npm = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("npm"));
    candidates(home, npm, std::env::var_os("PATH"))
        .into_iter()
        .find(|p| p.is_file())
}

/// A `.cmd` or `.bat` file runs through `cmd.exe`, which reads the arguments
/// in its own way. Rust refuses a newline in them, and a quote is risky.
fn is_batch(bin: &Path) -> bool {
    bin.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
}

/// The variables that mark a parent Claude Code session. The other
/// `CLAUDE_CODE_*` variables stay: a login setup can need them
/// (`CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CODE_USE_BEDROCK`). So does `CLAUDE_CONFIG_DIR`.
const PARENT_SESSION_VARS: [&str; 4] = [
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SSE_PORT",
];

/// The command for a run: no window on Windows, and none of the variables that
/// a parent Claude Code session sets for its children.
fn command(bin: &Path) -> Command {
    let mut cmd = Command::new(bin);
    for key in PARENT_SESSION_VARS {
        cmd.env_remove(key);
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
        .or_else(|| std::env::current_exe().ok().map(live_path))
}

/// After an update replaces the binary, Linux names the running file
/// "<path> (deleted)". The new binary at <path> serves the tools as well.
fn live_path(exe: PathBuf) -> PathBuf {
    match exe.to_str().and_then(|s| s.strip_suffix(" (deleted)")) {
        Some(path) => PathBuf::from(path),
        None => exe,
    }
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

    let mut mcp = mcp_config(&app, db, period);
    let mut question = Some(external::transcript(history));
    let batch = is_batch(&bin);
    // A batch file cannot take the question or the JSON in its arguments.
    let config_file = TempFile(prompt_file.0.with_extension("json"));
    if batch {
        std::fs::write(&config_file.0, &mcp)
            .map_err(|e| format!("Could not write the tool config: {e}"))?;
        mcp = config_file.0.to_string_lossy().into();
    }
    let stdin_text = if batch { question.take() } else { None };

    let mut cmd = command(&bin);
    cmd.args(flags(&prompt_file.0, model, mcp, question))
        .current_dir(cwd)
        .stdin(if batch { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start Claude Code: {e}"))?;
    if let (Some(mut stdin), Some(text)) = (child.stdin.take(), stdin_text) {
        std::thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
        });
    }
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
        for line in lossy_lines(stdout) {
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
    if parser.aborted {
        end(child);
        return Ok(());
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

/// The lines of a stream. A line that is not UTF-8 gets replacement
/// characters, and the reader goes on. It stops only at a read error.
fn lossy_lines(stream: impl Read) -> impl Iterator<Item = String> {
    let mut reader = BufReader::new(stream);
    std::iter::from_fn(move || {
        let mut buf = Vec::new();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(
                String::from_utf8_lossy(&buf)
                    .trim_end_matches(['\r', '\n'])
                    .to_string(),
            ),
        }
    })
}

/// Collect the last lines of the error output on a thread.
fn tail_of(stream: Option<impl Read + Send + 'static>) -> Arc<Mutex<String>> {
    let tail = Arc::new(Mutex::new(String::new()));
    if let Some(stream) = stream {
        let tail = tail.clone();
        std::thread::spawn(move || {
            let mut lines: Vec<String> = Vec::new();
            for line in lossy_lines(stream) {
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
    /// The command did not answer within `PROBE_LIMIT`.
    TimedOut,
    LoggedIn {
        version: String,
        email: String,
    },
}

impl Status {
    pub fn line(&self) -> String {
        match self {
            Self::NotInstalled => "Claude Code is not installed.".into(),
            Self::TimedOut => "Claude Code did not answer in time. Try again.".into(),
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

/// How long one probe call may run before the command is killed.
const PROBE_LIMIT: Duration = Duration::from_secs(10);

/// Run `bin` and return what it wrote on stdout. A call that runs longer than
/// `limit` is killed and gives a `TimedOut` error.
fn output_of(bin: &Path, args: &[&str], limit: Duration) -> std::io::Result<String> {
    let mut child = command(bin)
        .args(args.iter().map(OsStr::new))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child.stdout.take();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut out) = stdout {
            let _ = out.read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    match rx.recv_timeout(limit) {
        Ok(buf) => {
            let _ = child.wait();
            Ok(String::from_utf8_lossy(&buf).into_owned())
        }
        Err(_) => {
            end(&mut child);
            Err(std::io::ErrorKind::TimedOut.into())
        }
    }
}

/// Ask the command for its version and login. It blocks, so call it off the UI thread.
pub fn probe() -> Status {
    let Some(bin) = find_claude() else {
        return Status::NotInstalled;
    };
    probe_with(&bin, PROBE_LIMIT)
}

fn probe_with(bin: &Path, limit: Duration) -> Status {
    let read = |args: &[&str]| match output_of(bin, args, limit) {
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Err(Status::TimedOut),
        out => Ok(out.unwrap_or_default()),
    };
    let version = match read(&["--version"]) {
        Ok(v) => v,
        Err(status) => return status,
    };
    match read(&["auth", "status"]) {
        Ok(auth) => parse_status(&version, &auth),
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
    fn a_replaced_binary_points_to_the_new_file() {
        let path = PathBuf::from("/opt/gc/garmin-companion");
        assert_eq!(live_path("/opt/gc/garmin-companion (deleted)".into()), path);
        assert_eq!(live_path(path.clone()), path);
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
    fn the_command_drops_only_the_parent_session_vars() {
        let cmd = command(Path::new("claude"));
        let mut removed: Vec<String> = cmd
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.to_string_lossy().into())
            .collect();
        removed.sort();
        let mut want = PARENT_SESSION_VARS.to_vec();
        want.sort();
        assert_eq!(removed, want);
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
    fn a_line_that_is_not_utf8_does_not_stop_the_reader() {
        let data: &[u8] = b"one\n\xff\xfe bad\r\nthree";
        let lines: Vec<String> = lossy_lines(data).collect();
        assert_eq!(lines, ["one", "\u{fffd}\u{fffd} bad", "three"]);
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
        let list = candidates(Some("/h".into()), None, Some("/usr/bin:/bin".into()));
        assert_eq!(list[0], Path::new("/h/.local/bin/claude"));
        let path_at = list.iter().position(|p| p == Path::new("/usr/bin/claude"));
        assert!(path_at.unwrap() > 0);
    }

    #[test]
    fn the_npm_folder_is_searched_before_the_path() {
        let list = candidates(None, Some("/n/npm".into()), Some("/usr/bin".into()));
        let at = |p: &str| list.iter().position(|x| x == Path::new(p)).unwrap();
        assert!(at("/n/npm/claude") < at("/usr/bin/claude"));
    }

    #[test]
    fn a_cmd_or_bat_file_is_a_batch_file() {
        assert!(is_batch(Path::new("C:/npm/claude.cmd")));
        assert!(is_batch(Path::new("x.BAT")));
        assert!(!is_batch(Path::new("claude.exe")));
        assert!(!is_batch(Path::new("claude")));
    }

    #[cfg(unix)]
    #[test]
    fn a_probe_that_hangs_is_killed_and_reported() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("gc-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("claude");
        std::fs::write(&bin, "#!/bin/sh\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let start = Instant::now();
        let status = probe_with(&bin, Duration::from_millis(300));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(status, Status::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(5));
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
