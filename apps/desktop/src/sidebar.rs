//! The sidebar: the nav list, the lit entry that slides between rows, and the
//! sync and theme controls. From `app/src/components/Sidebar.tsx`.
//!
//! Only the order of the entries and the sliding highlight are ported. Drag to
//! reorder and the context menu come with the later phases.

use std::time::Instant;

use gpui::{
    div, prelude::*, px, relative, BoxShadow, ClickEvent, Context, Div, FontWeight, SharedString,
    TextTransform, Window,
};

use crate::icons::{icon, icon_hover, Icon};
use crate::scroll::SmoothScroll;
use crate::spring::{Spring, SETTLE};
use crate::theme::{Theme, FS_MD};
use crate::ui::SANS;

pub const SIDEBAR_W: f32 = 228.0;

/// The height of one entry: a 16px label at 1.2 line height, and 7px above and below.
const ROW_H: f32 = 16.0 * 1.2 + 14.0;
const GAP: f32 = 2.0;
/// The divider under the first entry: its rule, and the 7px and 8px around it.
const DIVIDE_H: f32 = 16.0;

/// The entries in the shipped order. Plan, Routes and Gear are left out until
/// those screens exist. Their icons stay in the table.
pub const NAV: [(&str, Icon); 15] = [
    ("Today", Icon::NavToday),
    ("Activities", Icon::NavActivities),
    ("Health", Icon::NavHealth),
    ("Sleep", Icon::NavSleep),
    ("Food", Icon::NavFood),
    ("Weight", Icon::NavWeight),
    ("Ask", Icon::NavAsk),
    ("Insights", Icon::NavInsights),
    ("Strength", Icon::NavStrength),
    ("Fitness", Icon::NavFitness),
    ("Plan", Icon::NavPlan),
    ("Routes", Icon::NavRoutes),
    ("Gear", Icon::NavGear),
    ("Reports", Icon::NavReports),
    ("Settings", Icon::NavSettings),
];

/// Where an entry rests inside the list, from the list's top edge.
fn row_top(index: usize) -> f32 {
    let pitch = ROW_H + GAP;
    // The first entry has the divider after it, and the divider has its own gap.
    index as f32 * pitch + if index > 0 { DIVIDE_H + GAP } else { 0.0 }
}

pub struct Sidebar {
    pub active: usize,
    /// How far the lit entry's ground is from its row. It springs to zero.
    pill: Spring,
    last_tick: Option<Instant>,
    /// The list scrolls when the window is too short for every entry.
    scroll: SmoothScroll,
    reduced_motion: bool,
}

impl Sidebar {
    pub fn new(reduced_motion: bool) -> Self {
        Self {
            active: 0,
            pill: Spring::new(SETTLE),
            last_tick: None,
            scroll: SmoothScroll::new(reduced_motion),
            reduced_motion,
        }
    }

    /// Light another entry. The ground slides from the old row to the new one.
    pub fn select(&mut self, index: usize) {
        if index == self.active {
            return;
        }
        // Start from where the ground is drawn now, so a second click mid-slide
        // continues from the current position.
        let drawn_top = row_top(self.active) + self.pill.x;
        self.active = index;
        self.pill.set(drawn_top - row_top(index));
        if self.reduced_motion {
            self.pill.set(0.0);
        } else {
            self.pill.to(0.0);
        }
        self.last_tick = None;
    }

    /// Advance the spring. Ask for another frame while it moves.
    pub fn tick(&mut self, window: &mut Window) {
        self.scroll.tick(window);
        if !self.pill.is_moving() {
            self.last_tick = None;
            return;
        }
        let now = Instant::now();
        let dt = self
            .last_tick
            .map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32());
        self.last_tick = Some(now);
        self.pill.step(dt);
        window.request_animation_frame();
    }

    fn pill_top(&self) -> f32 {
        row_top(self.active) + self.pill.x
    }
}

/// What the sidebar shows besides the nav.
pub struct Info<'a> {
    pub who: Option<&'a str>,
    pub synced_ago: Option<&'a str>,
    /// What a running sync reads, or the note after a failed one.
    pub sync_line: Option<&'a str>,
    pub sync_failed: bool,
}

fn quiet_row(t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .py(px(3.5))
        .font_family(SANS)
        .text_size(px(12.5))
        .line_height(relative(1.26))
        .text_color(t.faint)
}

fn entry<V: 'static>(
    index: usize,
    state: &Sidebar,
    t: &Theme,
    cx: &mut Context<V>,
    select: fn(&mut V, usize, &mut Window, &mut Context<V>),
) -> impl IntoElement {
    let (label, glyph) = NAV[index];
    let active = index == state.active;
    let fg = t.fg;
    let colour = if active { t.fg } else { t.mutd };
    let mut row = div()
        .id(("nav", index))
        .group("nav-row")
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(11.0))
        .h(px(ROW_H))
        .px(px(10.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .font_family(SANS)
        .text_size(px(FS_MD))
        .line_height(relative(1.2))
        .text_color(colour)
        .hover(move |s| s.text_color(fg))
        .on_click(
            cx.listener(move |view, _: &ClickEvent, window, cx| select(view, index, window, cx)),
        )
        .child(icon_hover(glyph, 17.0, t, colour, Some(("nav-row", t.fg))))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .when(active, |d| d.font_weight(FontWeight::MEDIUM))
                .child(SharedString::from(label)),
        );
    if index == 0 {
        // The word shows under the pointer only: the state it names never
        // changes on its own.
        row = row.child(
            div()
                .flex_none()
                .font_family(SANS)
                .text_size(px(9.0))
                .line_height(relative(1.0))
                .letter_spacing(px(9.0 * 0.13))
                .text_transform(TextTransform::Uppercase)
                .text_color(t.faint)
                .opacity(0.0)
                .group_hover("nav-row", |s| s.opacity(1.0))
                .child("Default"),
        );
    }
    row
}

/// What the sidebar does when the user clicks. Each one is a function of the view.
pub struct Actions<V> {
    pub select: fn(&mut V, usize, &mut Window, &mut Context<V>),
    pub toggle_theme: fn(&mut V, &mut Window, &mut Context<V>),
    pub sync_now: fn(&mut V, &mut Window, &mut Context<V>),
}

pub fn render<V: 'static>(
    state: &Sidebar,
    info: &Info,
    dark: bool,
    t: &Theme,
    cx: &mut Context<V>,
    actions: Actions<V>,
) -> impl IntoElement {
    let Actions {
        select,
        toggle_theme,
        sync_now,
    } = actions;
    // One element behind every row, so the rows' text always paints over it.
    let pill = div()
        .absolute()
        .top(px(state.pill_top()))
        .left_0()
        .right_0()
        .h(px(ROW_H))
        .rounded(px(8.0))
        .bg(t.bg)
        .shadow(vec![BoxShadow {
            color: t.shadow,
            offset: gpui::point(px(0.0), px(1.0)),
            blur_radius: px(2.0),
            spread_radius: px(0.0),
            inset: false,
        }]);

    let mut list = div()
        .relative()
        .flex()
        .flex_col()
        .gap(px(GAP))
        .w(px(188.0))
        .child(pill);
    for index in 0..NAV.len() {
        list = list.child(entry(index, state, t, cx, select));
        if index == 0 {
            list = list.child(
                div()
                    .flex_none()
                    .pt(px(7.0))
                    .pb(px(8.0))
                    .child(div().h(px(1.0)).bg(t.line)),
            );
        }
    }

    let sync_age = info
        .synced_ago
        .map(|a| format!("({a})"))
        .unwrap_or_default();
    let running = info.sync_line.is_some() && !info.sync_failed;
    let sync_fg = t.fg;
    let mut sync = quiet_row(t)
        .id("sync")
        .text_color(t.mutd)
        .child(icon(Icon::Sync, 14.0, t, t.mutd))
        .child(if running { "Syncing" } else { "Sync" })
        .child(div().text_color(t.faint).child(sync_age));
    if !running {
        sync = sync
            .cursor_pointer()
            .hover(move |s| s.text_color(sync_fg))
            .on_click(
                cx.listener(move |view, _: &ClickEvent, window, cx| sync_now(view, window, cx)),
            );
    }
    // The line is small and wraps. A long step name must not push the nav.
    let sync_note = info.sync_line.map(|line| {
        div()
            .w(px(SIDEBAR_W - 56.0))
            .font_family(SANS)
            .text_size(px(11.5))
            .line_height(relative(1.35))
            .text_color(if info.sync_failed { t.warn } else { t.faint })
            .child(line.to_string())
    });

    let (theme_icon, theme_label) = if dark {
        (Icon::Light, "Light mode")
    } else {
        (Icon::Dark, "Dark mode")
    };
    let fg = t.fg;
    let theme_row = quiet_row(t)
        .id("theme")
        .group("theme-row")
        .cursor_pointer()
        .hover(move |s| s.text_color(fg))
        .on_click(
            cx.listener(move |view, _: &ClickEvent, window, cx| toggle_theme(view, window, cx)),
        )
        .child(icon_hover(
            theme_icon,
            14.0,
            t,
            t.faint,
            Some(("theme-row", t.fg)),
        ))
        .child(theme_label);

    div()
        .flex_none()
        .w(px(SIDEBAR_W))
        .h_full()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .pt(px(38.0))
        .pb(px(40.0))
        .child(
            div()
                .pl(px(34.0))
                .mb(px(26.0))
                .font_family(SANS)
                .text_size(px(11.0))
                .line_height(relative(1.4))
                .letter_spacing(px(1.1))
                .text_transform(TextTransform::Uppercase)
                .text_color(t.faint)
                .child(info.who.unwrap_or("").to_string()),
        )
        .child(
            state
                .scroll
                .area("nav")
                .flex_1()
                .min_h_0()
                .pl(px(24.0))
                .child(list),
        )
        .child(
            div()
                .pl(px(34.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(sync)
                .children(sync_note)
                .child(theme_row),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_rest_where_the_stylesheet_puts_them() {
        assert!((row_top(0)).abs() < 1e-4);
        // Row 1 sits under row 0, the gap, the divider and a second gap.
        assert!((row_top(1) - (ROW_H + 2.0 + 16.0 + 2.0)).abs() < 1e-3);
        assert!((row_top(2) - row_top(1) - (ROW_H + 2.0)).abs() < 1e-3);
    }

    #[test]
    fn selecting_a_row_starts_the_ground_where_it_was_drawn() {
        let mut bar = Sidebar::new(false);
        bar.select(3);
        assert!((bar.pill_top() - row_top(0)).abs() < 1e-3);
        assert!(bar.pill.is_moving());
        bar.pill.step(0.016);
        let mid = bar.pill_top();
        // A second click mid-slide continues from the drawn position.
        bar.select(1);
        assert!((bar.pill_top() - mid).abs() < 1e-3);
        for _ in 0..120 {
            bar.pill.step(1.0 / 60.0);
        }
        assert!((bar.pill_top() - row_top(1)).abs() < 1e-3 && !bar.pill.is_moving());
    }

    #[test]
    fn selecting_the_lit_row_changes_nothing() {
        let mut bar = Sidebar::new(false);
        bar.select(0);
        assert!(!bar.pill.is_moving());
    }
}
