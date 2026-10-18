//! Sign-in, sync and the theme choice: the state and the jobs behind them.
//!
//! A job is a blocking call on its own thread. It sends messages over a
//! channel, and a poll loop on the UI side turns them into state. The screens
//! (`setup.rs`, `settings.rs`, the sidebar) only read this state.
//!
//! In demo mode (`--persona` or `--db`) nothing talks to Garmin. The sync is a
//! fake with the same steps, and no file in the data folder changes.

use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use garmin_api::session;
use garmin_api::settings::{self, SettingsView, ThemeMode};
use gpui::{App, Context, Window, WindowAppearance};

use crate::field::Field;
use crate::shell::Root;
use crate::theme::Palette;
use crate::{body, data, wellness};

/// How far back a recent sync reads.
const RECENT_DAYS: u32 = 30;
const FULL_DAYS: u32 = 365;
/// The helper that shows the Garmin sign-in page in a real browser view.
const HELPER: &str = "garmin-companion-login";
const HELPER_ENV: &str = "GARMIN_COMPANION_LOGIN";

#[derive(Debug, Clone, PartialEq)]
pub enum Sync {
    Idle,
    /// The line to show: what the sync is reading now.
    Running(String),
    Done(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignIn {
    Idle,
    Waiting(String),
    Failed(String),
}

pub struct Account {
    pub connected: bool,
    /// Made-up data. Nothing reaches Garmin or the data folder.
    pub demo: bool,
    pub sync: Sync,
    pub sign_in: SignIn,
    pub settings: Option<SettingsView>,
    pub synced_ago: Option<String>,
    pub mode: ThemeMode,
    pub token_path: Field,
    pub token_path_default: Option<String>,
}

impl Account {
    pub fn new(demo: bool, cx: &mut App) -> Self {
        let status = if demo {
            None
        } else {
            session::status().map_err(log_status).ok()
        };
        // A keyring that fails is not "not connected". Say so on Setup.
        let sign_in = match (demo, &status) {
            (false, None) => SignIn::Failed(
                "The app could not read its sign-in. Unlock the OS keyring, then start the app again."
                    .into(),
            ),
            _ => SignIn::Idle,
        };
        Self {
            connected: demo || status.as_ref().is_some_and(|s| s.connected),
            demo,
            sync: Sync::Idle,
            sign_in,
            settings: None,
            synced_ago: if demo { None } else { settings::synced_ago() },
            mode: if demo {
                ThemeMode::System
            } else {
                settings::prefs().theme
            },
            token_path: Field::new("Path to garmin_tokens.json", false, cx),
            token_path_default: status.and_then(|s| s.importable_token_path),
        }
    }

    pub fn syncing(&self) -> bool {
        matches!(self.sync, Sync::Running(_))
    }

    fn busy(&self) -> bool {
        self.syncing() || matches!(self.sign_in, SignIn::Waiting(_))
    }
}

fn log_status(error: garmin_api::ApiError) -> garmin_api::ApiError {
    eprintln!("could not read the sign-in state: {error}");
    error
}

enum Msg {
    Progress(String),
    Finished(Result<String, String>),
}

/// The palette for a mode. `System` follows the window.
pub fn palette_for(mode: ThemeMode, appearance: WindowAppearance) -> Palette {
    match (mode, appearance) {
        (ThemeMode::Light, _) => Palette::Paper,
        (ThemeMode::Dark, _) => Palette::Ink,
        (ThemeMode::System, WindowAppearance::Dark | WindowAppearance::VibrantDark) => Palette::Ink,
        (ThemeMode::System, _) => Palette::Paper,
    }
}

/// Run `job` on its own thread. Each message goes to `on` on the UI thread.
fn run_job(
    cx: &mut Context<Root>,
    job: impl FnOnce(mpsc::Sender<Msg>) + Send + 'static,
    on: fn(&mut Root, Msg, &mut Context<Root>),
) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || job(tx));
    cx.spawn(async move |this, cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(120))
            .await;
        let (msgs, ended) = drain(&rx);
        let alive = this.update(cx, |root, cx| {
            for msg in msgs {
                on(root, msg, cx);
            }
            cx.notify();
        });
        if alive.is_err() || ended {
            break;
        }
    })
    .detach();
}

/// Take every waiting message. `ended` is true after the final one, or when
/// the thread died with no final message.
fn drain(rx: &mpsc::Receiver<Msg>) -> (Vec<Msg>, bool) {
    let mut msgs = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(msg) => {
                let last = matches!(msg, Msg::Finished(_));
                msgs.push(msg);
                if last {
                    return (msgs, true);
                }
            }
            Err(mpsc::TryRecvError::Empty) => return (msgs, false),
            Err(mpsc::TryRecvError::Disconnected) => {
                msgs.push(Msg::Finished(Err(
                    "The task stopped without a result.".into()
                )));
                return (msgs, true);
            }
        }
    }
}

impl Root {
    /// Start a sync. A second call while one runs does nothing.
    pub fn start_sync(&mut self, full: bool, cx: &mut Context<Self>) {
        if self.account.busy() || !self.account.connected {
            return;
        }
        self.account.sync = Sync::Running("Starting.".into());
        let demo = self.account.demo;
        let days = if full { FULL_DAYS } else { RECENT_DAYS };
        run_job(
            cx,
            move |tx| {
                let result = if demo {
                    fake_sync(&tx)
                } else {
                    let on = |e: session::SyncEvent| {
                        let _ = tx.send(Msg::Progress(e.line));
                    };
                    session::sync(days, full, &on)
                        .map(|s| s.line)
                        .map_err(|e| e.to_string())
                };
                let _ = tx.send(Msg::Finished(result));
            },
            Self::on_sync,
        );
        cx.notify();
    }

    fn on_sync(&mut self, msg: Msg, cx: &mut Context<Self>) {
        match msg {
            Msg::Progress(line) => self.account.sync = Sync::Running(line),
            Msg::Finished(Ok(line)) => {
                self.account.sync = Sync::Done(line);
                self.reload(cx);
            }
            Msg::Finished(Err(error)) => self.account.sync = Sync::Failed(error),
        }
    }

    /// Open the Garmin sign-in page and wait for the user.
    pub fn start_sign_in(&mut self, cx: &mut Context<Self>) {
        if self.account.busy() || self.account.demo {
            return;
        }
        self.account.sign_in = SignIn::Waiting("Waiting for you to sign in to Garmin.".into());
        run_job(
            cx,
            move |tx| {
                let _ = tx.send(Msg::Finished(sign_in(&tx)));
            },
            Self::on_sign_in,
        );
        cx.notify();
    }

    fn on_sign_in(&mut self, msg: Msg, cx: &mut Context<Self>) {
        match msg {
            Msg::Progress(line) => self.account.sign_in = SignIn::Waiting(line),
            Msg::Finished(Ok(_)) => self.signed_in(cx),
            Msg::Finished(Err(error)) => self.account.sign_in = SignIn::Failed(error),
        }
    }

    /// Adopt the tokens of a `garminconnect` install. An empty field takes the
    /// default path.
    pub fn import_tokens(&mut self, cx: &mut Context<Self>) {
        if self.account.busy() || self.account.demo {
            return;
        }
        let typed = self.account.token_path.line.text().trim().to_string();
        let path = (!typed.is_empty()).then_some(typed);
        match session::import_tokens(path.as_deref()) {
            Ok(()) => self.signed_in(cx),
            Err(e) => self.account.sign_in = SignIn::Failed(e.to_string()),
        }
    }

    fn signed_in(&mut self, cx: &mut Context<Self>) {
        self.account.connected = true;
        self.account.sign_in = SignIn::Idle;
        self.account.token_path.line.clear();
        self.start_sync(true, cx);
    }

    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        if self.account.busy() {
            return;
        }
        if !self.account.demo {
            if let Err(e) = session::sign_out() {
                self.account.sync = Sync::Failed(e.to_string());
                return;
            }
        }
        self.account.connected = false;
        self.account.sync = Sync::Idle;
        self.account.settings = None;
        cx.notify();
    }

    /// Choose light, dark or system. The choice is saved, except in demo mode.
    pub fn set_theme_mode(&mut self, mode: ThemeMode, window: &Window, cx: &mut Context<Self>) {
        self.account.mode = mode;
        self.palette = palette_for(mode, window.appearance());
        let palette = self.palette;
        self.ask.update(cx, |ask, cx| ask.set_palette(palette, cx));
        if !self.account.demo {
            if let Err(e) = settings::set_theme(mode) {
                eprintln!("could not save the theme: {e}");
            }
        }
        self.reload_settings();
        cx.notify();
    }

    /// Read the Today view and the settings again, after a sync.
    fn reload(&mut self, cx: &mut Context<Self>) {
        match data::load_today(&self.source) {
            Ok(today) => self.today = today,
            Err(e) => eprintln!("could not reload Today: {e:#}"),
        }
        match wellness::load(&self.source) {
            Ok(wellness) => self.wellness = Some(wellness),
            Err(e) => eprintln!("could not reload Health and Sleep: {e:#}"),
        }
        match body::load_body(&self.source) {
            Ok(body) => self.body = body,
            Err(e) => eprintln!("could not reload the body screens: {e:#}"),
        }
        self.list = self.store.list();
        if !self.account.demo {
            self.account.synced_ago = settings::synced_ago();
        }
        self.reload_settings();
        cx.notify();
    }

    pub fn reload_settings(&mut self) {
        self.account.settings = Some(if self.account.demo {
            demo_settings(self.account.mode)
        } else {
            settings::settings_view().unwrap_or_else(|e| failed_settings(&e.to_string()))
        });
    }
}

/// The Settings view of a demo run. It reads nothing from the machine.
fn demo_settings(mode: ThemeMode) -> SettingsView {
    SettingsView {
        connected: true,
        account_line: "Demo. The data on screen is made up.".into(),
        cache_line: "Nothing is stored and nothing reaches Garmin.".into(),
        data_folder: None,
        importable_token_path: None,
        theme: mode,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        privacy_line: "In a demo, no request leaves this computer.".into(),
    }
}

fn failed_settings(error: &str) -> SettingsView {
    SettingsView {
        account_line: format!("Settings could not load: {error}"),
        ..demo_settings(ThemeMode::System)
    }
}

/// Four steps with a pause, in place of the real sync.
fn fake_sync(tx: &mpsc::Sender<Msg>) -> Result<String, String> {
    const STEPS: u32 = 4;
    for step in 1..=STEPS {
        std::thread::sleep(Duration::from_millis(900));
        let _ = tx.send(Msg::Progress(format!(
            "Reading your daily health data ({step} of {STEPS})"
        )));
    }
    Ok("Demo sync finished. Nothing was read from Garmin.".into())
}

/// Where the helper binary is. It sits next to the app, unless the
/// environment names another one.
fn helper_path() -> Result<std::path::PathBuf, String> {
    if let Some(path) = std::env::var_os(HELPER_ENV) {
        return Ok(path.into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = format!("{HELPER}{}", std::env::consts::EXE_SUFFIX);
    exe.parent()
        .map(|dir| dir.join(&name))
        .filter(|p| p.is_file())
        .ok_or_else(|| {
            "The sign-in helper is missing. Install the app again, or import a token file."
                .to_string()
        })
}

/// Show the Garmin page in the helper and trade the ticket for tokens.
fn sign_in(tx: &mpsc::Sender<Msg>) -> Result<String, String> {
    let output = Command::new(helper_path()?)
        .arg(session::sign_in_url())
        .output()
        .map_err(|e| format!("The sign-in helper did not start: {e}"))?;
    if !output.status.success() {
        return Err(match output.status.code() {
            Some(1) => "The sign-in window closed before Garmin sent a ticket.".to_string(),
            _ => format!("The sign-in helper failed ({}).", output.status),
        });
    }
    let ticket = ticket_from(&String::from_utf8_lossy(&output.stdout))
        .ok_or("The sign-in window closed before Garmin sent a ticket.")?;
    let _ = tx.send(Msg::Progress("Signing you in.".into()));
    session::redeem_ticket(&ticket).map_err(|e| e.to_string())?;
    Ok("Signed in.".into())
}

/// The ticket on the last non-empty line of the helper output.
fn ticket_from(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_picks_a_palette() {
        let (light, dark) = (WindowAppearance::Light, WindowAppearance::Dark);
        assert_eq!(palette_for(ThemeMode::Light, dark), Palette::Paper);
        assert_eq!(palette_for(ThemeMode::Dark, light), Palette::Ink);
        assert_eq!(palette_for(ThemeMode::System, dark), Palette::Ink);
        assert_eq!(palette_for(ThemeMode::System, light), Palette::Paper);
    }

    #[test]
    fn the_ticket_is_the_last_line_of_the_helper() {
        assert_eq!(ticket_from("ST-1-abc\n"), Some("ST-1-abc".into()));
        assert_eq!(ticket_from("log\nST-2\n\n"), Some("ST-2".into()));
        assert_eq!(ticket_from(" \n"), None);
    }

    #[test]
    fn a_dead_thread_ends_the_job_with_an_error() {
        let (tx, rx) = mpsc::channel::<Msg>();
        tx.send(Msg::Progress("a".into())).unwrap();
        drop(tx);
        let (msgs, ended) = drain(&rx);
        assert!(ended);
        assert!(matches!(msgs.last(), Some(Msg::Finished(Err(_)))));
    }

    #[test]
    fn the_final_message_ends_the_job() {
        let (tx, rx) = mpsc::channel::<Msg>();
        tx.send(Msg::Progress("a".into())).unwrap();
        let (_, ended) = drain(&rx);
        assert!(!ended);
        tx.send(Msg::Finished(Ok("x".into()))).unwrap();
        let (msgs, ended) = drain(&rx);
        assert!(ended && msgs.len() == 1);
    }
}
