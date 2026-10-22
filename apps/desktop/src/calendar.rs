//! The calendar that sets the focus: a month grid in the sidebar and in the
//! popover of the breadcrumb bar. Both use `body`.
//!
//! A click on a day focuses the day. A shift click or a drag focuses a range.
//! A day after today is not selectable. The functions at the top are pure
//! date math. The `Root` methods and the render functions follow them.

use crate::scale::px;
use chrono::{Datelike, Months, NaiveDate};
use gpui::{
    anchored, deferred, div, point, prelude::*, ClickEvent, Context, Div, ElementId, FocusHandle,
    FontWeight, MouseButton, SharedString, Window,
};

use crate::focus::{real_today, Focus};
use crate::icons::{icon, Icon};
use crate::kit::{chip, link};
use crate::shell::Root;
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

/// The width of the popover, with its padding.
const POPOVER_W: f32 = 232.0;
const CELL_H: f32 = 24.0;

/// A ready range in the preset row and in the command palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Today,
    Days7,
    Week,
    Month,
    Days30,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::Today,
        Preset::Days7,
        Preset::Week,
        Preset::Month,
        Preset::Days30,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Today => "Today",
            Preset::Days7 => "7 days",
            Preset::Week => "This week",
            Preset::Month => "This month",
            Preset::Days30 => "30 days",
        }
    }

    /// The range of the preset. It always ends at `today`.
    pub fn focus(self, today: NaiveDate) -> Focus {
        let back = |n: i64| Focus::range(today - chrono::Duration::days(n), today);
        match self {
            Preset::Today => Focus::day(today),
            Preset::Days7 => back(6),
            Preset::Week => back(i64::from(today.weekday().num_days_from_monday())),
            Preset::Month => Focus::range(first_of_month(today), today),
            Preset::Days30 => back(29),
        }
    }
}

pub fn first_of_month(day: NaiveDate) -> NaiveDate {
    day.with_day(1).unwrap_or(day)
}

/// The first day of the month `delta` months from `first`.
pub fn shift_month(first: NaiveDate, delta: i32) -> NaiveDate {
    let months = Months::new(delta.unsigned_abs());
    let moved = if delta < 0 {
        first.checked_sub_months(months)
    } else {
        first.checked_add_months(months)
    };
    moved.unwrap_or(first)
}

/// The days of the month of `first` in rows of seven, Monday first. A cell
/// outside the month is `None`. The last row is padded.
pub fn month_cells(first: NaiveDate) -> Vec<Option<NaiveDate>> {
    let lead = first.weekday().num_days_from_monday() as usize;
    let mut cells: Vec<Option<NaiveDate>> = vec![None; lead];
    let mut day = first;
    while day.month() == first.month() {
        cells.push(Some(day));
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
    while !cells.len().is_multiple_of(7) {
        cells.push(None);
    }
    cells
}

/// A day after today cannot be picked.
pub fn selectable(day: NaiveDate, today: NaiveDate) -> bool {
    day <= today
}

/// The range between two clicks, in any order. It stops at today.
pub fn pick_range(a: NaiveDate, b: NaiveDate, today: NaiveDate) -> Focus {
    Focus::range(a.min(today), b.min(today))
}

/// For each day of the month of `first`, true when a session starts on it.
/// `dates` are `YYYY-MM-DD` texts, in any order.
pub fn month_dots<'a>(first: NaiveDate, dates: impl Iterator<Item = &'a str>) -> Vec<bool> {
    let prefix = first.format("%Y-%m-").to_string();
    let mut dots = vec![false; 31];
    for date in dates {
        let day = date
            .strip_prefix(prefix.as_str())
            .and_then(|d| d.parse::<usize>().ok());
        if let Some(day) = day.filter(|d| (1..=31).contains(d)) {
            dots[day - 1] = true;
        }
    }
    dots
}

/// How a day looks against the focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    /// The end of a range, or the only day.
    End,
    /// Inside a range.
    Inside,
    None,
}

fn mark(day: NaiveDate, shown: Focus) -> Mark {
    if day == shown.start || day == shown.end {
        Mark::End
    } else if shown.contains(day) {
        Mark::Inside
    } else {
        Mark::None
    }
}

/// What the calendar remembers between frames.
pub struct Calendar {
    /// The first day of the month on show.
    pub month: NaiveDate,
    /// The day that a shift click extends from.
    pivot: NaiveDate,
    /// The ends of the drag in flight. The focus changes when the button comes up.
    drag: Option<(NaiveDate, NaiveDate)>,
    /// The sidebar block is closed.
    pub collapsed: bool,
    /// The popover of the breadcrumb bar is open. It holds the focus for Escape.
    pub popover: Option<FocusHandle>,
}

impl Calendar {
    pub fn new(focus: Focus, collapsed: bool) -> Self {
        Self {
            month: first_of_month(focus.end),
            pivot: focus.end,
            drag: None,
            collapsed,
            popover: None,
        }
    }
}

impl Root {
    fn cal_press(&mut self, day: NaiveDate, shift: bool, cx: &mut Context<Self>) {
        let today = real_today();
        if !selectable(day, today) {
            return;
        }
        if shift {
            let range = pick_range(self.calendar.pivot, day, today);
            return self.cal_commit(range, cx);
        }
        self.calendar.pivot = day;
        self.calendar.drag = Some((day, day));
        cx.notify();
    }

    /// The pointer is over a day. `pressed` is true while the left button is down.
    fn cal_over(&mut self, day: NaiveDate, pressed: bool, cx: &mut Context<Self>) {
        if !pressed {
            // The button came up outside the window: the drag ends here.
            return self.cal_release(cx);
        }
        let today = real_today();
        if let Some((_, end)) = self.calendar.drag.as_mut() {
            if *end != day && selectable(day, today) {
                *end = day;
                cx.notify();
            }
        }
    }

    fn cal_release(&mut self, cx: &mut Context<Self>) {
        if let Some((a, b)) = self.calendar.drag.take() {
            self.cal_commit(pick_range(a, b, real_today()), cx);
        }
    }

    /// Focus the range and close the popover.
    fn cal_commit(&mut self, range: Focus, cx: &mut Context<Self>) {
        self.calendar.popover = None;
        self.set_focus(range, cx);
        cx.notify();
    }

    fn cal_month(&mut self, delta: i32, cx: &mut Context<Self>) {
        let next = shift_month(self.calendar.month, delta);
        if next <= first_of_month(real_today()) {
            self.calendar.month = next;
            cx.notify();
        }
    }

    pub(crate) fn toggle_calendar_block(&mut self, cx: &mut Context<Self>) {
        self.calendar.collapsed = !self.calendar.collapsed;
        self.save_window_state();
        cx.notify();
    }

    pub(crate) fn toggle_calendar_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.calendar.popover.take().is_none() {
            let handle = cx.focus_handle();
            window.focus(&handle, cx);
            self.calendar.popover = Some(handle);
            self.calendar.month = first_of_month(self.focus.end);
        }
        cx.notify();
    }

    fn close_calendar_popover(&mut self, cx: &mut Context<Self>) {
        self.calendar.popover = None;
        cx.notify();
    }
}

fn name(scope: &str, part: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{scope}-{part}")))
}

fn month_button(scope: &str, delta: i32, enabled: bool, t: &Theme, cx: &mut Context<Root>) -> Div {
    let (glyph, part) = if delta < 0 {
        (Icon::ChevronLeft, "prev")
    } else {
        (Icon::ChevronRight, "next")
    };
    let hover = t.hover;
    let button = div()
        .id(name(scope, part))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.0))
        .rounded(px(4.0))
        .child(icon(glyph, 12.0, t, if enabled { t.mutd } else { t.faint }));
    if !enabled {
        return div().opacity(0.5).child(button);
    }
    div().child(
        button
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.cal_month(delta, cx))),
    )
}

fn day_cell(
    scope: &'static str,
    index: usize,
    day: NaiveDate,
    shown: Focus,
    dot: bool,
    t: &Theme,
    cx: &mut Context<Root>,
) -> impl IntoElement {
    let today = real_today();
    let open = selectable(day, today);
    let mark = mark(day, shown);
    let (bg, fg) = match mark {
        Mark::End => (t.acc, t.bg),
        Mark::Inside => (t.acctint, t.acc),
        Mark::None => (gpui::transparent_black(), if open { t.fg } else { t.faint }),
    };
    let ring = if day == today {
        t.acc
    } else {
        gpui::transparent_black()
    };
    let dot_colour = if mark == Mark::End { t.bg } else { t.acc };
    let hover = t.hover;
    let cell = div()
        .id((scope, index))
        .relative()
        .flex_1()
        .h(px(CELL_H))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .border_1()
        .border_color(ring)
        .bg(bg)
        .font_family(MONO)
        .text_size(px(11.5))
        .text_color(fg)
        .child(day.day().to_string())
        .when(dot, |d| {
            d.child(
                div()
                    .absolute()
                    .bottom(px(2.0))
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(div().size(px(3.0)).rounded_full().bg(dot_colour)),
            )
        });
    if !open {
        return cell.opacity(0.5);
    }
    cell.cursor_pointer()
        .when(mark == Mark::None, |d| d.hover(move |s| s.bg(hover)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |root, event: &gpui::MouseDownEvent, _, cx| {
                root.cal_press(day, event.modifiers.shift, cx)
            }),
        )
        .on_mouse_move(
            cx.listener(move |root, event: &gpui::MouseMoveEvent, _, cx| {
                let pressed = event.pressed_button == Some(MouseButton::Left);
                root.cal_over(day, pressed, cx)
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|root, _, _, cx| root.cal_release(cx)),
        )
        // A release outside every cell still ends the drag. `cal_release` runs
        // once, because it takes the drag.
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|root, _, _, cx| root.cal_release(cx)),
        )
}

fn weekdays(t: &Theme) -> Div {
    let mut row = div().flex().h(px(18.0));
    for initial in ["M", "T", "W", "T", "F", "S", "S"] {
        row = row.child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .font_family(SANS)
                .text_size(px(10.5))
                .text_color(t.faint)
                .child(initial),
        );
    }
    row
}

fn grid(root: &Root, scope: &'static str, shown: Focus, t: &Theme, cx: &mut Context<Root>) -> Div {
    let first = root.calendar.month;
    let dots = match &root.list {
        Ok(view) => month_dots(first, view.groups.iter().map(|g| g.date.as_str())),
        Err(_) => vec![false; 31],
    };
    let cells = month_cells(first);
    let mut rows = div().flex().flex_col();
    for (r, week) in cells.chunks(7).enumerate() {
        let mut row = div().flex().h(px(CELL_H));
        for (c, cell) in week.iter().enumerate() {
            row = match cell {
                Some(day) => {
                    let dot = dots[day.day0() as usize];
                    row.child(day_cell(scope, r * 7 + c, *day, shown, dot, t, cx))
                }
                None => row.child(div().flex_1()),
            };
        }
        rows = rows.child(row);
    }
    rows
}

fn presets(root: &Root, scope: &'static str, t: &Theme, cx: &mut Context<Root>) -> Div {
    let today = real_today();
    let mut row = div().flex().flex_wrap().gap(px(4.0));
    for (i, preset) in Preset::ALL.into_iter().enumerate() {
        let target = preset.focus(today);
        let on = root.focus == target;
        row = row.child(
            div()
                .id((scope, 100 + i))
                .cursor_pointer()
                .child(chip(preset.label(), on, false, t))
                .on_click(
                    cx.listener(move |root, _: &ClickEvent, _, cx| root.cal_commit(target, cx)),
                ),
        );
    }
    row
}

/// The month title, the grid, the presets and the link back to today.
pub fn body(root: &Root, scope: &'static str, t: &Theme, cx: &mut Context<Root>) -> Div {
    let today = real_today();
    let shown = root
        .calendar
        .drag
        .map_or(root.focus, |(a, b)| pick_range(a, b, today));
    let first = root.calendar.month;
    let can_next = shift_month(first, 1) <= first_of_month(today);
    let head = div()
        .flex()
        .items_center()
        .h(px(26.0))
        .child(
            div()
                .flex_1()
                .pl(px(4.0))
                .font_family(SANS)
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.fg)
                .child(first.format("%B %Y").to_string()),
        )
        .child(month_button(scope, -1, true, t, cx))
        .child(month_button(scope, 1, can_next, t, cx));
    let mut block = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .w_full()
        .child(head)
        .child(weekdays(t))
        .child(grid(root, scope, shown, t, cx))
        .child(presets(root, scope, t, cx));
    if !root.focus.is_live() {
        block = block.child(link(name(scope, "live"), "Back to today", t).on_click(
            cx.listener(|root, _: &ClickEvent, _, cx| root.cal_commit(Focus::today(), cx)),
        ));
    }
    // A release outside the grid ends a drag as well.
    block.on_mouse_up_out(
        MouseButton::Left,
        cx.listener(|root, _, _, cx| root.cal_release(cx)),
    )
}

/// The block above the Settings row: a header that closes it, and the body.
pub fn sidebar_block(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
    let (mutd, faint) = (t.mutd, t.faint);
    let closed = root.calendar.collapsed;
    let head = div()
        .id("cal-head")
        .group("cal-head")
        .flex()
        .items_center()
        .gap(px(6.0))
        .h(px(24.0))
        .px(px(8.0))
        .cursor_pointer()
        .font_family(SANS)
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(faint)
        .hover(move |s| s.text_color(mutd))
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.toggle_calendar_block(cx)))
        .child(icon(
            if closed {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            },
            10.0,
            t,
            faint,
        ))
        .child("Calendar")
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_right()
                .font_family(MONO)
                .text_size(px(11.0))
                .child(root.focus.label()),
        );
    let mut block = div()
        .flex_none()
        .mt(px(6.0))
        .pt(px(4.0))
        .border_t_1()
        .border_color(t.line)
        .child(head);
    if !closed {
        block = block.child(
            div()
                .px(px(2.0))
                .pb(px(2.0))
                .child(body(root, "cal-side", t, cx)),
        );
    }
    block
}

/// The popover under the breadcrumb. `None` while it is closed.
pub fn popover(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
    let handle = root.calendar.popover.clone()?;
    let panel = div()
        .id("cal-popover")
        .track_focus(&handle)
        .occlude()
        .w(px(POPOVER_W))
        .p(px(10.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(t.line)
        .bg(t.bg)
        .shadow(vec![gpui::BoxShadow {
            color: t.shadow,
            offset: point(px(0.0), px(1.0)),
            blur_radius: px(2.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, _, cx| {
            if event.keystroke.key == "escape" {
                root.close_calendar_popover(cx);
            }
        }))
        .on_mouse_down_out(cx.listener(|root, _, _, cx| root.close_calendar_popover(cx)))
        .child(body(root, "cal-pop", t, cx));
    Some(
        deferred(
            anchored()
                .offset(point(px(-6.0), px(28.0)))
                .snap_to_window_with_margin(px(8.0))
                .child(panel),
        )
        .with_priority(2)
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn the_grid_starts_on_monday_and_pads_whole_weeks() {
        // 1 October 2026 is a Thursday: three empty cells first.
        let cells = month_cells(d(2026, 10, 1));
        assert_eq!(cells.len() % 7, 0);
        assert!(cells[..3].iter().all(Option::is_none));
        assert_eq!(cells[3], Some(d(2026, 10, 1)));
        assert_eq!(cells.iter().flatten().count(), 31);
        assert_eq!(cells.iter().flatten().last(), Some(&d(2026, 10, 31)));
        // February 2027 starts on a Monday and has four weeks.
        let feb = month_cells(d(2027, 2, 1));
        assert_eq!(feb.len(), 28);
        assert_eq!(feb[0], Some(d(2027, 2, 1)));
    }

    #[test]
    fn a_range_is_ordered_and_stops_at_today() {
        let today = d(2026, 10, 20);
        let back = pick_range(d(2026, 10, 18), d(2026, 10, 14), today);
        assert_eq!((back.start, back.end), (d(2026, 10, 14), d(2026, 10, 18)));
        let past_end = pick_range(d(2026, 10, 18), d(2026, 10, 28), today);
        assert_eq!(past_end.end, today);
        assert_eq!(pick_range(today, today, today), Focus::day(today));
    }

    #[test]
    fn a_day_after_today_is_not_selectable() {
        let today = d(2026, 10, 20);
        assert!(selectable(today, today));
        assert!(selectable(d(2026, 10, 19), today));
        assert!(!selectable(d(2026, 10, 21), today));
    }

    #[test]
    fn the_presets_end_today() {
        // 20 October 2026 is a Tuesday.
        let today = d(2026, 10, 20);
        let range = |p: Preset| {
            let f = p.focus(today);
            (f.start, f.end)
        };
        assert_eq!(range(Preset::Today), (today, today));
        assert_eq!(range(Preset::Days7), (d(2026, 10, 14), today));
        assert_eq!(range(Preset::Week), (d(2026, 10, 19), today));
        assert_eq!(range(Preset::Month), (d(2026, 10, 1), today));
        assert_eq!(range(Preset::Days30), (d(2026, 9, 21), today));
        assert_eq!(Preset::Days7.focus(today).days(), 7);
        assert_eq!(Preset::Days30.focus(today).days(), 30);
    }

    #[test]
    fn the_month_moves_both_ways_across_a_year() {
        assert_eq!(shift_month(d(2026, 1, 1), -1), d(2025, 12, 1));
        assert_eq!(shift_month(d(2026, 12, 1), 1), d(2027, 1, 1));
        assert_eq!(first_of_month(d(2026, 10, 20)), d(2026, 10, 1));
    }

    #[test]
    fn a_mark_tells_the_ends_from_the_inside() {
        let f = Focus::range(d(2026, 10, 14), d(2026, 10, 18));
        assert_eq!(mark(d(2026, 10, 14), f), Mark::End);
        assert_eq!(mark(d(2026, 10, 18), f), Mark::End);
        assert_eq!(mark(d(2026, 10, 16), f), Mark::Inside);
        assert_eq!(mark(d(2026, 10, 19), f), Mark::None);
        assert_eq!(
            mark(d(2026, 10, 14), Focus::day(d(2026, 10, 14))),
            Mark::End
        );
    }

    #[test]
    fn a_dot_marks_a_day_with_a_session_in_this_month_only() {
        let dates = ["2026-10-20", "2026-10-03", "2026-09-30", "bad"];
        let dots = month_dots(d(2026, 10, 1), dates.into_iter());
        assert!(dots[19] && dots[2]);
        assert_eq!(dots.iter().filter(|x| **x).count(), 2);
    }
}
