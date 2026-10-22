//! The window's root view: the title bar, the sidebar, the tabs, the page, the
//! coach panel and the status bar. From `DesktopShell` in `app/src/router.tsx`
//! and the shell of the design.

use crate::scale::px;
use garmin_api::settings::ThemeMode;
use gpui::{
    div, prelude::*, relative, AnyElement, Context, ElementId, Entity, IntoElement, Render, Window,
};

use crate::account::Account;
use crate::activities::{self, Page, Store};
use crate::ask::{Ask, AskEvent, Source as AskSource};
use crate::body::BodyData;
use crate::chrome;
use crate::data::{Source, Today};
use crate::focus::Focus;
use crate::motion;
use crate::panes::{self, clamp_width, Side};
use crate::places::Places;
use crate::screen::{Group, Screen};
use crate::scroll::SmoothScroll;
use crate::sidebar::{self, Sidebar};
use crate::theme::{Palette, Theme};
use crate::training::Training;
use crate::ui::{body, ch, eyebrow_lg, h1, LH_NORMAL, SANS};
use crate::views::Views;
use crate::wellness::Wellness;
use crate::{blocks, settings, setup, statusbar, tabs, titlebar, window};

/// Below this grid width each block takes the whole row.
pub(crate) const NARROW_GRID: f32 = 640.0;
/// The widest the page gets.
pub(crate) const PAGE_MAX: f32 = 1120.0;

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
    ask.update(cx, |ask, cx| {
        ask.refresh_credits(cx);
        ask.restore(cx);
    });
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
    /// Plan, Routes and Gear.
    pub(crate) places: Places,
    /// Insights, Fitness and Reports. `None` leaves the entries as placeholders.
    pub(crate) training: Option<Training>,
    /// The charts, tiles and extras views. Empty until `start` has read them.
    pub(crate) views: Views,
    /// The day or the range that every page looks at.
    pub(crate) focus: Focus,
    /// True while the focus is the live "today". Then it follows the midnight rollover.
    follows_today: bool,
    /// The month on show and the drag of the calendar.
    pub(crate) calendar: crate::calendar::Calendar,
    /// The open menu of the status bar.
    pub(crate) status_menu: crate::statusmenu::StatusMenu,
    /// The widths of the sidebar and the coach panel, and a drag of a handle.
    pub(crate) sidebar_w: f32,
    pub(crate) coach_w: f32,
    pub(crate) pane_drag: Option<crate::panes::Drag>,
    /// What the last frame showed, for the hysteresis of the layout.
    pub(crate) shown: std::cell::Cell<panes::Shown>,
    pub(crate) palette: Palette,
    sidebar: Sidebar,
    /// The screen on show.
    pub(crate) active: Screen,
    /// The open tabs, oldest first. Today is first.
    tabs: Vec<Screen>,
    /// The user wants the coach panel. A narrow window hides it all the same.
    pub(crate) coach_open: bool,
    /// The number of cached activities, for the sidebar and the status bar.
    activity_count: Option<i64>,
    /// Counts the screens shown, so each one starts with a fresh scroll area.
    pub(crate) visits: usize,
    pub(crate) scroll: SmoothScroll,
    /// The Activities screen reads its view models from this store.
    pub(crate) store: Store,
    /// The list, read once at start and after a sync, not on every frame.
    pub(crate) list: anyhow::Result<garmin_api::ActivitiesView>,
    page: Page,
    /// Where Today reads from. A sync reads it again.
    pub(crate) source: Source,
    pub(crate) account: Account,
    /// The background reads in flight, one for each key. A new read drops the old one.
    pub(crate) loads: std::collections::HashMap<&'static str, gpui::Task<()>>,
    /// The command palette, while it is open.
    pub(crate) command_palette: Option<Entity<crate::command_palette::CommandPalette>>,
    /// The saved window size and position. A demo never writes it.
    window_state: window::State,
    /// The blocks of Today, in order. The user arranges them.
    pub(crate) today_layout: blocks::layout::Layout,
    /// The edit mode of Today. `start` makes it.
    pub(crate) edit: Option<blocks::edit::Edit>,
}

/// How long the window rests before its new size is saved.
const SAVE_AFTER: std::time::Duration = std::time::Duration::from_millis(400);

impl Root {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        today: Today,
        palette: Palette,
        store: Store,
        body: BodyData,
        source: Source,
        account: Account,
        ask: Entity<Ask>,
        window_state: window::State,
    ) -> Self {
        let list = store.list();
        let places = Places::load(&source);
        // A copy of the cache can have its own day. Then the focus starts there.
        let focus = match &source {
            Source::Db {
                date: Some(day), ..
            } => Focus::day(*day),
            _ => Focus::today(),
        };
        // A demo shows the shipped order and leaves the saved one alone.
        let prefs = if account.demo {
            Default::default()
        } else {
            garmin_api::settings::prefs()
        };
        let sidebar = Sidebar::new(
            motion::reduced(),
            &prefs.nav_order,
            &prefs.default_screen,
            &window_state.collapsed,
        );
        let mut tabs = vec![Screen::Today];
        tabs::open(&mut tabs, sidebar.active);
        Self {
            ask,
            list,
            today,
            wellness: None,
            training: None,
            body,
            places,
            views: Views::default(),
            calendar: crate::calendar::Calendar::new(focus, window_state.calendar_closed),
            status_menu: Default::default(),
            focus,
            follows_today: focus.is_live(),
            sidebar_w: clamp_width(Side::Sidebar, window_state.sidebar_w),
            coach_w: clamp_width(Side::Coach, window_state.coach_w),
            pane_drag: None,
            shown: std::cell::Cell::new(panes::Shown::ALL),
            palette,
            active: sidebar.active,
            sidebar,
            tabs,
            coach_open: window_state.coach_open,
            activity_count: store.count(),
            visits: 0,
            scroll: SmoothScroll::new(motion::reduced()),
            store,
            page: Page::List,
            source,
            account,
            loads: Default::default(),
            command_palette: None,
            today_layout: blocks::layout::Layout::load(
                window_state.today_layout.as_deref(),
                &window_state.today_seen,
            ),
            edit: None,
            window_state,
        }
    }

    /// Save the window size and position when they change, and when it closes.
    pub fn watch_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.account.demo {
            return;
        }
        cx.observe_window_bounds(window, |root, window, cx| {
            root.remember_window(window);
            root.loads.insert(
                "window",
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(SAVE_AFTER).await;
                    let _ = this.update(cx, |root, _| root.window_state.save());
                }),
            );
        })
        .detach();
        let root = cx.entity();
        window.on_window_should_close(cx, move |window, cx| {
            root.update(cx, |root, _| {
                root.remember_window(window);
                root.window_state.save();
            });
            true
        });
    }

    /// Keep the bounds of a window that is neither zoomed nor full screen.
    fn remember_window(&mut self, window: &Window) {
        if let gpui::WindowBounds::Windowed(bounds) = window.window_bounds() {
            self.window_state.remember(bounds);
        }
    }

    /// Show the screen with this nav label. For a start flag and the screenshots.
    /// An unknown label keeps the default screen.
    pub fn open(&mut self, label: &str, cx: &mut Context<Self>) {
        if let Some(screen) = Screen::from_label(label) {
            self.select_screen(screen, cx);
        }
    }

    /// The Ask screen wants premium or the coach settings.
    fn on_ask(&mut self, event: &AskEvent, cx: &mut Context<Self>) {
        match event {
            // A demo takes this path too: the checkout says that it is off.
            AskEvent::Premium { signed_in } if *signed_in || self.account.demo => {
                // Settings may not have opened yet: the cloud state is not live.
                if !self.account.cloud.live {
                    self.cloud_open(cx);
                }
                self.cloud_checkout(true, cx);
            }
            AskEvent::Premium { .. } => {
                self.show_settings(cx);
                self.account.cloud.say(
                    "Sign in with your email to get premium. Premium belongs to an account.",
                    false,
                );
            }
            AskEvent::Settings => self.show_settings(cx),
        }
    }

    /// Show the Settings screen, where the coach notes are drawn.
    pub(crate) fn show_settings(&mut self, cx: &mut Context<Self>) {
        self.select_screen(Screen::Settings, cx);
    }

    /// Show one session. The page starts at the top, like a new screen.
    pub fn open_activity(&mut self, id: i64, cx: &mut Context<Self>) {
        // A row on another screen, such as an insight, opens the session too.
        if self.active != Screen::Activities {
            self.select_screen(Screen::Activities, cx);
        }
        let store = self.store.clone();
        crate::account::load_async(
            self,
            "detail",
            cx,
            move || store.detail(id),
            |root, detail, cx| {
                root.page = match detail {
                    Ok(Some(view)) => Page::Detail(Box::new(view)),
                    _ => Page::Missing,
                };
                root.restart_page(cx);
            },
        );
    }

    /// Show the detail page of one Insights item.
    pub fn open_insight(&mut self, id: String, cx: &mut Context<Self>) {
        self.select_screen(Screen::Insights, cx);
        self.page = Page::Insight(id);
        self.restart_page(cx);
    }

    /// Open the coach panel and send it a question.
    pub fn ask_coach(&mut self, question: String, cx: &mut Context<Self>) {
        if !self.coach_open {
            self.toggle_coach(cx);
        }
        self.ask.update(cx, |ask, cx| ask.send(question, cx));
    }

    /// Open on the Activities entry, with one session shown.
    pub fn start_at_activity(&mut self, id: i64, cx: &mut Context<Self>) {
        self.select_screen(Screen::Activities, cx);
        self.open_activity(id, cx);
    }

    /// Show the table of every session, from page `at`.
    pub fn show_all_activities(&mut self, at: usize, cx: &mut Context<Self>) {
        self.page = Page::All(at);
        self.restart_page(cx);
    }

    pub fn close_activity(&mut self, cx: &mut Context<Self>) {
        self.loads.remove("detail");
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

    fn select(&mut self, screen: Screen, _: &mut Window, cx: &mut Context<Self>) {
        self.select_screen(screen, cx);
    }

    /// Show a screen: open its tab, or find it, and light its entry.
    pub(crate) fn select_screen(&mut self, screen: Screen, cx: &mut Context<Self>) {
        let was_empty = self.tabs.is_empty();
        tabs::open(&mut self.tabs, screen);
        if screen != self.active || was_empty {
            self.stop_edit(cx);
            self.active = screen;
            self.sidebar.select(screen);
            self.enter(screen, cx);
        }
        cx.notify();
    }

    /// Follow the link in the header of a block.
    pub(crate) fn go(&mut self, to: crate::blocks::Go, cx: &mut Context<Self>) {
        match to {
            crate::blocks::Go::Screen(screen) => self.select_screen(screen, cx),
            crate::blocks::Go::Activity(id) => {
                self.select_screen(Screen::Activities, cx);
                self.open_activity(id, cx);
            }
            crate::blocks::Go::AllActivities => {
                self.select_screen(Screen::Activities, cx);
                self.show_all_activities(0, cx);
            }
        }
    }

    /// Close a tab. If it was in view, the tab on its left comes into view.
    /// When the last tab closes, the page shows an empty area.
    pub(crate) fn close_tab(&mut self, screen: Screen, cx: &mut Context<Self>) {
        if let Some(next) = tabs::close(&mut self.tabs, screen, self.active) {
            self.select_screen(next, cx);
        }
        cx.notify();
    }

    pub(crate) fn tabs(&self) -> &[Screen] {
        &self.tabs
    }

    /// Show the coach panel, or hide it. The choice is saved.
    pub(crate) fn toggle_coach(&mut self, cx: &mut Context<Self>) {
        self.coach_open = !self.coach_open;
        self.save_window_state();
        cx.notify();
    }

    /// Close an open sidebar group, or open a closed one. The choice is saved.
    fn toggle_group(&mut self, group: Group, cx: &mut Context<Self>) {
        self.sidebar.toggle(group);
        self.save_window_state();
        cx.notify();
    }

    /// Set the interface scale at once and save it.
    pub(crate) fn set_scale(&mut self, factor: f32, cx: &mut Context<Self>) {
        crate::scale::set(factor);
        self.save_window_state();
        cx.notify();
    }

    /// Go one step up or down the scale. `None` resets it to 100 percent.
    pub(crate) fn zoom(&mut self, up: Option<bool>, cx: &mut Context<Self>) {
        let next = match up {
            Some(up) => crate::scale::step(up),
            None => Some(1.0),
        };
        if let Some(factor) = next {
            self.set_scale(factor, cx);
        }
    }

    pub(crate) fn save_window_state(&mut self) {
        if self.account.demo {
            return;
        }
        self.window_state.scale = crate::scale::get();
        self.window_state.calendar_closed = self.calendar.collapsed;
        self.window_state.sidebar_w = Some(self.sidebar_w);
        self.window_state.coach_w = Some(self.coach_w);
        self.window_state.coach_open = self.coach_open;
        self.window_state.collapsed = self.sidebar.closed_keys();
        // The shipped layout is not saved, so a later release can change it.
        self.window_state.today_layout = (self.today_layout != blocks::layout::Layout::standard())
            .then(|| self.today_layout.to_saved());
        self.window_state.today_seen = blocks::layout::Layout::standard().ids();
        self.window_state.save();
    }

    /// The count at the right of an entry. `None` when it is unknown or zero.
    fn nav_count(&self, screen: Screen) -> Option<i64> {
        let count = match screen {
            Screen::Activities => self.activity_count,
            Screen::Routes => self.places.route_count(),
            Screen::Gear => self.places.gear_count(),
            Screen::Insights => self.training.as_ref().map(|t| {
                t.insights
                    .sections
                    .iter()
                    .map(|s| s.items.len())
                    .sum::<usize>() as i64
            }),
            _ => None,
        };
        count.filter(|n| *n > 0)
    }

    pub(crate) fn activity_count(&self) -> Option<i64> {
        self.activity_count.filter(|n| *n > 0)
    }

    pub(crate) fn set_activity_count(&mut self, count: Option<i64>) {
        self.activity_count = count;
    }

    /// The page under the screen name in the breadcrumb bar.
    /// Whether the breadcrumb page is a button for the focus picker. The
    /// detail page of a session shows its own name and date as plain text.
    pub(crate) fn crumb_picks(&self) -> bool {
        self.active.follows_focus() && matches!(self.page, Page::List)
    }

    pub(crate) fn crumb(&self, t: &Theme) -> String {
        match (&self.page, self.active) {
            (Page::Detail(view), Screen::Activities) => activities::crumb(view),
            (Page::All(_), Screen::Activities) => "All sessions".into(),
            (Page::Insight(id), Screen::Insights) => {
                crate::insight::crumb(self.training.as_ref(), id)
            }
            _ => blocks::crumb(self.active, &self.ctx(t))
                .unwrap_or_else(|| self.active.crumb(self.focus.end)),
        }
    }

    /// Load what the screen of the lit entry needs. Call it once at start, for
    /// the default screen, which no click opened.
    pub fn start(&mut self, cx: &mut Context<Self>) {
        self.tell_ask_focus(cx);
        self.edit = Some(blocks::edit::Edit::new(cx));
        cx.set_global(blocks::RootHandle(cx.entity().downgrade()));
        self.enter(self.active, cx);
        let (path, day) = self.places.cache();
        crate::account::load_async(
            self,
            "views",
            cx,
            move || Views::load(&path, day),
            |root, views, cx| {
                root.views = views;
                cx.notify();
            },
        );
    }

    /// The source to read for the day at the end of the focus.
    pub(crate) fn focused_source(&self) -> Source {
        self.source.on_day(self.focus.end)
    }

    /// Look at another day or range. Every view that takes a day reads again
    /// with `focus.end`. The pages keep the old data until the new data comes.
    pub fn set_focus(&mut self, focus: Focus, cx: &mut Context<Self>) {
        self.follows_today = focus.is_live();
        if focus == self.focus {
            return;
        }
        self.focus = focus;
        self.calendar.month = crate::calendar::first_of_month(focus.end);
        self.tell_ask_focus(cx);
        self.reload(cx);
        cx.notify();
    }

    /// True when the focus is "today" and the real day has moved on.
    pub(crate) fn focus_is_stale(&self) -> bool {
        self.focus
            != self
                .focus
                .rolled(self.follows_today, crate::focus::real_today())
    }

    /// Move a "today" focus to the real today. `reload` calls this first.
    pub(crate) fn roll_focus(&mut self, cx: &mut Context<Self>) {
        if !self.focus_is_stale() {
            return;
        }
        self.focus = Focus::today();
        self.calendar.month = crate::calendar::first_of_month(self.focus.end);
        self.tell_ask_focus(cx);
    }

    /// The coach hears the focus with each question.
    fn tell_ask_focus(&mut self, cx: &mut Context<Self>) {
        let focus = self.focus;
        self.ask.update(cx, |ask, cx| ask.set_focus(focus, cx));
    }

    /// The first crumb: go back to the main page of the screen. On
    /// Activities this leaves the detail of a session.
    pub(crate) fn screen_home(&mut self, screen: Screen, cx: &mut Context<Self>) {
        if screen == Screen::Activities {
            self.close_activity(cx);
        } else {
            if screen == Screen::Ask {
                self.ask.update(cx, |ask, cx| ask.show_chat(cx));
            }
            self.page = Page::List;
            self.restart_page(cx);
        }
    }

    fn enter(&mut self, screen: Screen, cx: &mut Context<Self>) {
        // The coach hears which screen the question comes from. On Ask it is the screen.
        let context = (screen != Screen::Ask).then(|| screen.label());
        self.ask.update(cx, |ask, _| ask.set_context(context));
        self.loads.remove("detail");
        self.page = Page::List;
        self.visits += 1;
        if screen == Screen::Settings {
            self.reload_settings(cx);
            self.cloud_open(cx);
        }
        if screen == Screen::Gear {
            self.load_gear(cx);
        }
        if screen == Screen::Ask {
            self.ask.update(cx, |ask, cx| ask.visit(cx));
        }
        // A new screen starts at the top.
        self.scroll = SmoothScroll::new(motion::reduced());
        cx.notify();
    }

    /// Save the sidebar order, except in demo mode.
    fn save_nav_order(&self) {
        if self.account.demo {
            return;
        }
        let (keys, default) = (self.sidebar.keys(), self.sidebar.default_key());
        if let Err(e) = garmin_api::settings::set_nav_order(keys, default) {
            eprintln!("could not save the sidebar order: {e}");
        }
    }

    /// Light and dark swap. Any palette settles light or dark for itself. The
    /// choice is saved, so it stops following the system.
    pub(crate) fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if self.palette.theme().dark {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        self.set_theme_mode(mode, window, cx);
    }

    fn screen(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.tabs.is_empty() {
            return div()
                .text_size(px(13.5))
                .text_color(t.mutd)
                .child("Pick a screen in the sidebar.")
                .into_any_element();
        }
        // A session and the table of all sessions are not in the block layout.
        if self.active == Screen::Activities && !matches!(self.page, Page::List) {
            return self.activity_page(t, window, cx);
        }
        if let (Screen::Insights, Page::Insight(id)) = (self.active, &self.page) {
            let (available, id) = (self.grid_width(window), id.clone());
            return crate::insight::page(self, &id, available, available < NARROW_GRID, t, cx);
        }
        if let Some(page) = self.block_page(t, window, cx) {
            return page;
        }
        if self.active == Screen::Settings {
            return settings::render(self, t, window, cx);
        }
        let label = self.active.label();
        not_built(label, t)
    }
}

/// The heading of a page: the title and, when there is one, the lede.
pub(crate) fn page_heading(title: String, lede: Option<String>, t: &Theme) -> gpui::Div {
    let heading = div().mb(px(16.0)).child(
        div()
            .text_size(px(20.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(title),
    );
    heading.children(lede.map(|lede| {
        div()
            .mt(px(4.0))
            .max_w(px(640.0))
            .text_size(px(13.5))
            .text_color(t.mutd)
            .child(lede)
    }))
}

/// The page of a screen that nothing draws yet.
fn not_built(label: &str, t: &Theme) -> AnyElement {
    div()
        .child(eyebrow_lg("Not built yet", t))
        .child(h1(label).mt(px(18.0)))
        .child(
            body(
                "This screen comes in a later phase of the rewrite.",
                16.0,
                t.mutd,
            )
            .mt(px(28.0))
            .max_w(ch(62.0, 16.0)),
        )
        .into_any_element()
}

impl Root {
    /// The width that the page gives to a grid of blocks.
    fn grid_width(&self, window: &Window) -> f32 {
        let width = crate::scale::viewport_w(window);
        let layout = self.layout(width);
        (width - layout.sidebar - layout.coach - 40.0).min(PAGE_MAX)
    }

    /// The page of a screen that is built from blocks: a heading, a lede and
    /// the grid. `None` when the screen file has no layout yet.
    fn block_page(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let def = blocks::screen_def(self.active)?;
        let today = self.active == Screen::Today;
        // Only Today has a layout that the user arranges.
        let layout = if today {
            self.today_layout.pairs()
        } else {
            blocks::default_layout(self.active).to_vec()
        };
        if layout.is_empty() && !today {
            return None;
        }
        let editing = self.editing();
        let ctx = self.ctx(t);
        let title = (def.title)(&ctx).unwrap_or_else(|| self.active.label().to_string());
        let heading = page_heading(title, (def.lede)(&ctx), t);
        if today && self.today.empty && !editing {
            return Some(
                div()
                    .child(heading)
                    .child("Nothing cached yet.")
                    .into_any_element(),
            );
        }
        let available = self.grid_width(window);
        let narrow = !self.grid_is_wide(available);
        let grid = if layout.is_empty() {
            blocks::edit::empty_box(editing, t)
        } else if editing {
            let selected = self.edit.as_ref().and_then(|e| e.selected);
            blocks::edit::grid(&self.today_layout, selected, &ctx, available, narrow, cx)
        } else {
            blocks::grid(&layout, &ctx, available, narrow, cx)
        };
        let page = div().child(heading).child(grid);
        Some(if editing {
            blocks::edit::page(self, page, cx).into_any_element()
        } else {
            page.into_any_element()
        })
    }

    /// The pages of the Activities screen that are not the block layout.
    fn activity_page(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let available = self.grid_width(window);
        let narrow = !self.grid_is_wide(available);
        match &self.page {
            Page::Detail(view) => activities::detail(view, available, narrow, t, cx),
            Page::All(at) => activities::all(&self.list, *at, available, t, cx),
            Page::Missing | Page::List | Page::Insight(_) => activities::missing(t, cx),
        }
    }
}

impl Root {
    /// One screen in the whole window, under a bare title bar: the brand and the window buttons.
    fn framed(&self, body: AnyElement, t: &Theme, window: &Window) -> gpui::Div {
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(SANS)
            .line_height(relative(LH_NORMAL))
            .child(titlebar::render_bare(window, t))
            .child(body)
            .children(chrome::resize_handles(window))
    }

    /// The text at the right of each entry, by `Screen::index`.
    fn nav_counts(&self) -> Vec<Option<String>> {
        Screen::ALL
            .iter()
            .map(|s| self.nav_count(*s).map(|n| crate::map::num(n as f64)))
            .collect()
    }

    /// The screen in the scrolling page. Every screen goes through this
    /// column, so all of them have the same width and the same centering.
    fn page(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let screen = self.screen(t, window, cx);
        let column = div().max_w(px(PAGE_MAX)).mx_auto();
        self.scroll
            .area(ElementId::NamedInteger("page".into(), self.visits as u64))
            .flex_1()
            .min_w_0()
            .h_full()
            .px(px(20.0))
            .pt(px(18.0))
            .pb(px(28.0))
            .child(column.child(screen))
            .into_any_element()
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.sidebar.tick(window, cx.has_active_drag()) {
            self.save_nav_order();
        }

        // The `rem` units of the toolkit follow the interface scale.
        window.set_rem_size(gpui::px(16.0 * crate::scale::get()));
        let t = self.palette.theme();
        if !self.account.connected {
            return self.framed(setup::render(self, &t, window, cx), &t, window);
        }
        let width = crate::scale::viewport_w(window);
        let layout = self.layout(width);
        let narrow = layout.sidebar == 0.0;
        let on_ask = self.active == Screen::Ask && !self.tabs.is_empty();
        // One Ask entity, in one place: the Ask screen, or the panel at the side.
        let coach_w = self.coach_w;
        self.ask.update(cx, |ask, _| {
            ask.set_panel(!on_ask);
            ask.set_panel_width(coach_w);
        });

        let nav = (!narrow).then(|| {
            let counts = self.nav_counts();
            let calendar = crate::calendar::sidebar_block(self, &t, cx).into_any_element();
            let edge = panes::handle(self, Side::Sidebar, &t, cx);
            let bar = sidebar::render(
                &self.sidebar,
                &counts,
                &t,
                cx,
                sidebar::Actions {
                    select: |this, screen, window, cx| this.select(screen, window, cx),
                    toggle_group: |this, group, _, cx| this.toggle_group(group, cx),
                },
                calendar,
                layout.sidebar,
            );
            div().relative().flex_none().h_full().child(bar).child(edge)
        });

        // Each screen gets a new `SmoothScroll` in `enter`, so it starts at the top.
        self.scroll.tick(window);
        if let Some(edit) = self.edit.as_ref().filter(|_| self.editing()) {
            edit.tick(window);
        }
        let content = if on_ask {
            // Ask has its own scroll, so it gets the same column without the page.
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .justify_center()
                .child(
                    div()
                        .w_full()
                        .max_w(px(PAGE_MAX))
                        .h_full()
                        .child(self.ask.clone()),
                )
                .into_any_element()
        } else {
            self.page(&t, window, cx)
        };
        let center = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .child(tabs::tab_bar(self, &t, cx))
            .child(tabs::crumb_bar(self, narrow, &t, cx))
            .child(div().flex_1().min_h_0().flex().child(content));
        let editing = self.editing();
        let coach = (layout.coach > 0.0 && !on_ask && !editing).then(|| {
            div()
                .relative()
                .flex_none()
                .w(px(layout.coach))
                .h_full()
                .bg(t.bg2)
                .border_l_1()
                .border_color(t.line)
                .child(self.ask.clone())
                .child(panes::handle(self, Side::Coach, &t, cx))
                .into_any_element()
        });
        let coach = if editing && layout.coach > 0.0 {
            blocks::edit::panel(self, layout.coach, &t, window, cx)
        } else {
            coach
        };
        let provider = self.ask.read(cx).provider_label();

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(SANS)
            .line_height(relative(LH_NORMAL))
            .when(self.pane_drag.is_some(), |d| d.cursor_ew_resize())
            .on_mouse_move(cx.listener(|root, event: &gpui::MouseMoveEvent, _, cx| {
                let pressed = event.pressed_button == Some(gpui::MouseButton::Left);
                root.pane_move(crate::scale::dp(event.position.x), pressed, cx)
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|root, _, _, cx| root.pane_release(cx)),
            )
            .child(titlebar::render(self, window, &t, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .children(nav)
                    .child(center)
                    .children(coach),
            )
            .child(statusbar::render(self, provider, narrow, &t, cx))
            .children(crate::update_ui::banner(self, &t, cx))
            .children(self.command_palette.clone())
            .children(crate::theme_dialog::render(self, &t, window, cx))
            .children(chrome::resize_handles(window))
    }
}
