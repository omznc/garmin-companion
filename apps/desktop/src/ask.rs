//! The Ask screen: a chat with the coach. From `app/src/screens/Ask.tsx`.
//!
//! The thread scrolls and the composer is docked under it. A turn runs on its
//! own thread with a small tokio runtime, because the HTTP client needs one and
//! gpui has its own executor. The turn sends `garmin_coach::Event`s through a
//! channel, and a task on the UI side applies them.
//!
//! Three sources. `Db` runs the real engine against the data folder. `Copy` runs it
//! against a database copy in a demo, with the hosted coach off. `Demo` is the
//! default: it streams a canned answer and reads no data and calls no model, so
//! the screen works with no setup.
//!
//! The hosted coach shows its credits under the composer, with a link to
//! premium. The model choice lives in Settings (`--coach` sets it for a copy).
//!
//! Every question and answer goes to the saved threads of `garmin_api::chat`,
//! the same store that the mobile app uses (`threads.rs`). Recents lists the
//! threads, and a thread opens again, with its workout cards in their saved
//! state (`recents.rs`). The open thread comes back at the next start. A demo
//! keeps its threads in memory.
//!
//! Not here yet: pinned questions, and the collapse of the tool steps once an
//! answer lands.

mod recents;
mod threads;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::scale::px;
use anyhow::Result;
use garmin_api::chat;
use garmin_coach::{
    AccountView, Event, EventSink, HistoryMessage, HttpTransport, Provider, StreamResult, Transport,
};
use garmin_core::db::Db;
use garmin_core::workout::{
    EndCondition, ExecStep, Sport, Step as DraftStep, StepKind, Target, WorkoutDraft,
};
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, relative, ClickEvent, ColorExt as _,
    Context, Div, ElementId, Entity, IntoElement, Render, SharedString, Subscription, Task, Window,
};
use serde_json::Value;

use crate::cli_agent::TurnFn;
use crate::cloud;
use crate::composer::{Composer, ComposerEvent};
use crate::controls::quiet;
use crate::field::Field;
use crate::focus::Focus;
use crate::icons::Icon;
use crate::kit::{button, chip, icon_button, Look};
use crate::markdown;
use crate::motion;
use crate::scroll::SmoothScroll;
use crate::sidebar::SIDEBAR_W;
use crate::theme::{Palette, Theme, FS_BASE, FS_CAPTION, FS_MD, FS_SMALL};
use crate::ui::{body, ch, eyebrow, SANS};
use crate::{claude_code, codex, opencode};

/// How far off the bottom a reader can be and still follow new text.
const STICK_SLOP: f32 = 90.0;
/// Room under the thread for the docked composer.
const COMPOSER_ROOM: f32 = 176.0;
/// The reading column of the Ask screen, and the air above and beside it.
const COLUMN_MAX: f32 = 720.0;
const COLUMN_TOP: f32 = 28.0;
const COLUMN_SIDE: f32 = 56.0;
/// The width of the coach panel, with its left border.
pub const PANEL_W: f32 = 340.0;

/// The width of the text column of the Ask screen for a window `viewport` pixels wide.
fn column_width(viewport: f32) -> f32 {
    (viewport - SIDEBAR_W - 2.0 * COLUMN_SIDE).clamp(200.0, COLUMN_MAX)
}

/// What a demo says in place of a call to the hosted coach.
pub const CLOUD_OFF: &str = "The cloud coach is off in a demo. Start the app with --coach ollama:<model> to use another model.";

/// Where the answers come from.
#[derive(Clone)]
pub enum Source {
    Demo,
    /// The cache that the tools read. The model setting lives in it too.
    Db(PathBuf),
    /// A copy of a database in a demo. The tools read it, but the hosted coach is off.
    Copy(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
struct Step {
    call_id: String,
    label: String,
    running: bool,
    ok: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Who {
    You,
    Coach,
}

#[derive(Debug, Clone)]
struct Turn {
    who: Who,
    text: String,
    steps: Vec<Step>,
    error: Option<String>,
    /// The workouts the coach proposed, and whether each went to Garmin.
    drafts: Vec<DraftCard>,
    /// The id of the turn in the saved thread. Only a coach turn has one.
    id: String,
}

#[derive(Debug, Clone)]
struct DraftCard {
    call_id: String,
    draft: WorkoutDraft,
    save: Save,
}

/// What the screen shows: the chat, or the list of saved threads.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Pane {
    Chat,
    Recents,
}

#[derive(Debug, Clone, PartialEq)]
enum Save {
    Idle,
    Saving,
    Saved,
    Failed(String),
}

impl Turn {
    fn new(who: Who, text: String) -> Self {
        Self {
            who,
            text,
            steps: Vec::new(),
            error: None,
            drafts: Vec::new(),
            id: String::new(),
        }
    }
}

/// Opening questions for an empty thread. None of them names a sport.
const OPENERS: [&str; 8] = [
    "How did I sleep this week?",
    "Am I recovered enough for a hard day?",
    "Is my resting heart rate moving?",
    "What does my HRV say about this week?",
    "How active was I compared with last month?",
    "How is my weight trending?",
    "How did my last activity go?",
    "What is worth my attention today?",
];

/// What the Ask screen asks of the window around it.
pub enum AskEvent {
    /// The user wants premium. `signed_in` says whether checkout can start.
    Premium { signed_in: bool },
    /// Show the coach settings.
    Settings,
}

impl gpui::EventEmitter<AskEvent> for Ask {}

pub struct Ask {
    source: Source,
    /// The last known balance. It stays on screen when the network is gone.
    credits: Option<AccountView>,
    /// The hosted coach is the chosen provider, so credits matter.
    cloud_on: bool,
    /// The chosen provider. `None` in a copy, which asks no model.
    provider: Option<Provider>,
    /// The entity sits in the side panel and not in the Ask screen.
    panel: bool,
    /// The width of the side panel.
    panel_w: f32,
    /// The name of the screen behind the panel. The coach hears it with a question.
    context: Option<&'static str>,
    /// The focus period, when it is not the live day.
    focus: Option<Focus>,
    palette: Palette,
    composer: Entity<Composer>,
    turns: Vec<Turn>,
    /// The saved thread that `turns` belongs to. `None` until the first question.
    session: Option<String>,
    /// The title of that thread.
    title: Option<String>,
    pane: Pane,
    /// Frames left to hold the scroll area at the bottom after a thread opens.
    pin: u8,
    /// The list of Recents. `None` while it loads.
    threads: Option<Vec<chat::ThreadMeta>>,
    /// The thread that is being renamed, and its field.
    renaming: Option<String>,
    rename: Field,
    /// The thread that waits for the answer to "Delete?".
    deleting: Option<String>,
    busy: bool,
    stop: Arc<AtomicBool>,
    scroll: SmoothScroll,
    /// Counts the visits, so each visit starts with a fresh scroll area.
    visits: usize,
    task: Option<Task<()>>,
    _subscription: Subscription,
}

impl Ask {
    pub fn new(source: Source, palette: Palette, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| Composer::new(palette, cx));
        let subscription = cx.subscribe(
            &composer,
            |this, _, event: &ComposerEvent, cx| match event {
                ComposerEvent::Send(text) => this.send(text.clone(), cx),
                ComposerEvent::Stop => this.stop(),
            },
        );
        Self {
            source,
            credits: None,
            cloud_on: false,
            provider: None,
            panel: false,
            panel_w: PANEL_W,
            context: None,
            focus: None,
            palette,
            composer,
            turns: Vec::new(),
            session: None,
            title: None,
            pane: Pane::Chat,
            pin: 0,
            threads: None,
            renaming: None,
            rename: {
                let mut field = Field::new("Name of the chat", false, cx);
                field.sans = true;
                field
            },
            deleting: None,
            busy: false,
            stop: Arc::default(),
            scroll: SmoothScroll::new(motion::reduced()),
            visits: 0,
            task: None,
            _subscription: subscription,
        }
    }

    pub fn set_palette(&mut self, palette: Palette, cx: &mut Context<Self>) {
        self.palette = palette;
        self.composer.update(cx, |c, _| c.set_palette(palette));
    }

    /// The screen was opened again.
    pub fn visit(&mut self, cx: &mut Context<Self>) {
        self.visits += 1;
        self.refresh_credits(cx);
    }

    /// Draw the entity in the side panel, or in the Ask screen.
    pub fn set_panel(&mut self, panel: bool) {
        self.panel = panel;
    }

    /// The width of the side panel, which the user can drag.
    pub fn set_panel_width(&mut self, width: f32) {
        self.panel_w = width;
    }

    /// The screen behind the panel, or `None` on the Ask screen.
    pub fn set_context(&mut self, screen: Option<&'static str>) {
        self.context = screen;
    }

    /// The period that the pages look at. A live focus adds nothing to a question.
    pub fn set_focus(&mut self, focus: Focus, cx: &mut Context<Self>) {
        self.focus = (!focus.is_live()).then_some(focus);
        let hint = self.focus.map(|f| f.label());
        self.composer
            .update(cx, |c, cx| c.set_focus_label(hint, cx));
    }

    /// The name of the provider for the status bar.
    pub fn provider_label(&self) -> Option<&'static str> {
        Some(match self.provider? {
            Provider::Cloud => "Cloud",
            Provider::Openrouter => "Own key",
            Provider::Ollama => "Ollama",
            Provider::ClaudeCode => "Claude Code",
            Provider::OpenCode => "OpenCode",
            Provider::Codex => "Codex",
        })
    }

    pub fn provider(&self) -> Option<Provider> {
        self.provider
    }

    /// The credits left, while the hosted coach is the provider.
    pub fn credits_left(&self) -> Option<u64> {
        self.credits
            .as_ref()
            .filter(|_| self.cloud_on)
            .map(|v| v.credits_remaining)
    }

    /// A demo keeps the choice in memory only: set it here, with no read.
    pub fn set_provider(&mut self, provider: Provider) {
        self.provider = Some(provider);
        self.cloud_on = provider == Provider::Cloud;
    }

    /// Read the provider, and the balance if the hosted coach is the provider.
    pub fn refresh_credits(&mut self, cx: &mut Context<Self>) {
        let path = match &self.source {
            Source::Demo => {
                self.cloud_on = true;
                self.provider = Some(Provider::Cloud);
                self.credits = Some(cloud::sample_account());
                return;
            }
            Source::Copy(_) => {
                // A demo makes no cloud call, so there are no credits to show.
                self.cloud_on = false;
                self.provider = None;
                self.credits = None;
                return;
            }
            Source::Db(path) => path.clone(),
        };
        let saved = Db::open(&path)
            .and_then(|db| garmin_coach::load_config(&db))
            .map(|(provider, _)| provider);
        self.provider = saved.as_ref().ok().copied().flatten();
        self.cloud_on = matches!(saved, Ok(Some(Provider::Cloud)));
        if !self.cloud_on {
            return;
        }
        cx.spawn(async move |this, cx| {
            // A quiet read: it must not enrol an install that has asked nothing yet.
            if let Ok(Some(view)) = cloud::on_thread(cloud::account_quiet()).await {
                let _ = this.update(cx, |ask, cx| {
                    ask.credits = Some(view);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Leave the thread on screen. A turn that runs stops, and its engine
    /// thread saves what it wrote so far.
    fn leave(&mut self, cx: &mut Context<Self>) {
        self.stop();
        self.task = None;
        self.busy = false;
        self.composer.update(cx, |c, cx| c.set_busy(false, cx));
    }

    /// Start an empty chat. The old thread stays in Recents.
    pub fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.leave(cx);
        self.turns.clear();
        self.session = None;
        self.title = None;
        self.show(Pane::Chat);
        self.remember(None);
        cx.notify();
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// The cache that holds the open thread id, when the threads are real.
    fn db_path(&self) -> Option<&std::path::Path> {
        match &self.source {
            Source::Db(path) => Some(path),
            Source::Demo | Source::Copy(_) => None,
        }
    }

    /// Keep the open thread for the next start.
    fn remember(&self, session: Option<&str>) {
        if let Some(path) = self.db_path() {
            let (path, session) = (path.to_path_buf(), session.map(str::to_owned));
            std::thread::spawn(move || threads::save_current(&path, session.as_deref()));
        }
    }

    /// Open the thread of the last run. It reads in the background, and a
    /// question that the user sent in the meantime wins.
    pub fn restore(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.db_path().map(std::path::Path::to_path_buf) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let id = cx
                .background_executor()
                .spawn(async move { threads::load_current(&path) })
                .await;
            if let Some(id) = id {
                let _ = this.update(cx, |ask, cx| {
                    if ask.session.is_none() {
                        ask.open_thread(id, cx);
                    }
                });
            }
        })
        .detach();
    }

    /// Show the chat or the list. A new pane starts at its top.
    fn show(&mut self, pane: Pane) {
        self.pane = pane;
        self.renaming = None;
        self.deleting = None;
        self.visits += 1;
    }

    /// Open a saved thread. A thread that is gone opens an empty chat.
    pub fn open_thread(&mut self, id: String, cx: &mut Context<Self>) {
        self.leave(cx);
        let store = threads::store_for(&self.source);
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn({
                    let id = id.clone();
                    async move { chat::get_thread(store, &id, false, None) }
                })
                .await;
            let _ = this.update(cx, |ask, cx| ask.opened(id, read, cx));
        })
        .detach();
    }

    fn opened(
        &mut self,
        id: String,
        read: Result<Option<chat::Thread>, garmin_api::ApiError>,
        cx: &mut Context<Self>,
    ) {
        match read {
            Ok(Some(thread)) => {
                self.session = Some(id.clone());
                self.title = Some(thread.title);
                self.turns = threads::from_saved(thread.turns);
                self.remember(Some(&id));
                self.show(Pane::Chat);
                self.pin = 3;
            }
            Ok(None) => self.new_chat(cx),
            Err(e) => eprintln!("could not open the thread: {e}"),
        }
        cx.notify();
    }

    /// Show the list of saved threads, newest first.
    pub fn show_recents(&mut self, cx: &mut Context<Self>) {
        self.show(Pane::Recents);
        self.reload_threads(cx);
        cx.notify();
    }

    pub fn show_chat(&mut self, cx: &mut Context<Self>) {
        self.show(Pane::Chat);
        cx.notify();
    }

    /// Read the list again. The old list stays on screen until the new one comes.
    fn reload_threads(&mut self, cx: &mut Context<Self>) {
        let store = threads::store_for(&self.source);
        let running = self.busy.then(|| self.session.clone()).flatten();
        cx.spawn(async move |this, cx| {
            let args = chat::ListArgs {
                limit: Some(200),
                offset: None,
            };
            let list = cx
                .background_executor()
                .spawn(async move { chat::list_threads(store, &args, running.as_deref()) })
                .await;
            let _ = this.update(cx, |ask, cx| {
                match list {
                    Ok(list) => ask.threads = Some(list.threads),
                    Err(e) => {
                        eprintln!("could not list the threads: {e}");
                        ask.threads.get_or_insert_with(Vec::new);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Delete a saved thread. The open thread becomes an empty chat.
    pub fn delete_thread(&mut self, id: String, cx: &mut Context<Self>) {
        if self.session.as_deref() == Some(id.as_str()) {
            self.new_chat(cx);
            self.pane = Pane::Recents;
        }
        self.deleting = None;
        let store = threads::store_for(&self.source);
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { chat::delete_thread(store, &id) })
                .await;
            if let Err(e) = done {
                eprintln!("could not delete the thread: {e}");
            }
            let _ = this.update(cx, |ask, cx| ask.reload_threads(cx));
        })
        .detach();
        cx.notify();
    }

    /// Start to rename a thread: the field takes its title.
    pub fn start_rename(
        &mut self,
        id: String,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.deleting = None;
        self.renaming = Some(id);
        self.rename.line.clear();
        self.rename.line.insert(title);
        self.rename.focus.focus(window, cx);
        cx.notify();
    }

    /// Save the new title. An empty title keeps the old one.
    pub fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.renaming.take() else {
            return;
        };
        let title = self.rename.line.text().trim().to_owned();
        if title.is_empty() {
            cx.notify();
            return;
        }
        if self.session.as_deref() == Some(id.as_str()) {
            self.title = Some(title.clone());
        }
        let store = threads::store_for(&self.source);
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { chat::rename_thread(store, &id, &title) })
                .await;
            if let Err(e) = done {
                eprintln!("could not rename the thread: {e}");
            }
            let _ = this.update(cx, |ask, cx| ask.reload_threads(cx));
        })
        .detach();
        cx.notify();
    }

    /// Send a question: add the two turns and start the engine.
    pub fn send(&mut self, text: String, cx: &mut Context<Self>) {
        if self.busy || text.trim().is_empty() {
            return;
        }
        let session = match &self.session {
            Some(id) => id.clone(),
            None => {
                let id = threads::new_session_id();
                self.title = Some(chat::make_title(&text));
                self.session = Some(id.clone());
                self.remember(Some(&id));
                id
            }
        };
        self.pane = Pane::Chat;
        // The store counts the turns: its id is the only id. A failed save gives none.
        let saver = threads::TurnSaver::begin(threads::store_for(&self.source), &session, &text);
        let turn_id = saver
            .as_ref()
            .map(threads::TurnSaver::turn_id)
            .unwrap_or_default();
        self.turns.push(Turn::new(Who::You, text.clone()));
        let history = with_context(history(&self.turns), self.context, self.focus);
        let mut coach = Turn::new(Who::Coach, String::new());
        coach.id = turn_id;
        self.turns.push(coach);
        self.busy = true;
        self.stop = Arc::default();
        self.composer.update(cx, |c, cx| c.set_busy(true, cx));

        let (tx, rx) = async_channel::unbounded();
        let (source, stop) = (self.source.clone(), self.stop.clone());
        let period = self.focus.map(|f| garmin_coach::Period {
            start: f.start,
            end: f.end,
        });
        std::thread::spawn(move || run_on_thread(source, history, tx, stop, period, saver));
        self.task = Some(cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                if this.update(cx, |ask, cx| ask.apply(event, cx)).is_err() {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn apply(&mut self, event: Event, cx: &mut Context<Self>) {
        let Some(turn) = self.turns.last_mut() else {
            return;
        };
        match event {
            Event::Delta { text } => turn.text.push_str(&text),
            // The desktop has no trace of notes: it drops the narration.
            Event::Note { .. } => turn.text.clear(),
            Event::Draft { call_id, draft } => turn.drafts.push(DraftCard {
                call_id,
                draft,
                save: Save::Idle,
            }),
            Event::Tool {
                call_id,
                label,
                running,
                ok,
            } => upsert(
                &mut turn.steps,
                Step {
                    call_id,
                    label,
                    running,
                    ok,
                },
            ),
            Event::Done { .. } => self.finish(cx),
            Event::Error { text } => {
                turn.error = Some(text);
                self.finish(cx);
            }
        }
        cx.notify();
    }

    fn finish(&mut self, cx: &mut Context<Self>) {
        self.refresh_credits(cx);
        self.busy = false;
        self.composer.update(cx, |c, cx| c.set_busy(false, cx));
        self.reload_threads(cx);
    }

    /// Send the draft of one turn to Garmin. A second press while one runs, or
    /// after it saved, does nothing.
    fn save_draft(&mut self, (index, card): (usize, usize), cx: &mut Context<Self>) {
        let Some(turn) = self.turns.get_mut(index) else {
            return;
        };
        let turn_id = turn.id.clone();
        let Some(found) = turn.drafts.get_mut(card) else {
            return;
        };
        if matches!(found.save, Save::Saving | Save::Saved) {
            return;
        }
        found.save = Save::Saving;
        let (draft, call_id) = (found.draft.clone(), found.call_id.clone());
        let session = self.session.clone().unwrap_or_default();
        let store = threads::store_for(&self.source);
        cx.spawn(async move |this, cx| {
            let ids = (session, turn_id, call_id);
            let result = cx
                .background_executor()
                .spawn({
                    let ids = ids.clone();
                    async move {
                        threads::create_and_record(store, &ids, &draft, |d| {
                            garmin_api::workouts::create_workout(d)
                        })
                    }
                })
                .await;
            let _ = this.update(cx, |ask, cx| {
                // The user may have opened another thread while Garmin answered.
                let found = ask
                    .turns
                    .iter_mut()
                    .filter(|t| t.id == ids.1)
                    .flat_map(|t| t.drafts.iter_mut())
                    .find(|d| d.call_id == ids.2);
                if let Some(found) = found {
                    found.save = match result {
                        Ok(_) => Save::Saved,
                        Err(e) => Save::Failed(e.to_string()),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Put text in the composer without sending it.
    pub fn fill(&mut self, text: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, cx| c.fill(text, cx));
    }
}

/// The messages that go back to the model: every turn with text.
fn history(turns: &[Turn]) -> Vec<HistoryMessage> {
    turns
        .iter()
        .filter(|t| !t.text.trim().is_empty())
        .map(|t| HistoryMessage {
            role: match t.who {
                Who::You => "user".into(),
                Who::Coach => "assistant".into(),
            },
            content: t.text.clone(),
        })
        .collect()
}

/// The question gets one more line: the screen that the user is on. The
/// thread on screen does not show the line.
fn with_context(
    mut messages: Vec<HistoryMessage>,
    screen: Option<&str>,
    focus: Option<Focus>,
) -> Vec<HistoryMessage> {
    let Some(last) = messages.last_mut() else {
        return messages;
    };
    if let Some(screen) = screen {
        last.content = format!("{}\n\nThe user is on the {screen} screen.", last.content);
    }
    let period = focus.map(|f| garmin_coach::Period {
        start: f.start,
        end: f.end,
    });
    garmin_coach::with_period_note(messages, period)
}

/// A call starts and then comes back, as two events with one id.
fn upsert(steps: &mut Vec<Step>, step: Step) {
    match steps.iter_mut().find(|s| s.call_id == step.call_id) {
        Some(existing) => *existing = step,
        None => steps.push(step),
    }
}

/* -------------------------------------------------------------- the engine --- */

/// Sends each event to the UI, and saves the turn into its thread. The save
/// comes first and runs here, so an answer reaches the store even when the user
/// has left the thread.
struct ChannelSink {
    tx: async_channel::Sender<Event>,
    saver: Option<threads::TurnSaver>,
    stop: Arc<AtomicBool>,
}

impl EventSink for ChannelSink {
    fn emit(&self, event: Event) {
        if let Some(saver) = &self.saver {
            saver.on_event(&event, self.stop.load(Ordering::Relaxed));
        }
        // A closed channel means the window is gone, and nobody needs the event.
        let _ = self.tx.send_blocking(event);
    }
}

fn run_on_thread(
    source: Source,
    history: Vec<HistoryMessage>,
    tx: async_channel::Sender<Event>,
    stop: Arc<AtomicBool>,
    period: Option<garmin_coach::Period>,
    saver: Option<threads::TurnSaver>,
) {
    let sink = ChannelSink {
        tx,
        saver,
        stop: stop.clone(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        sink.emit(Event::Error {
            text: "Could not start the network runtime.".into(),
        });
        return;
    };
    runtime.block_on(async {
        match source {
            Source::Demo => {
                let memory = || Db::open(std::path::Path::new(":memory:"));
                let _ = garmin_coach::run_turn(&Demo, &sink, &stop, history, &memory, false).await;
            }
            Source::Db(path) => real_turn(&path, false, history, &sink, &stop, period).await,
            Source::Copy(path) => real_turn(&path, true, history, &sink, &stop, period).await,
        }
    });
}

async fn real_turn(
    path: &std::path::Path,
    demo: bool,
    history: Vec<HistoryMessage>,
    sink: &ChannelSink,
    stop: &AtomicBool,
    period: Option<garmin_coach::Period>,
) {
    let open = || Db::open(path);
    // These are programs on this computer, not HTTP transports.
    if let Ok((Some(provider), model)) = open().and_then(|db| garmin_coach::load_config(&db)) {
        let model = model.unwrap_or_default();
        let copy = demo.then_some(path);
        let run = match provider {
            Provider::ClaudeCode => Some(claude_code::run_turn as TurnFn),
            Provider::OpenCode => Some(opencode::run_turn as TurnFn),
            Provider::Codex => Some(codex::run_turn as TurnFn),
            _ => None,
        };
        if let Some(run) = run {
            return run(copy, &model, history, period, sink, stop);
        }
    }
    let transport = open().and_then(|db| {
        if demo && matches!(garmin_coach::load_config(&db)?.0, Some(Provider::Cloud)) {
            anyhow::bail!("{CLOUD_OFF}");
        }
        HttpTransport::from_config(&db)
    });
    match transport {
        Ok(transport) => {
            let _ = garmin_coach::run_turn_in(&transport, sink, stop, history, &open, true, period)
                .await;
        }
        Err(e) => sink.emit(Event::Error {
            text: format!("{e:#}"),
        }),
    }
}

/// A canned answer, streamed word by word. No data is read and no model is called.
struct Demo;

const DEMO_ANSWER: &str = "This is a sample answer, so the screen works with no setup.\n\n\
With a model set up, the coach reads **your own Garmin data** first, then answers. Health comes first:\n\n\
- Sleep, resting heart rate and HRV\n\
- Recovery, stress and body battery\n\
- Activity of any sport, but only the sport your data shows\n\n\
A number that the data cannot support is left out, and the answer says why.";

impl Transport for Demo {
    async fn complete(
        &self,
        _messages: &[Value],
        _tools: Option<&Value>,
        _tool_choice: Option<&str>,
        sink: &dyn EventSink,
        stop: &AtomicBool,
    ) -> Result<StreamResult> {
        let step = |running| Event::Tool {
            call_id: "demo".into(),
            label: "Reading 14 days of recovery signals".into(),
            running,
            ok: true,
        };
        sink.emit(step(true));
        tokio::time::sleep(Duration::from_millis(500)).await;
        sink.emit(step(false));
        for word in DEMO_ANSWER.split_inclusive(' ') {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            sink.emit(Event::Delta { text: word.into() });
            tokio::time::sleep(Duration::from_millis(35)).await;
        }
        Ok(StreamResult {
            content: DEMO_ANSWER.into(),
            tool_calls: Vec::new(),
        })
    }
}

/// Set the model from the command line: `provider` or `provider:model`.
pub fn setup(db_path: &std::path::Path, spec: &str) -> Result<String> {
    let (name, model) = spec.split_once(':').unwrap_or((spec, ""));
    let provider = Provider::parse(name).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown provider {name}: use cloud, openrouter, ollama, claude_code, opencode or codex"
        )
    })?;
    let model = match (model, provider) {
        ("", Provider::Cloud | Provider::OpenCode | Provider::Codex) => "",
        ("", Provider::Openrouter) => garmin_coach::DEFAULT_OPENROUTER_MODEL,
        ("", Provider::ClaudeCode) => garmin_coach::DEFAULT_CLAUDE_MODEL,
        ("", _) => anyhow::bail!("name a model, for example {name}:llama3.2"),
        (m, _) => m,
    };
    garmin_coach::save_config(&Db::open(db_path)?, provider, model)?;
    Ok(format!("coach set to {name} with {model}"))
}

/* ----------------------------------------------------------------- drawing --- */

/// A tool step as a chip. A call that still runs has the accent and a dot.
fn step_chip(step: &Step, t: &Theme) -> Div {
    chip(step.label.clone(), step.running, step.running, t)
}

fn user_bubble(text: &str, t: &Theme) -> impl IntoElement {
    div().flex().justify_end().child(
        div()
            .max_w(relative(0.85))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(7.0))
            .border_1()
            .border_color(t.line)
            .bg(t.bg2)
            .text_size(px(13.5))
            .line_height(relative(1.5))
            .child(SharedString::from(text.to_string())),
    )
}

fn minutes(seconds: f64) -> String {
    let s = seconds.round() as u32;
    match (s / 60, s % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} s"),
    }
}

fn end_text(end: EndCondition) -> String {
    match end {
        EndCondition::Time { seconds } => minutes(seconds),
        EndCondition::Distance { metres } if metres >= 1000.0 => {
            format!("{} km", metres / 1000.0)
        }
        EndCondition::Distance { metres } => format!("{metres} m"),
        EndCondition::LapButton => "until lap".into(),
    }
}

fn exec_line(e: &ExecStep) -> String {
    let kind = match e.kind {
        StepKind::Warmup => "Warm-up",
        StepKind::Interval => "Interval",
        StepKind::Recovery => "Recovery",
        StepKind::Rest => "Rest",
        StepKind::Cooldown => "Cool-down",
    };
    let target = match e.target {
        Target::None => String::new(),
        Target::HrZone { zone } => format!(", zone {zone}"),
        Target::Bpm { low, high } => format!(", {low:.0}-{high:.0} bpm"),
    };
    format!("{kind} {}{target}", end_text(e.end))
}

/// One short line for each step. A repeat reads "N x" and lists its steps.
fn step_lines(steps: &[DraftStep]) -> Vec<String> {
    steps
        .iter()
        .map(|s| match s {
            DraftStep::Exec(e) => exec_line(e),
            DraftStep::Repeat { times, steps } => {
                let inner: Vec<_> = steps.iter().map(exec_line).collect();
                format!("{times} x {}", inner.join(", "))
            }
        })
        .collect()
}

fn sport_name(sport: Sport) -> &'static str {
    match sport {
        Sport::Running => "Running",
        Sport::Cycling => "Cycling",
        Sport::Cardio => "Cardio",
        Sport::StrengthTraining => "Strength",
    }
}

/// The card for a proposed workout, with the button that saves it.
fn draft_card(
    at: (usize, usize),
    draft: &WorkoutDraft,
    save: &Save,
    cx: &mut Context<Ask>,
    t: &Theme,
) -> impl IntoElement {
    let (label, enabled) = match save {
        Save::Idle | Save::Failed(_) => ("Save to Garmin", true),
        Save::Saving => ("Saving", false),
        Save::Saved => ("Saved", false),
    };
    // The quiet disabled colour is the border colour, too faint for "Saved".
    let mut button =
        quiet(("save-draft", at.0 * 100 + at.1), label, enabled, true, t).text_color(t.mutd);
    if enabled {
        button = button
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.save_draft(at, cx)));
    }
    let mut card = div()
        .max_w(ch(62.0, FS_SMALL))
        .p(px(16.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(t.line)
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(eyebrow(sport_name(draft.sport), t))
        .child(body(draft.name.clone(), FS_MD, t.fg))
        .children(
            step_lines(&draft.steps)
                .into_iter()
                .map(|l| body(l, FS_SMALL, t.mutd)),
        )
        .child(div().mt(px(4.0)).child(button));
    if let Save::Failed(e) = save {
        card = card.child(body(e.clone(), FS_SMALL, t.warn));
    }
    card
}

/// The cards of the workouts that one coach turn proposed.
fn draft_cards(
    index: usize,
    turn: &Turn,
    cx: &mut Context<Ask>,
    t: &Theme,
) -> Vec<impl IntoElement> {
    turn.drafts
        .iter()
        .enumerate()
        .map(|(n, card)| draft_card((index, n), &card.draft, &card.save, cx, t))
        .collect()
}

fn coach_turn(turn: &Turn, live: bool, t: &Theme) -> Div {
    let mut col = div().flex().flex_col().gap(px(12.0));
    if !turn.steps.is_empty() {
        col = col.child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .children(turn.steps.iter().map(|s| step_chip(s, t))),
        );
    }
    if !turn.text.is_empty() {
        col = col.child(markdown::render(&turn.text, t));
    } else if live && turn.steps.is_empty() && turn.error.is_none() {
        col = col.child(body("Thinking", FS_SMALL, t.faint));
    }
    if let Some(error) = &turn.error {
        col = col.child(body(error.clone(), FS_SMALL, t.warn).max_w(ch(62.0, FS_SMALL)));
    }
    col
}

/// The line under the composer: the credits, and the way to more of them.
fn credit_row(view: &AccountView, cx: &mut Context<Ask>, t: &Theme) -> impl IntoElement {
    let colour = if view.out {
        t.warn
    } else if view.low {
        t.mutd
    } else {
        t.faint
    };
    let signed_in = view.email.is_some();
    let mut actions = div().flex().items_baseline().gap(px(20.0));
    if !view.premium {
        actions = actions.child(
            quiet("ask-premium", "Get premium", true, false, t).on_click(cx.listener(
                move |_, _: &ClickEvent, _, cx| cx.emit(AskEvent::Premium { signed_in }),
            )),
        );
    }
    if view.out {
        actions = actions.child(
            quiet("ask-own-key", "Use your own key or Ollama", true, false, t)
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(AskEvent::Settings))),
        );
    }
    div()
        .mt(px(9.0))
        .flex()
        .items_baseline()
        .justify_between()
        .gap(px(16.0))
        .child(body(view.credits_line.clone(), FS_CAPTION, colour))
        .child(actions)
}

impl Ask {
    fn empty(&self, cx: &mut Context<Self>, t: &Theme) -> gpui::AnyElement {
        let day = chrono::Datelike::ordinal(&chrono::Local::now().date_naive()) as usize;
        let (line, hover) = (t.line, t.hover);
        let chips = (0..3).map(|i| {
            let text = OPENERS[(day + i * 3) % OPENERS.len()];
            div()
                .id(("opener", i))
                .px(px(10.0))
                .py(px(7.0))
                .rounded(px(5.0))
                .border_1()
                .border_color(line)
                .bg(t.bg2)
                .text_size(px(12.5))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.fill(text, cx)))
                .child(text)
        });
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(8.0))
            .children(chips)
            .when(matches!(self.source, Source::Demo), |d| {
                d.child(body(
                    "Sample mode: answers are canned. Run with --db <copy> to ask your own data.",
                    FS_CAPTION,
                    t.faint,
                ))
            })
            .into_any_element()
    }

    /// The head of the Ask screen: the title, the line about local data and
    /// the chip with the credits.
    fn screen_head(&self, t: &Theme) -> Div {
        let credits = self.credits.as_ref().filter(|_| self.cloud_on).map(|v| {
            chip(
                format!("Cloud · {} left", v.credits_remaining),
                false,
                true,
                t,
            )
        });
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .mb(px(14.0))
            .child(
                div()
                    .text_size(px(20.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Ask the coach"),
            )
            .child(div().text_size(px(13.5)).text_color(t.mutd).child(
                "Questions use your local data. Only the rows that a question \
                         needs leave this computer.",
            ))
            .children(credits.map(|c| div().mt(px(6.0)).flex().child(c)))
    }

    /// The buttons under an answer that has ended. Copy puts the text on the clipboard.
    fn answer_actions(index: usize, text: &str, t: &Theme, cx: &mut Context<Self>) -> Div {
        let text = text.to_string();
        div().flex().gap(px(6.0)).child(
            button(("ask-copy", index), "Copy", Look::Default, true, t).on_click(cx.listener(
                move |_, _: &ClickEvent, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
                },
            )),
        )
    }
}

impl Render for Ask {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.panel {
            self.panel_view(window, cx).into_any_element()
        } else {
            self.screen_view(window, cx).into_any_element()
        }
    }
}

impl Ask {
    /// The Ask screen: a reading column with the composer docked under it.
    fn screen_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let t = self.palette.theme();
        let width = column_width(crate::scale::viewport_w(window));
        self.composer.update(cx, |c, _| {
            c.set_column_width(width);
            c.set_panel(true);
            c.set_hint("Ask about your training...");
        });

        // Follow new text while the reader is at the bottom.
        self.follow(window, cx);

        let recents = self.pane == Pane::Recents;
        let content = if recents {
            self.recents_list(window, cx, &t)
        } else if self.turns.is_empty() {
            self.empty(cx, &t)
        } else {
            let last = self.turns.len() - 1;
            let live = self.busy;
            let mut col = div().flex().flex_col().gap(px(22.0));
            for (i, turn) in self.turns.iter().enumerate() {
                col = col.child(match turn.who {
                    Who::You => user_bubble(&turn.text, &t).into_any_element(),
                    Who::Coach => {
                        let ended = !(live && i == last) && !turn.text.is_empty();
                        coach_turn(turn, live && i == last, &t)
                            .children(draft_cards(i, turn, cx, &t))
                            .children(ended.then(|| Self::answer_actions(i, &turn.text, &t, cx)))
                            .into_any_element()
                    }
                });
            }
            col.into_any_element()
        };

        // The breadcrumb bar of the window shows the thread (`crumb_tail`).
        let bar = div()
            .flex()
            .justify_end()
            .mb(px(14.0))
            .child(self.thread_buttons(cx, &t));
        let thread = self
            .scroll
            .area(ElementId::NamedInteger("ask".into(), self.visits as u64))
            .size_full()
            .px(px(COLUMN_SIDE))
            .child(
                div()
                    .max_w(px(COLUMN_MAX))
                    .mx_auto()
                    .pt(px(COLUMN_TOP))
                    .pb(px(COMPOSER_ROOM))
                    .child(bar)
                    .child(self.screen_head(&t))
                    .child(content),
            );
        let dock = div()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .px(px(COLUMN_SIDE))
            .child(
                div()
                    .max_w(px(COLUMN_MAX))
                    .mx_auto()
                    .pt(px(24.0))
                    .pb(px(18.0))
                    .bg(linear_gradient(
                        180.0,
                        linear_color_stop(t.bg.opacity(0.0), 0.0),
                        linear_color_stop(t.bg, 0.3),
                    ))
                    .child(self.composer.clone())
                    .children(
                        self.credits
                            .as_ref()
                            .filter(|_| self.cloud_on)
                            .map(|v| credit_row(v, cx, &t)),
                    ),
            );
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(thread)
            .when(!recents, |d| d.child(dock))
    }

    /// The header of the panel: the crumbs, the credits and the buttons for
    /// Recents and for a new thread.
    fn panel_header(&self, cx: &mut Context<Self>, t: &Theme) -> Div {
        let credits = self.credits.as_ref().filter(|_| self.cloud_on).map(|v| {
            chip(
                format!("Cloud · {} left", v.credits_remaining),
                false,
                true,
                t,
            )
        });
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(34.0))
            .pl(px(12.0))
            .pr(px(6.0))
            .border_b_1()
            .border_color(t.line)
            .child(self.crumbs(cx, t))
            .children(credits)
            .child(self.thread_buttons(cx, t))
    }

    /// The empty thread of the panel: one line and three questions to start with.
    fn panel_empty(&self, cx: &mut Context<Self>, t: &Theme) -> gpui::AnyElement {
        let day = chrono::Datelike::ordinal(&chrono::Local::now().date_naive()) as usize;
        let (line, hover) = (t.line, t.hover);
        let openers = (0..3).map(|i| {
            let text = OPENERS[(day + i * 3) % OPENERS.len()];
            div()
                .id(("opener", i))
                .px(px(10.0))
                .py(px(7.0))
                .rounded(px(5.0))
                .border_1()
                .border_color(line)
                .bg(t.bg)
                .text_size(px(12.5))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.fill(text, cx)))
                .child(text)
        });
        let demo = matches!(self.source, Source::Demo).then(|| {
            body(
                "Sample mode: answers are canned. Run with --db <copy> to ask your own data.",
                11.5,
                t.faint,
            )
        });
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                body(
                    "Ask about your sleep, recovery, activity or weight. The coach reads \
                     your own Garmin data before it answers.",
                    FS_BASE,
                    t.mutd,
                )
                .line_height(relative(1.6))
                .mb(px(6.0)),
            )
            .children(openers)
            .children(demo)
            .into_any_element()
    }

    /// The thread of the panel: your questions in a box, the answers as text.
    fn panel_thread(&self, cx: &mut Context<Self>, t: &Theme) -> gpui::AnyElement {
        if self.turns.is_empty() {
            return self.panel_empty(cx, t);
        }
        let last = self.turns.len() - 1;
        let mut col = div().flex().flex_col().gap(px(14.0));
        for (i, turn) in self.turns.iter().enumerate() {
            col = col.child(match turn.who {
                Who::You => div()
                    .px(px(11.0))
                    .py(px(9.0))
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(t.line)
                    .bg(t.bg)
                    .child(SharedString::from(turn.text.clone()))
                    .into_any_element(),
                Who::Coach => coach_turn(turn, self.busy && i == last, t)
                    .children(draft_cards(i, turn, cx, t))
                    .into_any_element(),
            });
        }
        col.into_any_element()
    }

    /// The coach panel: the header, the thread and the composer.
    fn panel_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let t = self.palette.theme();
        let panel_w = self.panel_w;
        self.composer.update(cx, |c, _| {
            c.set_column_width(panel_w - 1.0 - 20.0);
            c.set_panel(true);
            c.set_hint("Ask about this screen...");
        });
        self.follow(window, cx);
        let recents = self.pane == Pane::Recents;
        let content = if recents {
            self.recents_list(window, cx, &t)
        } else {
            self.panel_thread(cx, &t)
        };
        let thread = self
            .scroll
            .area(ElementId::NamedInteger("coach".into(), self.visits as u64))
            .flex_1()
            .min_h_0()
            .w_full()
            .child(div().px(px(12.0)).py(px(14.0)).child(content));
        let out = self
            .credits
            .as_ref()
            .filter(|v| self.cloud_on && v.out)
            .map(|v| credit_row(v, cx, &t));
        let dock = div()
            .flex_none()
            .p(px(10.0))
            .border_t_1()
            .border_color(t.line)
            .child(self.composer.clone())
            .children(out);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.panel_header(cx, &t))
            .child(thread)
            .when(!recents, |d| d.child(dock))
    }

    /// Keep the scroll area where the reader is: at the bottom while text
    /// streams, and at the bottom for a few frames after a thread opens,
    /// because its height is known only after the first layout.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll.tick(window);
        if self.pin > 0 {
            self.pin -= 1;
            self.scroll.pin_bottom();
            cx.notify();
        } else if self.busy && self.scroll.distance_below() <= STICK_SLOP {
            self.scroll.pin_bottom();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, running: bool) -> Step {
        Step {
            call_id: id.into(),
            label: "L".into(),
            running,
            ok: true,
        }
    }

    #[test]
    fn a_call_that_returns_replaces_its_running_row() {
        let mut steps = vec![step("a", true)];
        upsert(&mut steps, step("b", true));
        upsert(&mut steps, step("a", false));
        assert_eq!(steps, vec![step("a", false), step("b", true)]);
    }

    #[test]
    fn history_skips_the_empty_coach_turn() {
        let turns = vec![
            Turn::new(Who::You, "hi".into()),
            Turn::new(Who::Coach, "hello".into()),
            Turn::new(Who::You, "more".into()),
            Turn::new(Who::Coach, String::new()),
        ];
        let roles: Vec<_> = history(&turns).into_iter().map(|m| m.role).collect();
        assert_eq!(roles, ["user", "assistant", "user"]);
    }

    #[test]
    fn the_question_carries_the_screen_but_the_thread_does_not() {
        let turns = vec![Turn::new(Who::You, "hi".into())];
        let sent = with_context(history(&turns), Some("Sleep"), None);
        assert_eq!(sent[0].content, "hi\n\nThe user is on the Sleep screen.");
        assert_eq!(turns[0].text, "hi");
        assert_eq!(with_context(history(&turns), None, None)[0].content, "hi");
    }

    #[test]
    fn a_focus_period_adds_its_dates_to_the_question() {
        let turns = vec![Turn::new(Who::You, "hi".into())];
        let day = |d| chrono::NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
        let focus = Focus::range(day(14), day(20));
        let sent = with_context(history(&turns), None, Some(focus));
        assert!(sent[0]
            .content
            .contains("Focus period: 2026-10-14 to 2026-10-20 (ISO dates)."));
    }

    #[test]
    fn a_repeat_reads_as_n_times() {
        let work = ExecStep {
            kind: StepKind::Interval,
            end: EndCondition::Time { seconds: 180.0 },
            target: Target::HrZone { zone: 4 },
            note: None,
        };
        let rest = ExecStep {
            kind: StepKind::Recovery,
            end: EndCondition::Distance { metres: 400.0 },
            target: Target::None,
            note: None,
        };
        let lines = step_lines(&[DraftStep::Repeat {
            times: 4,
            steps: vec![work, rest],
        }]);
        assert_eq!(lines, ["4 x Interval 3 min, zone 4, Recovery 400 m"]);
    }

    #[test]
    fn the_openers_name_no_sport() {
        for q in OPENERS {
            let lower = q.to_lowercase();
            assert!(!lower.contains("run") && !lower.contains("cadence"), "{q}");
        }
    }

    #[test]
    fn a_copy_refuses_the_cloud_provider() {
        let path = std::env::temp_dir().join(format!("gc-ask-copy-{}.db", std::process::id()));
        setup(&path, "cloud").unwrap();
        let (tx, rx) = async_channel::unbounded();
        run_on_thread(Source::Copy(path), vec![], tx, Arc::default(), None, None);
        let events: Vec<Event> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(matches!(&events[..], [Event::Error { text }] if text == CLOUD_OFF));
    }

    #[test]
    fn setup_checks_the_provider_and_names_a_model() {
        let dir = std::env::temp_dir().join(format!("gc-ask-{}.db", std::process::id()));
        assert!(setup(&dir, "nope").is_err());
        assert!(setup(&dir, "cloud").unwrap().contains("cloud"));
        assert!(setup(&dir, "ollama").is_err());
        assert!(setup(&dir, "ollama:llama3.2").unwrap().contains("llama3.2"));
        let db = Db::open(&dir).unwrap();
        let (provider, model) = garmin_coach::load_config(&db).unwrap();
        assert_eq!(provider, Some(Provider::Ollama));
        assert_eq!(model.as_deref(), Some("llama3.2"));
        drop(db);
        let _ = std::fs::remove_file(&dir);
    }
}
