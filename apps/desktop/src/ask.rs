//! The Ask screen: a chat with the coach. From `app/src/screens/Ask.tsx`.
//!
//! The thread scrolls and the composer is docked under it. A turn runs on its
//! own thread with a small tokio runtime, because the HTTP client needs one and
//! gpui has its own executor. The turn sends `garmin_coach::Event`s through a
//! channel, and a task on the UI side applies them.
//!
//! Two sources. `Db` runs the real engine against a cache copy. `Demo` is the
//! default: it streams a canned answer and reads no data and calls no model, so
//! the screen works with no setup.
//!
//! The hosted coach shows its credits under the composer, with a link to
//! premium. The model choice lives in Settings (`--coach` sets it for a copy).
//!
//! Not here yet: saved conversations (Recents), pinned questions, and the
//! collapse of the tool steps once an answer lands.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use garmin_coach::{
    AccountView, Event, EventSink, HistoryMessage, HttpTransport, Provider, StreamResult, Transport,
};
use garmin_core::db::Db;
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, relative, ClickEvent, ColorExt as _,
    Context, ElementId, Entity, IntoElement, Render, SharedString, Subscription, Task, Window,
};
use serde_json::Value;

use crate::cloud;
use crate::composer::{Composer, ComposerEvent};
use crate::controls::quiet;
use crate::markdown;
use crate::motion;
use crate::scroll::SmoothScroll;
use crate::shell::{column_width, COLUMN_MAX, COLUMN_SIDE, COLUMN_TOP};
use crate::theme::{Palette, Theme, FS_CAPTION, FS_MD, FS_SMALL};
use crate::ui::{block_in, body, ch, eyebrow_lg, h1, underlined, SANS};

/// How far off the bottom a reader can be and still follow new text.
const STICK_SLOP: f32 = 90.0;
/// Room under the thread for the docked composer.
const COMPOSER_ROOM: f32 = 176.0;

/// Where the answers come from.
#[derive(Clone)]
pub enum Source {
    Demo,
    /// The cache copy that the tools read. The model setting lives in it too.
    Db(PathBuf),
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
}

impl Turn {
    fn new(who: Who, text: String) -> Self {
        Self {
            who,
            text,
            steps: Vec::new(),
            error: None,
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
    palette: Palette,
    composer: Entity<Composer>,
    turns: Vec<Turn>,
    busy: bool,
    stop: Arc<AtomicBool>,
    scroll: SmoothScroll,
    /// Counts the visits, so the empty screen animates in again.
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
            palette,
            composer,
            turns: Vec::new(),
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

    /// Read the provider, and the balance if the hosted coach is the provider.
    pub fn refresh_credits(&mut self, cx: &mut Context<Self>) {
        let path = match &self.source {
            Source::Demo => {
                self.cloud_on = true;
                self.credits = Some(cloud::sample_account());
                return;
            }
            Source::Db(path) => path.clone(),
        };
        let saved = Db::open(&path)
            .and_then(|db| garmin_coach::load_config(&db))
            .map(|(provider, _)| provider);
        self.cloud_on = matches!(saved, Ok(Some(Provider::Cloud)));
        if !self.cloud_on {
            return;
        }
        cx.spawn(async move |this, cx| {
            if let Ok(view) = cloud::on_thread(cloud::account()).await {
                let _ = this.update(cx, |ask, cx| {
                    ask.credits = Some(view);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Send a question: add the two turns and start the engine.
    pub fn send(&mut self, text: String, cx: &mut Context<Self>) {
        if self.busy || text.trim().is_empty() {
            return;
        }
        self.turns.push(Turn::new(Who::You, text));
        let history = history(&self.turns);
        self.turns.push(Turn::new(Who::Coach, String::new()));
        self.busy = true;
        self.stop = Arc::default();
        self.composer.update(cx, |c, cx| c.set_busy(true, cx));

        let (tx, rx) = async_channel::unbounded();
        let (source, stop) = (self.source.clone(), self.stop.clone());
        std::thread::spawn(move || run_on_thread(source, history, tx, stop));
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

/// A call starts and then comes back, as two events with one id.
fn upsert(steps: &mut Vec<Step>, step: Step) {
    match steps.iter_mut().find(|s| s.call_id == step.call_id) {
        Some(existing) => *existing = step,
        None => steps.push(step),
    }
}

/* -------------------------------------------------------------- the engine --- */

struct ChannelSink(async_channel::Sender<Event>);

impl EventSink for ChannelSink {
    fn emit(&self, event: Event) {
        // A closed channel means the window is gone, and nobody needs the event.
        let _ = self.0.send_blocking(event);
    }
}

fn run_on_thread(
    source: Source,
    history: Vec<HistoryMessage>,
    tx: async_channel::Sender<Event>,
    stop: Arc<AtomicBool>,
) {
    let sink = ChannelSink(tx);
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
                let _ = garmin_coach::run_turn(&Demo, &sink, &stop, history, &memory).await;
            }
            Source::Db(path) => real_turn(&path, history, &sink, &stop).await,
        }
    });
}

async fn real_turn(
    path: &std::path::Path,
    history: Vec<HistoryMessage>,
    sink: &ChannelSink,
    stop: &AtomicBool,
) {
    let open = || Db::open(path);
    match open().and_then(|db| HttpTransport::from_config(&db)) {
        Ok(transport) => {
            let _ = garmin_coach::run_turn(&transport, sink, stop, history, &open).await;
        }
        Err(e) => sink.emit(Event::Error {
            text: format!("{e:#}. Set a model with --coach, for example: --coach ollama:llama3.2"),
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
        anyhow::anyhow!("unknown provider {name}: use cloud, openrouter or ollama")
    })?;
    let model = match (model, provider) {
        ("", Provider::Cloud) => "",
        ("", Provider::Openrouter) => garmin_coach::DEFAULT_OPENROUTER_MODEL,
        ("", _) => anyhow::bail!("name a model, for example {name}:llama3.2"),
        (m, _) => m,
    };
    garmin_coach::save_config(&Db::open(db_path)?, provider, model)?;
    Ok(format!("coach set to {name} with {model}"))
}

/* ----------------------------------------------------------------- drawing --- */

fn step_row(step: &Step, t: &Theme) -> impl IntoElement {
    let dot = if step.running {
        t.acc
    } else if step.ok {
        t.faint
    } else {
        t.warn
    };
    div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .font_family(SANS)
        .text_size(px(FS_SMALL))
        .text_color(if step.running { t.mutd } else { t.faint })
        .child(div().flex_none().size(px(5.0)).rounded_full().bg(dot))
        .child(SharedString::from(step.label.clone()))
}

fn user_bubble(text: &str, t: &Theme) -> impl IntoElement {
    div().flex().justify_end().child(
        div()
            .max_w(ch(48.0, FS_MD))
            .px(px(15.0))
            .py(px(11.0))
            .rounded(px(10.0))
            .bg(t.sel)
            .text_size(px(FS_MD))
            .line_height(relative(1.6))
            .child(SharedString::from(text.to_string())),
    )
}

fn coach_turn(turn: &Turn, live: bool, t: &Theme) -> impl IntoElement {
    let mut col = div().flex().flex_col().gap(px(16.0));
    if !turn.steps.is_empty() {
        col = col.child(
            div()
                .flex()
                .flex_col()
                .gap(px(7.0))
                .children(turn.steps.iter().map(|s| step_row(s, t))),
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
        let key = self.visits;
        let day = chrono::Datelike::ordinal(&chrono::Local::now().date_naive()) as usize;
        let chips = (0..3).map(|i| {
            let text = OPENERS[(day + i * 3) % OPENERS.len()];
            underlined(("opener", i), text, FS_MD, t)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.fill(text, cx)))
        });
        let lede = "Ask about your sleep, recovery, activity or weight. The coach reads \
                    your own Garmin data before it answers.";
        div()
            .child(block_in(key, 0, eyebrow_lg("Ask", t)))
            .child(block_in(
                key,
                0,
                h1("What would you like to know?").mt(px(18.0)),
            ))
            .child(block_in(
                key,
                1,
                body(lede, 16.0, t.mutd).mt(px(22.0)).max_w(ch(58.0, 16.0)),
            ))
            .child(block_in(
                key,
                2,
                div()
                    .mt(px(36.0))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(14.0))
                    .children(chips),
            ))
            .when(matches!(self.source, Source::Demo), |d| {
                d.child(block_in(
                    key,
                    3,
                    body(
                        "Sample mode: answers are canned. Run with --db <copy> to ask your own data.",
                        FS_CAPTION,
                        t.faint,
                    )
                    .mt(px(48.0)),
                ))
            })
            .into_any_element()
    }
}

impl Render for Ask {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.palette.theme();
        let width = column_width(f32::from(window.viewport_size().width));
        self.composer.update(cx, |c, _| c.set_column_width(width));

        self.scroll.tick(window);
        // Follow new text while the reader is at the bottom.
        if self.busy && self.scroll.distance_below() <= STICK_SLOP {
            self.scroll.pin_bottom();
        }

        let content = if self.turns.is_empty() {
            self.empty(cx, &t)
        } else {
            let last = self.turns.len() - 1;
            let live = self.busy;
            div()
                .flex()
                .flex_col()
                .gap(px(34.0))
                .children(
                    self.turns
                        .iter()
                        .enumerate()
                        .map(|(i, turn)| match turn.who {
                            Who::You => user_bubble(&turn.text, &t).into_any_element(),
                            Who::Coach => {
                                coach_turn(turn, live && i == last, &t).into_any_element()
                            }
                        }),
                )
                .into_any_element()
        };

        let thread = self
            .scroll
            .area(ElementId::NamedInteger("ask".into(), self.visits as u64))
            .size_full()
            .px(px(COLUMN_SIDE))
            .child(
                div()
                    .max_w(px(COLUMN_MAX))
                    .pt(px(COLUMN_TOP))
                    .pb(px(COMPOSER_ROOM))
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
            .child(dock)
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
    fn the_openers_name_no_sport() {
        for q in OPENERS {
            let lower = q.to_lowercase();
            assert!(!lower.contains("run") && !lower.contains("cadence"), "{q}");
        }
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
