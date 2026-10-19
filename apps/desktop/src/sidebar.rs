//! The sidebar: the nav list, the lit entry that slides between rows, and the
//! sync and theme controls. From `app/src/components/Sidebar.tsx`.
//!
//! The user orders the entries by drag or by the right-click menu. The entry at
//! the top is the screen the app opens on. The keyboard moves of the old
//! sidebar are not ported.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    anchored, deferred, div, point, prelude::*, px, relative, AnyElement, Bounds, BoxShadow,
    ClickEvent, Context, Div, DragMoveEvent, FontWeight, MouseButton, Pixels, Point, Render,
    SharedString, TextTransform, Window,
};

use crate::icons::{icon, icon_hover, Icon};
use crate::scroll::SmoothScroll;
use crate::spring::{Spring, SETTLE};
use crate::theme::{Theme, FS_MD, FS_SMALL};
use crate::ui::SANS;

pub const SIDEBAR_W: f32 = 228.0;

/// The height of one entry: a 16px label at 1.2 line height, and 7px above and below.
const ROW_H: f32 = 16.0 * 1.2 + 14.0;
const GAP: f32 = 2.0;
/// The divider under the first entry: its rule, and the 7px and 8px around it.
const DIVIDE_H: f32 = 16.0;
/// The width of the list, and so of an entry.
const LIST_W: f32 = 188.0;

/// The entries in the shipped order.
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

/// Where a row rests by the stylesheet's numbers, from the list's top edge. The
/// sidebar uses this only until it has measured the real rows.
fn row_top(position: usize) -> f32 {
    let pitch = ROW_H + GAP;
    // The first entry has the divider after it, and the divider has its own gap.
    position as f32 * pitch + if position > 0 { DIVIDE_H + GAP } else { 0.0 }
}

/// The saved key of an entry. It stays the same when the order changes.
fn key_of(nav: usize) -> String {
    NAV[nav].0.to_lowercase()
}

/// The entries in the order of the saved keys. An unknown or repeated key is
/// dropped. An entry that the keys miss goes last, so a new screen never takes
/// over as the default.
fn order_from_keys(keys: &[String]) -> Vec<usize> {
    let mut order = Vec::with_capacity(NAV.len());
    for key in keys {
        let found = (0..NAV.len()).find(|&i| key_of(i) == *key);
        if let Some(i) = found.filter(|i| !order.contains(i)) {
            order.push(i);
        }
    }
    let missing: Vec<usize> = (0..NAV.len()).filter(|i| !order.contains(i)).collect();
    order.extend(missing);
    order
}

/// The order with the entry at `from` put at `to`.
fn moved(order: &[usize], from: usize, to: usize) -> Vec<usize> {
    let mut next = order.to_vec();
    let entry = next.remove(from);
    next.insert(to, entry);
    next
}

/// A drag in progress, by position in the order.
#[derive(Clone, Copy)]
struct Drag {
    from: usize,
    to: usize,
}

/// The context menu: the position of its entry and where the pointer was.
#[derive(Clone, Copy)]
struct Menu {
    position: usize,
    at: Point<Pixels>,
}

/// What the event handlers of the list write. `tick` reads it each frame.
#[derive(Default)]
struct Shared {
    /// Where each row rests, from the first row, as the layout puts it.
    /// Empty until the first paint.
    tops: Vec<f32>,
    row_h: f32,
    /// Where the first row is in the window, for the pointer.
    origin_y: f32,
    drag: Option<Drag>,
    /// Where the left button went down, in window height.
    press_y: f32,
    menu: Option<Menu>,
    /// A move to apply: the position it starts at, and where it ends.
    command: Option<(usize, usize)>,
}

impl Shared {
    /// Keep the measured rows. True when the rows moved, so the view must draw again.
    fn store_rows(&mut self, tops: Vec<f32>, row_h: f32, origin_y: f32) -> bool {
        self.origin_y = origin_y;
        let same = self.tops.len() == tops.len()
            && self
                .tops
                .iter()
                .zip(&tops)
                .all(|(a, b)| (a - b).abs() < 0.01)
            && (self.row_h - row_h).abs() < 0.01;
        if !same {
            self.tops = tops;
            self.row_h = row_h;
        }
        !same
    }

    /// The position that the dragged row at `from` takes with the pointer at
    /// this window height. The centre of the held row moves with the pointer
    /// from the press. It is compared with the centres of the other rows, as
    /// in the old app. A press low in a row does not change the result.
    fn position_at(&self, y: f32, from: usize) -> usize {
        let held = self.tops[from] + self.row_h / 2.0 + (y - self.press_y);
        let passed = self
            .tops
            .iter()
            .enumerate()
            .filter(|(i, top)| *i != from && **top + self.row_h / 2.0 < held);
        passed.count().min(NAV.len() - 1)
    }
}

pub struct Sidebar {
    /// The lit entry, as an index into `NAV`.
    pub active: usize,
    /// The entries in the user's order, as indices into `NAV`.
    order: Vec<usize>,
    shared: Rc<RefCell<Shared>>,
    /// How far each entry is drawn from its row, by index into `NAV`. A
    /// moved entry springs to zero.
    shift: Vec<Spring>,
    /// How far the lit entry's ground is from its row. It springs to zero.
    pill: Spring,
    last_tick: Option<Instant>,
    /// The list scrolls when the window is too short for every entry.
    scroll: SmoothScroll,
    reduced_motion: bool,
}

impl Sidebar {
    /// `keys` is the saved order. The app starts on the first entry of it.
    pub fn new(reduced_motion: bool, keys: &[String]) -> Self {
        let order = order_from_keys(keys);
        Self {
            active: order[0],
            order,
            shared: Rc::default(),
            shift: vec![Spring::new(SETTLE); NAV.len()],
            pill: Spring::new(SETTLE),
            last_tick: None,
            scroll: SmoothScroll::new(reduced_motion),
            reduced_motion,
        }
    }

    /// The keys to save, in the order of the list.
    pub fn keys(&self) -> Vec<String> {
        self.order.iter().map(|&i| key_of(i)).collect()
    }

    fn position(&self, nav: usize) -> usize {
        self.order.iter().position(|&i| i == nav).unwrap_or(0)
    }

    /// Where a row rests: as measured, or by the stylesheet before the first paint.
    fn top_of(&self, position: usize) -> f32 {
        let shared = self.shared.borrow();
        shared
            .tops
            .get(position)
            .copied()
            .unwrap_or_else(|| row_top(position))
    }

    fn row_height(&self) -> f32 {
        let h = self.shared.borrow().row_h;
        if h > 0.0 {
            h
        } else {
            ROW_H
        }
    }

    /// The entry that opens when the app starts. A drag shows where it would end.
    fn default_nav(&self) -> usize {
        match self.shared.borrow().drag {
            Some(d) => moved(&self.order, d.from, d.to)[0],
            None => self.order[0],
        }
    }

    /// Light another entry. The ground slides from the old row to the new one.
    pub fn select(&mut self, index: usize) {
        if index == self.active {
            return;
        }
        // Start from where the ground is drawn now, so a second click mid-slide
        // continues from the current position.
        let drawn_top = self.pill_top();
        self.active = index;
        let rest = self.pill_top() - self.pill.x;
        self.pill.set(drawn_top - rest);
        if self.reduced_motion {
            self.pill.set(0.0);
        } else {
            self.pill.to(0.0);
        }
        self.last_tick = None;
    }

    /// Put the entry at position `from` at position `to`. The rows keep the
    /// place they are drawn at and glide to the new place. True when the order changed.
    fn apply_move(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.order.len() || to >= self.order.len() {
            return false;
        }
        let next = moved(&self.order, from, to);
        for (new, &nav) in next.iter().enumerate() {
            let shift = self.top_of(self.position(nav)) - self.top_of(new);
            let spring = &mut self.shift[nav];
            spring.x += shift;
            spring.target = 0.0;
            if self.reduced_motion {
                spring.set(0.0);
            }
        }
        self.order = next;
        self.last_tick = None;
        true
    }

    /// Take in what the handlers wrote since the last frame.
    fn take_gestures(&mut self, dragging: bool) -> bool {
        let command = {
            let mut shared = self.shared.borrow_mut();
            // The pointer was let go, maybe outside the window: the drag ends here.
            if !dragging {
                if let Some(d) = shared.drag.take() {
                    shared.command = Some((d.from, d.to));
                }
            }
            shared.command.take()
        };
        command.is_some_and(|(from, to)| self.apply_move(from, to))
    }

    /// During a drag, send each row to the place that a release would leave it in.
    fn aim_rows(&mut self) {
        let Some(d) = self.shared.borrow().drag else {
            return;
        };
        for (new, &nav) in moved(&self.order, d.from, d.to).iter().enumerate() {
            let target = self.top_of(new) - self.top_of(self.position(nav));
            if self.reduced_motion {
                self.shift[nav].set(target);
            } else {
                self.shift[nav].to(target);
            }
        }
    }

    /// Advance the springs. Ask for another frame while one moves.
    /// `dragging` is true while a drag is in flight. True when the order changed.
    pub fn tick(&mut self, window: &mut Window, dragging: bool) -> bool {
        self.scroll.tick(window);
        let changed = self.take_gestures(dragging);
        self.aim_rows();
        let moving = self.pill.is_moving() || self.shift.iter().any(Spring::is_moving);
        if !moving {
            self.last_tick = None;
            return changed;
        }
        let now = Instant::now();
        let dt = self
            .last_tick
            .map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32());
        self.last_tick = Some(now);
        self.pill.step(dt);
        for spring in &mut self.shift {
            spring.step(dt);
        }
        window.request_animation_frame();
        changed
    }

    fn pill_top(&self) -> f32 {
        self.top_of(self.position(self.active)) + self.shift[self.active].x + self.pill.x
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

/// The hairline lift under the lit row and under a row in hand.
fn lift(t: &Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: t.shadow,
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(2.0),
        spread_radius: px(0.0),
        inset: false,
    }]
}

/// What a drag carries: the entry, by position in the order.
struct NavDrag {
    position: usize,
}

/// The view that follows the pointer during a drag: a copy of the row.
struct Ghost {
    nav: usize,
    t: Theme,
}

impl Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (label, glyph) = NAV[self.nav];
        div()
            .w(px(LIST_W))
            .h(px(ROW_H))
            .flex()
            .items_center()
            .gap(px(11.0))
            .px(px(10.0))
            .rounded(px(8.0))
            .bg(self.t.bg)
            .shadow(lift(&self.t))
            .font_family(SANS)
            .text_size(px(FS_MD))
            .line_height(relative(1.2))
            .text_color(self.t.fg)
            .child(icon(glyph, 17.0, &self.t, self.t.fg))
            .child(SharedString::from(label))
    }
}

fn entry<V: 'static>(
    position: usize,
    state: &Sidebar,
    t: &Theme,
    cx: &mut Context<V>,
    select: fn(&mut V, usize, &mut Window, &mut Context<V>),
) -> impl IntoElement {
    let index = state.order[position];
    let (label, glyph) = NAV[index];
    let active = index == state.active;
    let held = state
        .shared
        .borrow()
        .drag
        .is_some_and(|d| d.from == position);
    let fg = t.fg;
    let colour = if active { t.fg } else { t.mutd };
    let shared = state.shared.clone();
    let ghost_theme = *t;
    let mut row = div()
        .id(("nav", index))
        .group("nav-row")
        .relative()
        .top(px(state.shift[index].x))
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
        .when(held, |d| d.opacity(0.35))
        .hover(move |s| s.text_color(fg))
        .on_click(
            cx.listener(move |view, _: &ClickEvent, window, cx| select(view, index, window, cx)),
        )
        .on_drag(NavDrag { position }, {
            let shared = shared.clone();
            move |drag, _, _, cx| {
                shared.borrow_mut().drag = Some(Drag {
                    from: drag.position,
                    to: drag.position,
                });
                cx.new(|_| Ghost {
                    nav: index,
                    t: ghost_theme,
                })
            }
        })
        .on_mouse_down(MouseButton::Left, {
            let shared = shared.clone();
            move |event, _, _| shared.borrow_mut().press_y = f32::from(event.position.y)
        })
        .on_mouse_down(MouseButton::Right, move |event, window, _| {
            shared.borrow_mut().menu = Some(Menu {
                position,
                at: event.position,
            });
            window.refresh();
        })
        .child(icon_hover(glyph, 17.0, t, colour, Some(("nav-row", t.fg))))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .when(active, |d| d.font_weight(FontWeight::MEDIUM))
                .child(SharedString::from(label)),
        );
    if index == state.default_nav() {
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
    // The wrapper stays on its row when the entry is drawn away from it, so
    // the measured rows are the real ones.
    div().flex_none().child(row)
}

/// The right-click menu: three moves, as in the old sidebar.
fn context_menu(state: &Sidebar, t: &Theme) -> Option<AnyElement> {
    let menu = state.shared.borrow().menu?;
    let at = menu.position;
    let last = state.order.len() - 1;
    let items = [
        ("Set as default screen", at > 0, 0),
        ("Move up", at > 0, at.saturating_sub(1)),
        ("Move down", at < last, (at + 1).min(last)),
    ];
    let close = {
        let shared = state.shared.clone();
        move |window: &mut Window| {
            shared.borrow_mut().menu = None;
            window.refresh();
        }
    };
    let (fg, sel) = (t.fg, t.sel);
    let mut list = div()
        .occlude()
        .w(px(190.0))
        .py(px(4.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(t.line)
        .bg(t.bg)
        .shadow(lift(t))
        .font_family(SANS)
        .text_size(px(FS_SMALL))
        .line_height(relative(1.26))
        .on_mouse_down_out({
            let close = close.clone();
            move |_, window, _| close(window)
        });
    for (i, (label, enabled, to)) in items.into_iter().enumerate() {
        let item = div()
            .id(("nav-menu", i))
            .px(px(12.0))
            .py(px(6.0))
            .text_color(if enabled { t.mutd } else { t.faint });
        let shared = state.shared.clone();
        let close = close.clone();
        list = list.child(if enabled {
            item.cursor_pointer()
                .hover(move |s| s.bg(sel).text_color(fg))
                .on_click(move |_, window, _| {
                    shared.borrow_mut().command = Some((at, to));
                    close(window);
                })
                .child(label)
        } else {
            item.opacity(0.5).child(label)
        });
    }
    Some(
        deferred(
            anchored()
                .position(menu.at)
                .snap_to_window_with_margin(px(8.0))
                .child(list),
        )
        .with_priority(1)
        .into_any_element(),
    )
}

/// Keep the real row tops. Asks for a new frame only when they changed.
fn measure(shared: &RefCell<Shared>, kids: &[Bounds<Pixels>], window: &mut Window) {
    // The children are the ground, then each row, with the divider after the first.
    if kids.len() != NAV.len() + 2 {
        return;
    }
    let rows: Vec<&Bounds<Pixels>> = std::iter::once(&kids[1]).chain(&kids[3..]).collect();
    let base = rows[0].origin.y;
    let tops = rows.iter().map(|b| f32::from(b.origin.y - base)).collect();
    let height = f32::from(rows[0].size.height);
    if shared
        .borrow_mut()
        .store_rows(tops, height, f32::from(base))
    {
        // `refresh` does nothing while the window draws.
        window.request_animation_frame();
    }
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
        .h(px(state.row_height()))
        .rounded(px(8.0))
        .bg(t.bg)
        .shadow(lift(t));

    let measured = state.shared.clone();
    let dragged = state.shared.clone();
    let mut list = div()
        .relative()
        .flex()
        .flex_col()
        .gap(px(GAP))
        .w(px(LIST_W))
        .on_children_prepainted(move |kids, window, _| measure(&measured, &kids, window))
        // The listener runs wherever the pointer is, so a drag out of the sidebar goes on.
        .on_drag_move(move |event: &DragMoveEvent<NavDrag>, window, _| {
            let mut shared = dragged.borrow_mut();
            let Some(from) = shared.drag.map(|d| d.from) else {
                return;
            };
            let to = shared.position_at(f32::from(event.event.position.y), from);
            if let Some(d) = shared.drag.as_mut().filter(|d| d.to != to) {
                d.to = to;
                window.refresh();
            }
        })
        .child(pill);
    for position in 0..NAV.len() {
        list = list.child(entry(position, state, t, cx, select));
        if position == 0 {
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
        .children(context_menu(state, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::IntoElement;

    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rows_rest_where_the_stylesheet_puts_them() {
        assert!((row_top(0)).abs() < 1e-4);
        // Row 1 sits under row 0, the gap, the divider and a second gap.
        assert!((row_top(1) - (ROW_H + 2.0 + 16.0 + 2.0)).abs() < 1e-3);
        assert!((row_top(2) - row_top(1) - (ROW_H + 2.0)).abs() < 1e-3);
    }

    #[test]
    fn selecting_a_row_starts_the_ground_where_it_was_drawn() {
        let mut bar = Sidebar::new(false, &[]);
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
        let mut bar = Sidebar::new(false, &[]);
        bar.select(0);
        assert!(!bar.pill.is_moving());
    }

    #[test]
    fn the_ground_sits_on_the_measured_row() {
        let mut bar = Sidebar::new(true, &[]);
        // A pitch of 37.1 and not the 35.2 of the stylesheet.
        let tops: Vec<f32> = (0..NAV.len()).map(|i| i as f32 * 37.1).collect();
        assert!(bar.shared.borrow_mut().store_rows(tops, 33.4, 100.0));
        bar.select(14);
        assert!((bar.pill_top() - 14.0 * 37.1).abs() < 1e-3);
        assert!((bar.row_height() - 33.4).abs() < 1e-3);
        // The same rows again need no new frame.
        let same: Vec<f32> = (0..NAV.len()).map(|i| i as f32 * 37.1).collect();
        assert!(!bar.shared.borrow_mut().store_rows(same, 33.4, 100.0));
    }

    #[test]
    fn a_saved_order_is_reconciled_with_the_entries() {
        // An unknown and a repeated key drop out. The missing entries go last.
        let order = order_from_keys(&keys(&["sleep", "gone", "sleep", "today"]));
        assert_eq!(&order[..2], &[3, 0]);
        assert_eq!(order.len(), NAV.len());
        assert_eq!(order_from_keys(&[]), (0..NAV.len()).collect::<Vec<_>>());
        let bar = Sidebar::new(false, &keys(&["settings"]));
        assert_eq!(bar.active, NAV.len() - 1);
        assert_eq!(bar.keys()[..2], ["settings", "today"]);
    }

    #[test]
    fn a_move_keeps_the_rows_where_they_are_drawn() {
        let mut bar = Sidebar::new(false, &[]);
        assert!(bar.apply_move(3, 0));
        assert_eq!(bar.order[..2], [3, 0]);
        // The entry that was at row 3 is drawn there until it glides to row 0.
        assert!((bar.shift[3].x - (row_top(3) - row_top(0))).abs() < 1e-3);
        assert!((bar.shift[0].x - (row_top(0) - row_top(1))).abs() < 1e-3);
        assert!(!bar.apply_move(2, 2));
    }

    #[test]
    fn the_pointer_picks_the_position_under_it() {
        let mut shared = Shared::default();
        shared.store_rows(
            (0..NAV.len()).map(|i| i as f32 * 35.0).collect(),
            33.0,
            50.0,
        );
        // The rows are 35 apart. The centre of the held row follows the pointer.
        // A small move, from any point of the row, gives the held row.
        shared.press_y = 75.0;
        assert_eq!(shared.position_at(78.0, 0), 0);
        shared.press_y = 120.0;
        assert_eq!(shared.position_at(126.0, 1), 1);
        assert_eq!(shared.position_at(114.0, 1), 1);
        // Down and up need the same distance: a little more than one pitch.
        shared.press_y = 75.0;
        assert_eq!(shared.position_at(75.0 + 36.0, 0), 1);
        assert_eq!(shared.position_at(75.0 + 71.0, 0), 2);
        assert_eq!(shared.position_at(75.0 + 176.0, 0), 5);
        shared.press_y = 200.0;
        assert_eq!(shared.position_at(200.0 - 36.0, 4), 3);
        assert_eq!(shared.position_at(200.0 - 106.0, 4), 1);
        assert_eq!(shared.position_at(200.0 - 141.0, 4), 0);
        assert_eq!(shared.position_at(5000.0, 0), NAV.len() - 1);
    }

    /// The sidebar alone in a window, as the shell draws it.
    struct Host(Sidebar);

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.0.tick(window, cx.has_active_drag());
            let info = Info {
                who: None,
                synced_ago: None,
                sync_line: None,
                sync_failed: false,
            };
            let t = crate::theme::Palette::Paper.theme();
            render(
                &self.0,
                &info,
                t.dark,
                &t,
                cx,
                Actions {
                    select: |_, _, _, _| {},
                    toggle_theme: |_, _, _| {},
                    sync_now: |_, _, _| {},
                },
            )
        }
    }

    fn frame(cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[gpui::test]
    fn a_tiny_drag_from_the_lower_half_of_a_row_changes_nothing(cx: &mut gpui::TestAppContext) {
        let (host, cx) = cx.add_window_view(|_, _| Host(Sidebar::new(true, &[])));
        // The first frame measures the rows. The next one draws them.
        for _ in 0..3 {
            frame(cx);
        }
        let (y, x) = {
            let shared = host.read_with(cx, |h, _| h.0.shared.clone());
            let shared = shared.borrow();
            assert!(!shared.tops.is_empty(), "rows were not measured");
            (shared.origin_y + shared.tops[0] + shared.row_h * 0.75, 70.0)
        };
        let at = |dy: f32| point(px(x), px(y + dy));
        let none = gpui::Modifiers::default();
        cx.simulate_mouse_down(at(0.0), MouseButton::Left, none);
        cx.simulate_mouse_move(at(1.5), MouseButton::Left, none);
        cx.simulate_mouse_move(at(3.0), MouseButton::Left, none);
        // The drag is active now. This move runs the drag handler.
        cx.simulate_mouse_move(at(3.5), MouseButton::Left, none);
        frame(cx);
        let shared = host.read_with(cx, |h, _| h.0.shared.clone());
        assert!(shared.borrow().drag.is_some(), "the drag did not start");
        cx.simulate_mouse_up(at(3.0), MouseButton::Left, none);
        for _ in 0..3 {
            frame(cx);
        }
        let order = host.read_with(cx, |h, _| h.0.order.clone());
        assert_eq!(order, (0..NAV.len()).collect::<Vec<_>>());
    }

    #[gpui::test]
    fn measure_asks_for_a_frame(cx: &mut gpui::TestAppContext) {
        let (_host, cx) = cx.add_window_view(|_, _| Host(Sidebar::new(true, &[])));
        frame(cx);
        assert!(cx.update(|window, cx| window.simulate_next_frame(cx)) > 0);
    }
}
