//! What the coach providers that run a local command share: Claude Code,
//! OpenCode and Codex.
//!
//! Each provider has its own module with its flags and its [`Stream`] parser.
//! This module finds the command, probes it, runs it for one turn, and reads its
//! output until the turn ends, the person stops it, or the command dies.

use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use garmin_coach::external;
use garmin_coach::{Event, EventSink, Period};
use garmin_core::db::Db;
use serde_json::Value;

/// How much of the error output of the command the person sees.
const STDERR_LINES: usize = 5;

/// What differs between the commands, as data.
pub struct Cli {
    /// The name that the person reads.
    pub name: &'static str,
    /// The file name of the command, without an extension.
    pub bin: &'static str,
    /// What to do when the command is not logged in.
    pub login_hint: &'static str,
    /// Install folders below the home folder, tried before the PATH.
    pub home_dirs: &'static [&'static str],
    /// Variables that a parent session of the same command sets for its children.
    pub env_remove: &'static [&'static str],
}

/* ----------------------------------------------------------------- stream --- */

/// Turns the lines of the output into events. It holds the state between lines.
pub trait Stream {
    fn feed(&mut self, line: &Value) -> Vec<Event>;
    /// The turn is over: no more lines are needed.
    fn finished(&self) -> bool;
    /// The turn ended before its last line, and the process still runs.
    fn aborted(&self) -> bool {
        false
    }
    /// What went wrong, when the command ends with no final line.
    fn failure(&self) -> Option<String> {
        None
    }
    fn sources(&self) -> Vec<String>;
}

pub fn error(text: &str) -> Event {
    Event::Error { text: text.into() }
}

/// A tool event. The label reads the finished input, when the call has one.
pub fn tool_event(call_id: &str, name: &str, args: &Value, done: bool, ok: bool) -> Event {
    Event::Tool {
        call_id: call_id.into(),
        label: external::tool_label(name, args),
        running: !done,
        ok,
    }
}

/// The error of a turn that ended with no answer text.
pub const NO_ANSWER: &str =
    "The model finished without writing an answer. Ask again, or try a different model.";

/// The text of a command that sends whole messages, not token deltas. Text
/// before a tool call is narration, not the answer.
#[derive(Default)]
pub struct Answer(String);

impl Answer {
    /// A new message. Ask shows it after the text so far.
    pub fn add(&mut self, text: &str) -> Vec<Event> {
        if text.is_empty() {
            return Vec::new();
        }
        let gap = if self.0.is_empty() { "" } else { "\n\n" };
        let text = format!("{gap}{text}");
        self.0.push_str(&text);
        vec![Event::Delta { text }]
    }

    /// A tool call came: the text so far was narration.
    pub fn note(&mut self) -> Vec<Event> {
        let text = std::mem::take(&mut self.0);
        if text.trim().is_empty() {
            return Vec::new();
        }
        vec![Event::Note {
            text: text.trim().into(),
        }]
    }

    /// The end of the turn: `Done`, or an error when there is no answer.
    pub fn done(&self, sources: Vec<String>) -> Vec<Event> {
        if self.0.trim().is_empty() {
            return vec![error(NO_ANSWER)];
        }
        vec![Event::Done { sources }]
    }
}

/* ---------------------------------------------------------------- command --- */

/// Where the command can be. A GUI start has a small PATH, so the usual
/// install folders come first.
fn candidates(
    cli: &Cli,
    home: Option<PathBuf>,
    npm: Option<PathBuf>,
    path: Option<OsString>,
) -> Vec<PathBuf> {
    // On Windows, an npm install makes a `.cmd` file, and Rust finds no file
    // without its extension.
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{}.exe", cli.bin), format!("{}.cmd", cli.bin)]
    } else {
        vec![cli.bin.to_string()]
    };
    let mut list = Vec::new();
    if let Some(home) = home {
        for dir in cli.home_dirs {
            list.extend(names.iter().map(|n| home.join(dir).join(n)));
        }
    }
    if !cfg!(windows) {
        list.push(Path::new("/opt/homebrew/bin").join(cli.bin));
        list.push(Path::new("/usr/local/bin").join(cli.bin));
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

pub fn find(cli: &Cli) -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let home = std::env::var_os(var).map(PathBuf::from);
    let npm = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("npm"));
    candidates(cli, home, npm, std::env::var_os("PATH"))
        .into_iter()
        .find(|p| p.is_file())
}

/// A `.cmd` or `.bat` file runs through `cmd.exe`, which reads the arguments
/// in its own way. Rust refuses a newline in them, and a quote is risky.
pub fn is_batch(bin: &Path) -> bool {
    bin.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
}

/// The command for a run: no window on Windows, and none of the variables that
/// a parent session sets for its children.
fn command(bin: &Path, cli: &Cli) -> Command {
    let mut cmd = Command::new(bin);
    for key in cli.env_remove {
        cmd.env_remove(key);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    // Its own process group, so `end` can kill the whole tree.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd
}

/// The pids of the commands that run now. Each pid is the leader of its tree.
/// A pid leaves the list before the command is reaped, so `kill_all` never
/// hits a reused pid.
static LIVE: Mutex<Vec<u32>> = Mutex::new(Vec::new());

fn live() -> std::sync::MutexGuard<'static, Vec<u32>> {
    LIVE.lock().unwrap_or_else(|e| e.into_inner())
}

fn register(child: &Child) {
    live().push(child.id());
}

fn forget(child: &Child) {
    live().retain(|&p| p != child.id());
}

/// Kill every command that runs now, with its tree. The app calls this on quit.
pub fn kill_all() {
    // The lock stays held, so no command is reaped during the kills.
    for pid in live().drain(..) {
        kill_tree(pid);
    }
}

/// Kill a command and every process that it started: a launcher script, a
/// native binary, an MCP child.
fn kill_tree(pid: u32) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(pid) {
        // The group id is the pid of the command (`process_group(0)`).
        // SAFETY: `kill` takes plain integers and has no memory effect.
        unsafe { libc::kill(-pid, libc::SIGKILL) };
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Kill the command and its tree. Then wait for the command.
fn end(child: &mut Child) {
    forget(child);
    kill_tree(child.id());
    let _ = child.kill();
    let _ = child.wait();
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

/// The arguments of the MCP server that the command starts: this app, as
/// `garmin-companion mcp [--db PATH] [--focus A..B]`.
pub fn mcp_args(db: Option<&Path>, period: Option<Period>) -> Vec<String> {
    let mut args = vec!["mcp".to_string()];
    if let Some(db) = db {
        args.extend(["--db".into(), db.to_string_lossy().into()]);
    }
    if let Some(p) = period {
        args.extend(["--focus".into(), format!("{}..{}", p.start, p.end)]);
    }
    args
}

/// A new folder in the temp folder, with a random name and mode 0700. It is
/// removed when it goes out of scope. Put the files of a turn in it.
pub fn temp_dir(name: &str) -> std::io::Result<tempfile::TempDir> {
    let prefix = format!("garmin-companion-{name}-");
    let mut builder = tempfile::Builder::new();
    builder.prefix(&prefix);
    // The default mode is 0777 minus the umask, so set 0700 on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir()
}

/// What every turn needs before it builds the flags.
pub struct Setup {
    pub bin: PathBuf,
    /// This app, the MCP server.
    pub app: PathBuf,
    pub system: String,
    /// An empty folder that the app owns. The command finds no project files in it.
    pub cwd: PathBuf,
    /// Removes `cwd` when the setup goes out of scope, if it is a temp folder.
    _cwd_guard: Option<tempfile::TempDir>,
}

/// Find the command, and read the system prompt from the cache.
pub fn setup(
    cli: &Cli,
    not_installed: &str,
    db: Option<&Path>,
    period: Option<Period>,
) -> Result<Setup, String> {
    let bin = find(cli).ok_or(not_installed)?;
    let app = app_path().ok_or("Could not find this app's own path.")?;
    let cache = match db {
        Some(path) => Db::open(path),
        None => Db::open_default(),
    }
    .map_err(|e| format!("{e:#}"))?;
    let system = external::mcp_system_prompt(&cache, period).map_err(|e| format!("{e:#}"))?;
    let (cwd, guard) =
        work_dir(cli, db).map_err(|e| format!("Could not make the work folder: {e}"))?;
    Ok(Setup {
        bin,
        app,
        system,
        cwd,
        _cwd_guard: guard,
    })
}

fn work_dir(cli: &Cli, db: Option<&Path>) -> std::io::Result<(PathBuf, Option<tempfile::TempDir>)> {
    if db.is_some() {
        // A demo copy must not write in the data folder of the real cache.
        let dir = temp_dir(&format!("{}-cwd", cli.bin))?;
        return Ok((dir.path().to_path_buf(), Some(dir)));
    }
    let base = garmin_core::paths::ensure_base_dir().map_err(std::io::Error::other)?;
    let dir = base.join(format!("{}-cwd", cli.bin));
    std::fs::create_dir_all(&dir)?;
    Ok((dir, None))
}

/* -------------------------------------------------------------------- turn --- */

/// The `run_turn` function of a provider. `db` is a cache copy, or `None`.
pub type TurnFn = fn(
    Option<&Path>,
    &str,
    Vec<garmin_coach::HistoryMessage>,
    Option<Period>,
    &dyn EventSink,
    &AtomicBool,
);

/// One run of the command.
pub struct Run {
    pub cli: &'static Cli,
    pub bin: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: Vec<(&'static str, OsString)>,
    /// Text for stdin. Without it, stdin is closed.
    pub stdin: Option<String>,
    /// Stop the run after this time. For a command with no turn limit.
    pub limit: Option<Duration>,
}

/// Start the command and read its output until the turn ends. The result goes
/// to the sink as `Done` or `Error`. A stopped turn ends with `Done`.
pub fn run_child(
    run: Run,
    parser: &mut impl Stream,
    sink: &dyn EventSink,
    stop: &AtomicBool,
) -> Result<(), String> {
    let mut cmd = command(&run.bin, run.cli);
    cmd.args(&run.args)
        .envs(run.env.iter().map(|(k, v)| (k, v)))
        .current_dir(&run.cwd)
        .stdin(if run.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", run.cli.name))?;
    register(&child);
    if let (Some(mut stdin), Some(text)) = (child.stdin.take(), run.stdin) {
        std::thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    watch(&mut child, run.cli, run.limit, parser, sink, stop)
}

/// Read the stream until the turn ends, the person stops it, or the command dies.
fn watch(
    child: &mut Child,
    cli: &Cli,
    limit: Option<Duration>,
    parser: &mut impl Stream,
    sink: &dyn EventSink,
    stop: &AtomicBool,
) -> Result<(), String> {
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

    let deadline = limit.map(|l| Instant::now() + l);
    while !parser.finished() {
        if stop.load(Ordering::Relaxed) {
            end(child);
            sink.emit(Event::Done {
                sources: parser.sources(),
            });
            return Ok(());
        }
        if deadline.is_some_and(|d| Instant::now() > d) {
            end(child);
            return Err(format!("{} took too long and was stopped.", cli.name));
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => parser.feed(&line).into_iter().for_each(|e| sink.emit(e)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if parser.aborted() {
        end(child);
        return Ok(());
    }
    let status = reap(child);
    if parser.finished() {
        return Ok(());
    }
    let tail = parser
        .failure()
        .or_else(|| stderr.lock().ok().map(|s| s.clone()))
        .unwrap_or_default();
    let code = status.map_or("none".into(), |c| c.to_string());
    Err(if tail.trim().is_empty() {
        format!("{} stopped without an answer (exit code {code}).", cli.name)
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

/// Wait a moment for a clean exit, then kill. Returns the exit code.
fn reap(child: &mut Child) -> Option<i32> {
    forget(child);
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
    /// `who` is empty when the command does not tell the account.
    Ready {
        version: String,
        who: String,
    },
}

impl Status {
    pub fn line(&self, cli: &Cli) -> String {
        let name = cli.name;
        match self {
            Self::NotInstalled => format!("{name} is not installed."),
            Self::TimedOut => format!("{name} did not answer in time. Try again."),
            Self::LoggedOut => format!("{name}: not logged in. {}", cli.login_hint),
            Self::Ready { version, who } if who.is_empty() => format!("{name} {version}"),
            Self::Ready { version, who } => format!("{name} {version}, logged in as {who}"),
        }
    }

    pub fn ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

/// The version number in the output of `--version`: the first word that starts
/// with a digit. "2.1.304 (Claude Code)" and "codex-cli 0.46.0" both work.
pub fn version_of(text: &str) -> String {
    let words = || text.split_whitespace();
    words()
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()))
        .or_else(|| words().next())
        .unwrap_or_default()
        .into()
}

/// How long one probe call may run before the command is killed.
pub const PROBE_LIMIT: Duration = Duration::from_secs(10);

/// What a probe call gave.
pub struct Output {
    pub text: String,
    /// The exit code was 0.
    pub ok: bool,
}

/// Run `bin` and return what it wrote on stdout. A call that runs longer than
/// `limit` is killed and gives a `TimedOut` error.
fn output_of(bin: &Path, cli: &Cli, args: &[&str], limit: Duration) -> std::io::Result<Output> {
    let mut child = command(bin, cli)
        .args(args.iter().map(OsStr::new))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    register(&child);
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
            forget(&child);
            Ok(Output {
                text: String::from_utf8_lossy(&buf).into_owned(),
                ok: child.wait().is_ok_and(|s| s.success()),
            })
        }
        Err(_) => {
            end(&mut child);
            Err(std::io::ErrorKind::TimedOut.into())
        }
    }
}

/// One probe call. A hang gives `TimedOut`. A command that cannot start gives
/// an empty output.
pub fn read(bin: &Path, cli: &Cli, args: &[&str], limit: Duration) -> Result<Output, Status> {
    match output_of(bin, cli, args, limit) {
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Err(Status::TimedOut),
        out => Ok(out.unwrap_or(Output {
            text: String::new(),
            ok: false,
        })),
    }
}

#[cfg(test)]
pub mod test_support {
    use super::*;

    struct Rec(Mutex<Vec<Event>>);
    impl EventSink for Rec {
        fn emit(&self, e: Event) {
            self.0.lock().unwrap().push(e);
        }
    }

    /// For the ignored tests that need a real command. Ask one question about
    /// the cache copy in `GC_LIVE_DB`, with this app in `APPIMAGE`. Check that a
    /// tool step ran and that the turn ended with `Done`.
    pub fn live_turn(run: super::TurnFn, model: &str) {
        let db = PathBuf::from(std::env::var("GC_LIVE_DB").unwrap());
        let rec = Rec(Mutex::default());
        let history = vec![garmin_coach::HistoryMessage {
            role: "user".into(),
            content: "What was my resting heart rate in the last 3 days?".into(),
        }];
        run(
            Some(&db),
            model,
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
        assert!(tool(false), "{all:?}");
        assert!(matches!(all.last(), Some(Event::Done { .. })), "{all:?}");
    }
}

#[cfg(all(test, unix))]
pub mod script_support {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Hold this in a test that runs a script. A fork in another test thread
    /// can keep the script file open for writing, and the run then fails with
    /// "Text file busy".
    pub fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A shell script named `tool` in its own folder. Remove the folder after use.
    pub fn script(name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gc-cli-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("tool");
        std::fs::write(&bin, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn temp_dir_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("test").unwrap();
        let mode = std::fs::metadata(dir.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    const TEST_CLI: Cli = Cli {
        name: "Test",
        bin: "tool",
        login_hint: "Log in.",
        home_dirs: &[".local/bin"],
        env_remove: &["TOOL_PARENT"],
    };

    #[test]
    fn a_replaced_binary_points_to_the_new_file() {
        let path = PathBuf::from("/opt/gc/garmin-companion");
        assert_eq!(live_path("/opt/gc/garmin-companion (deleted)".into()), path);
        assert_eq!(live_path(path.clone()), path);
    }

    #[test]
    fn the_install_folders_come_before_the_path() {
        let list = candidates(
            &TEST_CLI,
            Some("/h".into()),
            None,
            Some("/usr/bin:/bin".into()),
        );
        assert_eq!(list[0], Path::new("/h/.local/bin/tool"));
        let path_at = list.iter().position(|p| p == Path::new("/usr/bin/tool"));
        assert!(path_at.unwrap() > 0);
    }

    #[test]
    fn the_npm_folder_is_searched_before_the_path() {
        let list = candidates(
            &TEST_CLI,
            None,
            Some("/n/npm".into()),
            Some("/usr/bin".into()),
        );
        let at = |p: &str| list.iter().position(|x| x == Path::new(p)).unwrap();
        assert!(at("/n/npm/tool") < at("/usr/bin/tool"));
    }

    #[test]
    fn a_cmd_or_bat_file_is_a_batch_file() {
        assert!(is_batch(Path::new("C:/npm/claude.cmd")));
        assert!(is_batch(Path::new("x.BAT")));
        assert!(!is_batch(Path::new("claude.exe")));
        assert!(!is_batch(Path::new("claude")));
    }

    #[test]
    fn the_command_drops_only_the_parent_session_vars() {
        let cmd = command(Path::new("tool"), &TEST_CLI);
        let removed: Vec<String> = cmd
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.to_string_lossy().into())
            .collect();
        assert_eq!(removed, ["TOOL_PARENT"]);
    }

    #[test]
    fn a_line_that_is_not_utf8_does_not_stop_the_reader() {
        let data: &[u8] = b"one\n\xff\xfe bad\r\nthree";
        let lines: Vec<String> = lossy_lines(data).collect();
        assert_eq!(lines, ["one", "\u{fffd}\u{fffd} bad", "three"]);
    }

    #[test]
    fn the_version_is_the_first_word_with_a_digit() {
        assert_eq!(version_of("2.1.304 (Claude Code)\n"), "2.1.304");
        assert_eq!(version_of("codex-cli 0.46.0\n"), "0.46.0");
        assert_eq!(version_of("1.18.34"), "1.18.34");
        assert_eq!(version_of(""), "");
    }

    #[test]
    fn the_status_line_names_the_command() {
        let ready = |who: &str| Status::Ready {
            version: "1.0".into(),
            who: who.into(),
        };
        assert_eq!(
            ready("a@b.c").line(&TEST_CLI),
            "Test 1.0, logged in as a@b.c"
        );
        assert_eq!(ready("").line(&TEST_CLI), "Test 1.0");
        assert_eq!(
            Status::LoggedOut.line(&TEST_CLI),
            "Test: not logged in. Log in."
        );
    }

    #[cfg(unix)]
    mod run {
        use super::*;
        use crate::cli_agent::script_support::{script, serial};

        struct Rec(Mutex<Vec<Event>>);
        impl EventSink for Rec {
            fn emit(&self, e: Event) {
                self.0.lock().unwrap().push(e);
            }
        }

        /// Ends after the first line.
        struct OneLine(bool);
        impl Stream for OneLine {
            fn feed(&mut self, _: &Value) -> Vec<Event> {
                self.0 = true;
                Vec::new()
            }
            fn finished(&self) -> bool {
                self.0
            }
            fn sources(&self) -> Vec<String> {
                Vec::new()
            }
        }

        fn run_of(bin: PathBuf, limit: Option<Duration>) -> Run {
            Run {
                cli: &TEST_CLI_STATIC,
                cwd: bin.parent().unwrap().into(),
                bin,
                args: Vec::new(),
                env: Vec::new(),
                stdin: None,
                limit,
            }
        }

        static TEST_CLI_STATIC: Cli = TEST_CLI;

        #[test]
        fn a_probe_that_hangs_is_killed_and_reported() {
            let _lock = serial();
            let bin = script("probe", "exec sleep 30");
            let start = Instant::now();
            let out = read(&bin, &TEST_CLI, &[], Duration::from_millis(300));
            let _ = std::fs::remove_dir_all(bin.parent().unwrap());
            assert_eq!(out.err(), Some(Status::TimedOut));
            assert!(start.elapsed() < Duration::from_secs(5));
        }

        #[test]
        fn a_run_that_passes_its_time_limit_is_killed() {
            let _lock = serial();
            let bin = script("limit", "exec sleep 30");
            let start = Instant::now();
            let rec = Rec(Mutex::default());
            let out = run_child(
                run_of(bin.clone(), Some(Duration::from_millis(300))),
                &mut OneLine(false),
                &rec,
                &AtomicBool::new(false),
            );
            let _ = std::fs::remove_dir_all(bin.parent().unwrap());
            assert_eq!(out, Err("Test took too long and was stopped.".into()));
            assert!(start.elapsed() < Duration::from_secs(5));
        }

        /// Whether a process with this pid still runs (a zombie counts as gone).
        fn alive(pid: i32) -> bool {
            std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
                s.rsplit(')')
                    .next()
                    .is_some_and(|r| !r.trim().starts_with('Z'))
            })
        }

        #[test]
        fn a_kill_reaches_the_processes_that_the_command_started() {
            let _lock = serial();
            let pid_file =
                std::env::temp_dir().join(format!("gc-grandchild-{}", std::process::id()));
            let body = format!("sleep 60 &\necho $! > {}\nwait", pid_file.display());
            let bin = script("tree", &body);
            let rec = Rec(Mutex::default());
            let out = run_child(
                run_of(bin.clone(), Some(Duration::from_millis(800))),
                &mut OneLine(false),
                &rec,
                &AtomicBool::new(false),
            );
            let pid: i32 = std::fs::read_to_string(&pid_file)
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let _ = std::fs::remove_file(&pid_file);
            let _ = std::fs::remove_dir_all(bin.parent().unwrap());
            assert!(out.is_err());
            std::thread::sleep(Duration::from_millis(200));
            assert!(!alive(pid), "the grandchild {pid} still runs");
        }

        #[test]
        fn kill_all_ends_a_running_tree_and_the_list_empties() {
            let _lock = serial();
            let pid_file = std::env::temp_dir().join(format!("gc-killall-{}", std::process::id()));
            let body = format!("sleep 60 &\necho $! > {}\nwait", pid_file.display());
            let bin = script("killall", &body);
            let run = run_of(bin.clone(), None);
            let runner = std::thread::spawn(move || {
                let rec = Rec(Mutex::default());
                run_child(run, &mut OneLine(false), &rec, &AtomicBool::new(false))
            });
            let until = Instant::now() + Duration::from_secs(5);
            while !std::fs::read_to_string(&pid_file).is_ok_and(|s| s.trim().parse::<i32>().is_ok())
                && Instant::now() < until
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            let pid: i32 = std::fs::read_to_string(&pid_file)
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            assert_eq!(live().len(), 1);
            kill_all();
            let out = runner.join().unwrap();
            let _ = std::fs::remove_file(&pid_file);
            let _ = std::fs::remove_dir_all(bin.parent().unwrap());
            assert!(out.is_err());
            assert!(live().is_empty());
            std::thread::sleep(Duration::from_millis(200));
            assert!(!alive(pid), "the grandchild {pid} still runs");
        }

        #[test]
        fn the_text_for_stdin_reaches_the_command() {
            let _lock = serial();
            let bin = script(
                "stdin",
                "read line; echo \"{\\\"got\\\":\\\"$line\\\"}\"; sleep 30",
            );
            let rec = Rec(Mutex::default());
            let mut run = run_of(bin.clone(), None);
            run.stdin = Some("hello\n".into());
            let stop = AtomicBool::new(false);
            struct Seen(Arc<Mutex<Vec<Value>>>, bool);
            impl Stream for Seen {
                fn feed(&mut self, l: &Value) -> Vec<Event> {
                    self.0.lock().unwrap().push(l.clone());
                    self.1 = true;
                    Vec::new()
                }
                fn finished(&self) -> bool {
                    self.1
                }
                fn sources(&self) -> Vec<String> {
                    Vec::new()
                }
            }
            let lines = Arc::new(Mutex::new(Vec::new()));
            let out = run_child(run, &mut Seen(lines.clone(), false), &rec, &stop);
            let _ = std::fs::remove_dir_all(bin.parent().unwrap());
            assert_eq!(out, Ok(()));
            assert_eq!(lines.lock().unwrap()[0]["got"], "hello");
        }
    }
}
