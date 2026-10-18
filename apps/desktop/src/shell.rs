//! The window's root view: sidebar, reading column, and the window chrome.
//! From `DesktopShell` in `app/src/router.tsx`.

use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, relative, ColorExt as _, Context,
    ElementId, IntoElement, Render, Window,
};

use crate::body::{self, BodyData};
use crate::chrome;
use crate::data::Today;
use crate::motion;
use crate::scroll::SmoothScroll;
use crate::sidebar::{self, Info, Sidebar, NAV, SIDEBAR_W};
use crate::theme::{Palette, Theme};
use crate::today;
use crate::ui::{block_in, body, ch, eyebrow_lg, h1, LH_NORMAL, SANS};

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

pub struct Root {
    today: Today,
    body: BodyData,
    palette: Palette,
    sidebar: Sidebar,
    /// Counts the screens shown, so each one animates in again.
    visits: usize,
    scroll: SmoothScroll,
}

impl Root {
    pub fn new(today: Today, palette: Palette, body: BodyData) -> Self {
        Self {
            today,
            body,
            palette,
            sidebar: Sidebar::new(motion::reduced()),
            visits: 0,
            scroll: SmoothScroll::new(motion::reduced()),
        }
    }

    /// Start on the screen with this sidebar label. An unknown label keeps Today.
    pub fn open(mut self, label: &str) -> Self {
        if let Some(i) = NAV.iter().position(|(n, _)| n.eq_ignore_ascii_case(label)) {
            self.sidebar.select(i);
        }
        self
    }

    fn select(&mut self, index: usize, _: &mut Window, cx: &mut Context<Self>) {
        if index != self.sidebar.active {
            self.sidebar.select(index);
            self.visits += 1;
            // A new screen starts at the top.
            self.scroll = SmoothScroll::new(motion::reduced());
            cx.notify();
        }
    }

    /// Light and dark Paper swap. Any other palette settles light or dark for itself.
    fn toggle_theme(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.palette = if self.palette.theme().dark {
            Palette::Paper
        } else {
            Palette::Ink
        };
        cx.notify();
    }

    fn screen(&self, t: &Theme) -> gpui::AnyElement {
        if self.sidebar.active == 0 {
            return today::render(&self.today, self.visits, t).into_any_element();
        }
        let label = NAV[self.sidebar.active].0;
        if let Some(screen) = body::screen(label, &self.body, self.visits, t) {
            return screen;
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
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sidebar.tick(window);

        let t = self.palette.theme();
        let viewport = f32::from(window.viewport_size().width);
        // What lies left of the capped pair carries the nav's ground too.
        let margin = ((viewport - SHELL_MAX) / 2.0).max(0.0);

        let info = Info {
            who: self.today.who.as_deref(),
            synced_ago: self.today.synced_ago.as_deref(),
        };
        let nav = sidebar::render(
            &self.sidebar,
            &info,
            t.dark,
            &t,
            cx,
            |this, index, window, cx| this.select(index, window, cx),
            |this, window, cx| this.toggle_theme(window, cx),
        );

        // Each screen gets a new `SmoothScroll` in `select`, so it starts at
        // the top, as the current app does on a route change.
        self.scroll.tick(window);
        let column = self
            .scroll
            .area(ElementId::NamedInteger("page".into(), self.visits as u64))
            .flex_1()
            .min_w_0()
            .h_full()
            .px(px(COLUMN_SIDE))
            .pb(px(COLUMN_BOTTOM))
            .child(
                div()
                    .max_w(px(COLUMN_MAX))
                    .pt(px(COLUMN_TOP))
                    .child(self.screen(&t)),
            );

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
