//! The window's root view: sidebar, reading column, and the window chrome.
//! From `DesktopShell` in `app/src/router.tsx`.

use garmin_api::settings::ThemeMode;
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, relative, ColorExt as _, Context,
    ElementId, Entity, IntoElement, Render, Window,
};

use crate::account::{Account, Sync};
use crate::activities::{self, Page, Store};
use crate::ask::{Ask, AskEvent, Source as AskSource};
use crate::body::{self, BodyData};
use crate::chrome;
use crate::data::{Source, Today};
use crate::motion;
use crate::scroll::SmoothScroll;
use crate::sidebar::{self, Info, Sidebar, NAV, SIDEBAR_W};
use crate::theme::{Palette, Theme};
use crate::training::{self, Training};
use crate::ui::{block_in, body, ch, eyebrow_lg, h1, LH_NORMAL, SANS};
use crate::wellness::{self, Wellness};
use crate::{settings, setup, today};

/// The sidebar and the column, as a pair, are never wider than this.
/// On an ultrawide the pair stays together in the middle.
const SHELL_MAX: f32 = 1240.0;
/// The reading column's width, and the air above and beside it.
pub const COLUMN_MAX: f32 = 720.0;
pub const COLUMN_TOP: f32 = 78.0;
pub const COLUMN_SIDE: f32 = 56.0;
const COLUMN_BOTTOM: f32 = 160.0;
/// How tall the fade under the title strip is.
const FADE_H: f32 = 44.0;

/// The entry of the Settings screen: the last one in the nav.
const SETTINGS: usize = NAV.len() - 1;
/// The sidebar entry of the Ask screen.
const ASK: usize = 6;

/// The width of the text column for a window `viewport` pixels wide.
pub fn column_width(viewport: f32) -> f32 {
    (viewport.min(SHELL_MAX) - SIDEBAR_W - 2.0 * COLUMN_SIDE).clamp(200.0, COLUMN_MAX)
}

/// What the Ask screen starts with. Only the command line sets it.
pub struct AskStart {
    pub source: AskSource,
    /// A question that the Ask screen sends at once, for a demo.
    pub question: Option<String>,
    /// Text that waits in the composer, for a demo. `\n` is a new line.
    pub draft: Option<String>,
}

/// Build the Ask screen with its start state.
pub fn new_ask(start: AskStart, palette: Palette, cx: &mut Context<Root>) -> Entity<Ask> {
    let ask = cx.new(|cx| Ask::new(start.source, palette, cx));
    ask.update(cx, |ask, cx| ask.refresh_credits(cx));
    cx.subscribe(&ask, |root, _, event: &AskEvent, cx| root.on_ask(event, cx))
        .detach();
    if let Some(question) = start.question {
        ask.update(cx, |ask, cx| ask.send(question, cx));
    }
    if let Some(draft) = start.draft {
        ask.update(cx, |ask, cx| ask.fill(&draft.replace("\\n", "\n"), cx));
    }
    ask
}

pub struct Root {
    pub(crate) ask: Entity<Ask>,
    pub(crate) today: Today,
    /// Health and Sleep. `None` leaves both entries as placeholders.
    pub(crate) wellness: Option<Wellness>,
    pub(crate) body: BodyData,
    /// Insights, Fitness and Reports. `None` leaves the entries as placeholders.
    pub(crate) training: Option<Training>,
    pub(crate) palette: Palette,
    sidebar: Sidebar,
    /// Counts the screens shown, so each one animates in again.
    pub(crate) visits: usize,
    scroll: SmoothScroll,
    /// The Activities screen reads its view models from this store.
    pub(crate) store: Store,
    /// The list, read once at start and after a sync, not on every frame.
    pub(crate) list: anyhow::Result<garmin_api::ActivitiesView>,
    page: Page,
    /// Where Today reads from. A sync reads it again.
    pub(crate) source: Source,
    pub(crate) account: Account,
}

impl Root {
    pub fn new(
        today: Today,
        palette: Palette,
        store: Store,
        body: BodyData,
        source: Source,
        account: Account,
        ask: Entity<Ask>,
    ) -> Self {
        let list = store.list();
        Self {
            ask,
            list,
            today,
            wellness: None,
            training: None,
            body,
            palette,
            sidebar: Sidebar::new(motion::reduced()),
            visits: 0,
            scroll: SmoothScroll::new(motion::reduced()),
            store,
            page: Page::List,
            source,
            account,
        }
    }

    /// Show the screen with this nav label. For a start flag and the screenshots.
    /// An unknown label keeps Today.
    pub fn open(&mut self, label: &str, cx: &mut Context<Self>) {
        if let Some(index) = NAV.iter().position(|(n, _)| n.eq_ignore_ascii_case(label)) {
            self.select_index(index, cx);
        }
    }

    /// The Ask screen wants premium or the coach settings.
    fn on_ask(&mut self, event: &AskEvent, cx: &mut Context<Self>) {
        match event {
            AskEvent::Premium { signed_in: true } => {
                // Settings may not have opened yet: the cloud state is not live.
                if !self.account.cloud.live {
                    self.cloud_open(cx);
                }
                self.cloud_checkout(cx);
            }
            AskEvent::Premium { signed_in: false } => {
                self.select_index(SETTINGS, cx);
                self.account.cloud.say(
                    "Sign in with your email to get premium. Premium belongs to an account.",
                    false,
                );
            }
            AskEvent::Settings => self.select_index(SETTINGS, cx),
        }
    }

    /// Show one session. The page starts at the top, like a new screen.
    pub fn open_activity(&mut self, id: i64, cx: &mut Context<Self>) {
        self.page = match self.store.detail(id) {
            Ok(Some(view)) => Page::Detail(Box::new(view)),
            _ => Page::Missing,
        };
        self.restart_page(cx);
    }

    /// Open on the Activities entry, with one session shown.
    pub fn start_at_activity(&mut self, id: i64, cx: &mut Context<Self>) {
        self.sidebar.select(1);
        self.open_activity(id, cx);
    }

    pub fn close_activity(&mut self, cx: &mut Context<Self>) {
        self.page = Page::List;
        self.restart_page(cx);
    }

    fn restart_page(&mut self, cx: &mut Context<Self>) {
        self.visits += 1;
        self.scroll = SmoothScroll::new(motion::reduced());
        cx.notify();
    }

    pub fn with_wellness(mut self, wellness: Wellness) -> Self {
        self.wellness = Some(wellness);
        self
    }

    pub fn with_training(mut self, training: Training) -> Self {
        self.training = Some(training);
        self
    }

    /// Show another week of the report. The page keeps its scroll position.
    pub fn show_report(&mut self, offset: usize, cx: &mut Context<Self>) {
        match (
            training::load_report(&self.source, offset),
            &mut self.training,
        ) {
            (Ok(report), Some(training)) => training.report = report,
            (Err(e), _) => eprintln!("could not load the week: {e:#}"),
            _ => {}
        }
        cx.notify();
    }

    fn select(&mut self, index: usize, _: &mut Window, cx: &mut Context<Self>) {
        self.select_index(index, cx);
    }

    fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if index != self.sidebar.active {
            self.sidebar.select(index);
            self.page = Page::List;
            self.visits += 1;
            if index == SETTINGS {
                self.reload_settings();
                self.cloud_open(cx);
            }
            if index == ASK {
                self.ask.update(cx, |ask, cx| ask.visit(cx));
            }
            // A new screen starts at the top.
            self.scroll = SmoothScroll::new(motion::reduced());
            cx.notify();
        }
    }

    /// Light and dark swap. Any palette settles light or dark for itself. The
    /// choice is saved, so it stops following the system.
    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if self.palette.theme().dark {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        self.set_theme_mode(mode, window, cx);
    }

    fn screen(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.sidebar.active == 0 {
            return today::render(&self.today, self.visits, t).into_any_element();
        }
        if self.sidebar.active == 1 {
            return self.activities_screen(t, cx);
        }
        if let Some(w) = &self.wellness {
            match self.sidebar.active {
                2 => return wellness::render_health(&w.health, self.visits, t).into_any_element(),
                3 => return wellness::render_sleep(&w.sleep, self.visits, t).into_any_element(),
                _ => {}
            }
        }
        if self.sidebar.active == SETTINGS {
            return settings::render(self, t, window, cx);
        }
        let label = NAV[self.sidebar.active].0;
        if let Some(screen) = body::screen(label, &self.body, self.visits, t) {
            return screen;
        }
        if let Some(data) = &self.training {
            if let Some(screen) = training::screen(label, data, self.visits, t, cx) {
                return screen;
            }
        }
        div()
            .child(block_in(self.visits, 0, eyebrow_lg("Not built yet", t)))
            .child(block_in(self.visits, 0, h1(label).mt(px(18.0))))
            .child(block_in(
                self.visits,
                1,
                body(
                    "This screen comes in a later phase of the rewrite.",
                    16.0,
                    t.mutd,
                )
                .mt(px(28.0))
                .max_w(ch(62.0, 16.0)),
            ))
            .into_any_element()
    }

    fn activities_screen(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        match &self.page {
            Page::Detail(view) => activities::detail(view, self.visits, t, cx),
            Page::Missing => activities::missing(t, cx),
            Page::List => match &self.list {
                Ok(view) => activities::list(view, self.visits, t, cx),
                Err(error) => activities::failed(error, t),
            },
        }
    }
}

impl Root {
    /// One screen in the whole window, with no sidebar: the window chrome only.
    fn framed(&self, body: gpui::AnyElement, t: &Theme, window: &Window) -> gpui::Div {
        let mut root = div()
            .relative()
            .size_full()
            .flex()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(SANS)
            .line_height(relative(LH_NORMAL))
            .child(body)
            .child(chrome::drag_strip());
        if let Some(controls) = chrome::controls(window, t) {
            root = root.child(controls);
        }
        root.children(chrome::resize_handles(window))
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sidebar.tick(window);

        let t = self.palette.theme();
        if !self.account.connected {
            return self.framed(setup::render(self, &t, window, cx), &t, window);
        }
        let viewport = f32::from(window.viewport_size().width);
        // What lies left of the capped pair carries the nav's ground too.
        let margin = ((viewport - SHELL_MAX) / 2.0).max(0.0);

        let sync_line = match &self.account.sync {
            Sync::Running(line) => Some(line.clone()),
            Sync::Failed(_) => Some("Sync failed. Settings has the reason.".to_string()),
            _ => None,
        };
        let info = Info {
            who: self.today.who.as_deref(),
            synced_ago: self.account.synced_ago.as_deref(),
            sync_line: sync_line.as_deref(),
            sync_failed: matches!(self.account.sync, Sync::Failed(_)),
        };
        let nav = sidebar::render(
            &self.sidebar,
            &info,
            t.dark,
            &t,
            cx,
            sidebar::Actions {
                select: |this, index, window, cx| this.select(index, window, cx),
                toggle_theme: |this, window, cx| this.toggle_theme(window, cx),
                sync_now: |this, _, cx| this.start_sync(false, cx),
            },
        );

        // Each screen gets a new `SmoothScroll` in `select`, so it starts at
        // the top, as the current app does on a route change.
        self.scroll.tick(window);
        let column = if self.sidebar.active == ASK {
            self.ask.clone().into_any_element()
        } else {
            let screen = self.screen(&t, window, cx);
            self.scroll
                .area(ElementId::NamedInteger("page".into(), self.visits as u64))
                .flex_1()
                .min_w_0()
                .h_full()
                .px(px(COLUMN_SIDE))
                .pb(px(COLUMN_BOTTOM))
                .child(div().max_w(px(COLUMN_MAX)).pt(px(COLUMN_TOP)).child(screen))
                .into_any_element()
        };

        // Text under the title strip fades out as it scrolls up.
        let fade = div()
            .absolute()
            .top_0()
            .left(px(margin + SIDEBAR_W))
            .right_0()
            .h(px(FADE_H))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(t.bg, 0.0),
                linear_color_stop(t.bg.opacity(0.0), 1.0),
            ));

        let mut root = div()
            .relative()
            .size_full()
            .flex()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(SANS)
            .line_height(relative(LH_NORMAL))
            .child(
                div()
                    .flex_none()
                    .w(px(margin + SIDEBAR_W))
                    .h_full()
                    .bg(t.bg2)
                    .flex()
                    .justify_end()
                    .child(nav),
            )
            .child(column)
            .child(fade)
            .child(chrome::drag_strip());
        if let Some(controls) = chrome::controls(window, &t) {
            root = root.child(controls);
        }
        root.children(chrome::resize_handles(window))
    }
}
