//! The window's root view: sidebar, reading column, and the window chrome.
//! From `DesktopShell` in `app/src/router.tsx`.

use garmin_api::settings::ThemeMode;
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, relative, ColorExt as _, Context,
    ElementId, IntoElement, Render, Window,
};

use crate::account::{Account, Sync};
use crate::chrome;
use crate::data::{Source, Today};
use crate::motion;
use crate::scroll::SmoothScroll;
use crate::sidebar::{self, Info, Sidebar, NAV, SIDEBAR_W};
use crate::theme::{Palette, Theme};
use crate::ui::{block_in, body, ch, eyebrow_lg, h1, LH_NORMAL, SANS};
use crate::{settings, setup, today};

/// The sidebar and the column, as a pair, are never wider than this.
/// On an ultrawide the pair stays together in the middle.
const SHELL_MAX: f32 = 1240.0;
/// The reading column's width, and the air above and beside it.
const COLUMN_MAX: f32 = 720.0;
const COLUMN_TOP: f32 = 78.0;
const COLUMN_SIDE: f32 = 56.0;
const COLUMN_BOTTOM: f32 = 160.0;
/// How tall the fade under the title strip is.
const FADE_H: f32 = 44.0;

/// The entry of the Settings screen: the last one in the nav.
const SETTINGS: usize = NAV.len() - 1;

pub struct Root {
    pub(crate) today: Today,
    pub(crate) palette: Palette,
    sidebar: Sidebar,
    /// Counts the screens shown, so each one animates in again.
    pub(crate) visits: usize,
    scroll: SmoothScroll,
    /// Where Today reads from. A sync reads it again.
    pub(crate) source: Source,
    pub(crate) account: Account,
}

impl Root {
    pub fn new(today: Today, palette: Palette, source: Source, account: Account) -> Self {
        Self {
            today,
            palette,
            sidebar: Sidebar::new(motion::reduced()),
            visits: 0,
            scroll: SmoothScroll::new(motion::reduced()),
            source,
            account,
        }
    }

    /// Show the screen with this nav label. For a start flag and the screenshots.
    pub fn open(&mut self, label: &str, cx: &mut Context<Self>) {
        if let Some(index) = NAV.iter().position(|(n, _)| n.eq_ignore_ascii_case(label)) {
            self.select_index(index, cx);
        }
    }

    fn select(&mut self, index: usize, _: &mut Window, cx: &mut Context<Self>) {
        self.select_index(index, cx);
    }

    fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if index != self.sidebar.active {
            self.sidebar.select(index);
            self.visits += 1;
            if index == SETTINGS {
                self.reload_settings();
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

    fn screen(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.sidebar.active == 0 {
            return today::render(&self.today, self.visits, t).into_any_element();
        }
        if self.sidebar.active == SETTINGS {
            return settings::render(self, t, cx);
        }
        let label = NAV[self.sidebar.active].0;
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
        let screen = self.screen(&t, cx);
        let column = self
            .scroll
            .area(ElementId::NamedInteger("page".into(), self.visits as u64))
            .flex_1()
            .min_w_0()
            .h_full()
            .px(px(COLUMN_SIDE))
            .pb(px(COLUMN_BOTTOM))
            .child(div().max_w(px(COLUMN_MAX)).pt(px(COLUMN_TOP)).child(screen));

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
