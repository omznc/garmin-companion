//! The sidebar: three groups of screens with a count at the right of an
//! entry, and the Settings row at the bottom. From the `nav` of the design.
//!
//! The user orders the entries inside a group by drag or by the right-click
//! menu. The entry at the top is the screen the app opens on. A group header
//! closes and opens its group.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    anchored, deferred, div, point, prelude::*, px, relative, AnyElement, BoxShadow, ClickEvent,
    Context, Div, DragMoveEvent, FontWeight, MouseButton, Pixels, Point, Render, SharedString,
    Stateful, Window,
};

use crate::icons::{icon, icon_hover, Icon};
use crate::kit::{kbd, modifier};
use crate::screen::{Group, Screen};
use crate::scroll::SmoothScroll;
use crate::theme::{Theme, FS_SMALL};
use crate::ui::{MONO, SANS};

/// The width of the sidebar, with its right border.
pub const SIDEBAR_W: f32 = 224.0;
/// The side padding of the sidebar.
const PAD_X: f32 = 6.0;
const PAD_Y: f32 = 8.0;
/// The width of an entry.
const LIST_W: f32 = SIDEBAR_W - 1.0 - 2.0 * PAD_X;
const ROW_H: f32 = 26.0;
const HEAD_H: f32 = 24.0;
/// The air under a group.
const GROUP_GAP: f32 = 6.0;

/// The screens in the order of the saved keys. Each group keeps the relative
/// order of its saved keys. An unknown or repeated key is dropped. A screen
/// that the keys miss goes last in its group, so a new screen never takes
/// over as the default.
fn order_from_keys(keys: &[String]) -> Vec<Screen> {
    let saved: Vec<Screen> = keys.iter().filter_map(|k| Screen::from_key(k)).collect();
    let mut order = Vec::with_capacity(Screen::ALL.len());
    for group in Group::ALL {
        let start = order.len();
        let in_group = |s: &Screen| s.group() == group;
        for screen in saved.iter().copied().filter(in_group) {
            if !order[start..].contains(&screen) {
                order.push(screen);
            }
        }
        for screen in Screen::ALL.into_iter().filter(in_group) {
            if !order[start..].contains(&screen) {
                order.push(screen);
            }
        }
    }
    order
}

/// The order with the entry at `from` put at `to`.
fn moved(order: &[Screen], from: usize, to: usize) -> Vec<Screen> {
    let mut next = order.to_vec();
    let entry = next.remove(from);
    next.insert(to, entry);
    next
}

/// Where each row rests, from the top of the list. A group has a header, the
/// rows of an open group, and the air under it. The rows of a closed group
/// are not drawn, so they rest at the header.
fn row_tops(order: &[Screen], closed: &[Group]) -> Vec<f32> {
    let mut y = 0.0;
    let mut tops = Vec::with_capacity(order.len());
    let mut last = None;
    for screen in order {
        let group = screen.group();
        if last != Some(group) {
            if last.is_some() {
                y += GROUP_GAP;
            }
            y += HEAD_H;
            last = Some(group);
        }
        tops.push(y);
        if !closed.contains(&group) {
            y += ROW_H;
        }
    }
    tops
}

/// A drag in progress, by position in the order.
#[derive(Clone, Copy)]
struct Drag {
    from: usize,
    to: usize,
    /// The first and the last position of the group of the held row.
    span: (usize, usize),
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
    /// Where each row rests, from the top of the list.
    tops: Vec<f32>,
    drag: Option<Drag>,
    /// Where the left button went down, in window height.
    press_y: f32,
    menu: Option<Menu>,
    /// A move to apply: the position it starts at, and where it ends.
    command: Option<(usize, usize)>,
}

impl Shared {
    /// The position that the dragged row at `from` takes with the pointer at
    /// this window height. The centre of the held row moves with the pointer
    /// from the press. It is compared with the centres of the other rows, as
    /// in the old app. A press low in a row does not change the result.
    fn position_at(&self, y: f32, from: usize) -> usize {
        let held = self.tops[from] + ROW_H / 2.0 + (y - self.press_y);
        let passed = self
            .tops
            .iter()
            .enumerate()
            .filter(|(i, top)| *i != from && **top + ROW_H / 2.0 < held);
        passed.count().min(self.tops.len() - 1)
    }
}

pub struct Sidebar {
    /// The lit entry.
    pub active: Screen,
    /// The entries in the user's order. A group stays whole.
    order: Vec<Screen>,
    /// The groups that the user closed.
    closed: Vec<Group>,
    shared: Rc<RefCell<Shared>>,
    /// How far each entry is drawn from its row, by `Screen::index`. It is
    /// non-zero only during a drag.
    shift: Vec<f32>,
    /// The list scrolls when the window is too short for every entry.
    scroll: SmoothScroll,
}

impl Sidebar {
    /// `keys` is the saved order and `closed` the keys of the closed groups.
    /// The app starts on the first entry of the order.
    pub fn new(reduced_motion: bool, keys: &[String], closed: &[String]) -> Self {
        let order = order_from_keys(keys);
        let closed: Vec<Group> = Group::ALL
            .into_iter()
            .filter(|g| closed.contains(&g.key()))
            .collect();
        let shared = Shared {
            tops: row_tops(&order, &closed),
            ..Shared::default()
        };
        Self {
            active: order[0],
            order,
            closed,
            shared: Rc::new(RefCell::new(shared)),
            shift: vec![0.0; Screen::ALL.len()],
            scroll: SmoothScroll::new(reduced_motion),
        }
    }

    /// The keys to save, in the order of the list.
    pub fn keys(&self) -> Vec<String> {
        self.order.iter().map(|s| s.key()).collect()
    }

    /// The keys of the closed groups, for the window state.
    pub fn closed_keys(&self) -> Vec<String> {
        self.closed.iter().map(|g| g.key()).collect()
    }

    pub fn is_closed(&self, group: Group) -> bool {
        self.closed.contains(&group)
    }

    /// Close an open group and open a closed one.
    pub fn toggle(&mut self, group: Group) {
        match self.closed.iter().position(|g| *g == group) {
            Some(at) => {
                self.closed.remove(at);
            }
            None => self.closed.push(group),
        }
        self.shared.borrow_mut().tops = row_tops(&self.order, &self.closed);
    }

    fn position(&self, screen: Screen) -> usize {
        self.order.iter().position(|&s| s == screen).unwrap_or(0)
    }

    /// The first and the last position of the group of the row at `position`.
    fn span(&self, position: usize) -> (usize, usize) {
        let group = self.order[position].group();
        let first = self.order.iter().position(|s| s.group() == group);
        let last = self.order.iter().rposition(|s| s.group() == group);
        (first.unwrap_or(position), last.unwrap_or(position))
    }

    /// Where a row rests, from the top of the list.
    fn top_of(&self, position: usize) -> f32 {
        self.shared
            .borrow()
            .tops
            .get(position)
            .copied()
            .unwrap_or(0.0)
    }

    /// Light another entry.
    pub fn select(&mut self, screen: Screen) {
        self.active = screen;
    }

    /// Put the entry at position `from` at position `to`. The rows keep the
    /// place they are drawn at, and move to the new place at once. True when the order changed.
    fn apply_move(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.order.len() || to >= self.order.len() {
            return false;
        }
        // A row never leaves its group.
        if self.order[from].group() != self.order[to].group() {
            return false;
        }
        let next = moved(&self.order, from, to);
        self.shift.fill(0.0);
        self.order = next;
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
        for (new, &screen) in moved(&self.order, d.from, d.to).iter().enumerate() {
            self.shift[screen.index()] = self.top_of(new) - self.top_of(self.position(screen));
        }
    }

    /// Advance the springs. Ask for another frame while one moves.
    /// `dragging` is true while a drag is in flight. True when the order changed.
    pub fn tick(&mut self, window: &mut Window, dragging: bool) -> bool {
        self.scroll.tick(window);
        let changed = self.take_gestures(dragging);
        self.aim_rows();
        changed
    }
}

/// The hairline lift under a row in hand and under the menu.
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
    screen: Screen,
    t: Theme,
}

impl Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(LIST_W))
            .h(px(ROW_H))
            .flex()
            .items_center()
            .gap(px(8.0))
            .pl(px(22.0))
            .pr(px(8.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(self.t.line)
            .bg(self.t.bg2)
            .shadow(lift(&self.t))
            .font_family(SANS)
            .text_size(px(13.0))
            .line_height(relative(1.2))
            .text_color(self.t.fg)
            .child(icon(self.screen.icon(), 14.0, &self.t, self.t.fg))
            .child(SharedString::from(self.screen.label()))
    }
}

/// The `.nav` button: a 26 px row with an icon, a name and an optional right part.
fn nav_row(
    id: impl Into<gpui::ElementId>,
    screen: Screen,
    active: bool,
    pad_left: f32,
    t: &Theme,
) -> Stateful<Div> {
    let colour = if active { t.fg } else { t.mutd };
    let (hover, fg) = (t.hover, t.fg);
    div()
        .id(id)
        .group("nav-row")
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(ROW_H))
        .pl(px(pad_left))
        .pr(px(8.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(if active {
            t.line
        } else {
            gpui::transparent_black()
        })
        .when(active, |d| d.bg(t.active))
        .when(!active, |d| d.hover(move |s| s.bg(hover).text_color(fg)))
        .cursor_pointer()
        .font_family(SANS)
        .text_size(px(13.0))
        .line_height(relative(1.2))
        .text_color(colour)
        .child(icon_hover(
            screen.icon(),
            14.0,
            t,
            colour,
            Some(("nav-row", t.fg)),
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(SharedString::from(screen.label())),
        )
}

fn count_text(count: Option<&str>, t: &Theme) -> Option<Div> {
    count.map(|text| {
        div()
            .flex_none()
            .font_family(MONO)
            .text_size(px(11.0))
            .text_color(t.faint)
            .child(text.to_string())
    })
}

fn entry<V: 'static>(
    position: usize,
    state: &Sidebar,
    count: Option<&str>,
    t: &Theme,
    cx: &mut Context<V>,
    select: fn(&mut V, Screen, &mut Window, &mut Context<V>),
) -> impl IntoElement {
    let screen = state.order[position];
    let held = state
        .shared
        .borrow()
        .drag
        .is_some_and(|d| d.from == position);
    let shared = state.shared.clone();
    let span = state.span(position);
    let ghost_theme = *t;
    let row = nav_row(
        ("nav", screen.index()),
        screen,
        screen == state.active,
        22.0,
        t,
    )
    .relative()
    .top(px(state.shift[screen.index()]))
    .flex_none()
    .when(held, |d| d.opacity(0.35))
    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| select(view, screen, window, cx)))
    .on_drag(NavDrag { position }, {
        let shared = shared.clone();
        move |drag, _, _, cx| {
            shared.borrow_mut().drag = Some(Drag {
                from: drag.position,
                to: drag.position,
                span,
            });
            cx.new(|_| Ghost {
                screen,
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
    .children(count_text(count, t));
    // The wrapper stays on its row when the entry is drawn away from it.
    div().flex_none().child(row)
}

/// The header of a group: a chevron and the name. A click closes or opens the group.
fn group_header<V: 'static>(
    group: Group,
    closed: bool,
    t: &Theme,
    cx: &mut Context<V>,
    toggle: fn(&mut V, Group, &mut Window, &mut Context<V>),
) -> impl IntoElement {
    let (mutd, faint) = (t.mutd, t.faint);
    div()
        .id(("group", group as usize))
        .group("group-head")
        .flex()
        .items_center()
        .gap(px(6.0))
        .h(px(HEAD_H))
        .px(px(8.0))
        .cursor_pointer()
        .font_family(SANS)
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .letter_spacing(px(11.5 * 0.02))
        .text_color(faint)
        .hover(move |s| s.text_color(mutd))
        .on_click(
            cx.listener(move |view, _: &ClickEvent, window, cx| toggle(view, group, window, cx)),
        )
        .child(icon_hover(
            if closed {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            },
            10.0,
            t,
            faint,
            Some(("group-head", mutd)),
        ))
        .child(group.label())
}

/// The right-click menu: three moves, as in the old sidebar.
fn context_menu(state: &Sidebar, t: &Theme) -> Option<AnyElement> {
    let menu = state.shared.borrow().menu?;
    let at = menu.position;
    let (first, last) = state.span(at);
    let items = [
        ("Set as default screen", at > 0 && first == 0, 0),
        ("Move up", at > first, at.saturating_sub(1)),
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

/// What the sidebar does when the user clicks. Each one is a function of the view.
pub struct Actions<V> {
    pub select: fn(&mut V, Screen, &mut Window, &mut Context<V>),
    pub toggle_group: fn(&mut V, Group, &mut Window, &mut Context<V>),
}

/// One group: the header, and the rows when the group is open.
fn group_block<V: 'static>(
    group: Group,
    state: &Sidebar,
    counts: &[Option<String>],
    t: &Theme,
    cx: &mut Context<V>,
    actions: &Actions<V>,
) -> Div {
    let closed = state.is_closed(group);
    let mut block = div().flex_none().mb(px(GROUP_GAP)).child(group_header(
        group,
        closed,
        t,
        cx,
        actions.toggle_group,
    ));
    if closed {
        return block;
    }
    for position in 0..state.order.len() {
        let screen = state.order[position];
        if screen.group() == group {
            let count = counts.get(screen.index()).and_then(|c| c.as_deref());
            block = block.child(entry(position, state, count, t, cx, actions.select));
        }
    }
    block
}

/// The Settings row, under a line at the bottom.
fn settings_row<V: 'static>(
    state: &Sidebar,
    t: &Theme,
    cx: &mut Context<V>,
    select: fn(&mut V, Screen, &mut Window, &mut Context<V>),
) -> impl IntoElement {
    let screen = Screen::Settings;
    let row = nav_row("nav-settings", screen, state.active == screen, 8.0, t)
        .on_click(
            cx.listener(move |view, _: &ClickEvent, window, cx| select(view, screen, window, cx)),
        )
        .child(kbd(format!("{} ,", modifier()), t));
    div()
        .flex_none()
        .mt(px(6.0))
        .pt(px(6.0))
        .border_t_1()
        .border_color(t.line)
        .child(row)
}

pub fn render<V: 'static>(
    state: &Sidebar,
    counts: &[Option<String>],
    t: &Theme,
    cx: &mut Context<V>,
    actions: Actions<V>,
) -> impl IntoElement {
    let dragged = state.shared.clone();
    let mut list = div()
        .flex()
        .flex_col()
        // The listener runs wherever the pointer is, so a drag out of the sidebar goes on.
        .on_drag_move(move |event: &DragMoveEvent<NavDrag>, window, _| {
            let mut shared = dragged.borrow_mut();
            let Some(from) = shared.drag.map(|d| d.from) else {
                return;
            };
            let (first, last) = shared.drag.map_or((0, 0), |d| d.span);
            let to = shared
                .position_at(f32::from(event.event.position.y), from)
                .clamp(first, last);
            if let Some(d) = shared.drag.as_mut().filter(|d| d.to != to) {
                d.to = to;
                window.refresh();
            }
        });
    for group in [Group::Daily, Group::Training, Group::Coach] {
        list = list.child(group_block(group, state, counts, t, cx, &actions));
    }

    div()
        .flex_none()
        .w(px(SIDEBAR_W))
        .h_full()
        .flex()
        .flex_col()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .bg(t.bg2)
        .border_r_1()
        .border_color(t.line)
        .child(state.scroll.area("nav").flex_1().min_h_0().child(list))
        .child(settings_row(state, t, cx, actions.select))
        .children(context_menu(state, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::IntoElement;

    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn bar() -> Sidebar {
        Sidebar::new(false, &[], &[])
    }

    #[test]
    fn rows_rest_under_the_header_of_their_group() {
        let tops = row_tops(&Screen::ALL, &[]);
        assert_eq!(tops[0], HEAD_H);
        assert_eq!(tops[1] - tops[0], ROW_H);
        // The next group starts after the last row, the air and its own header.
        let last_daily = tops[Screen::Weight.index()];
        assert_eq!(
            tops[Screen::Activities.index()],
            last_daily + ROW_H + GROUP_GAP + HEAD_H
        );
    }

    #[test]
    fn a_closed_group_takes_only_its_header() {
        let open = row_tops(&Screen::ALL, &[]);
        let shut = row_tops(&Screen::ALL, &[Group::Daily]);
        assert_eq!(
            shut[Screen::Activities.index()],
            HEAD_H + GROUP_GAP + HEAD_H
        );
        assert!(shut[Screen::Activities.index()] < open[Screen::Activities.index()]);
        let mut bar = bar();
        bar.toggle(Group::Daily);
        assert_eq!(bar.closed_keys(), ["daily"]);
        assert_eq!(
            bar.top_of(Screen::Activities.index()),
            shut[Screen::Activities.index()]
        );
        bar.toggle(Group::Daily);
        assert!(bar.closed_keys().is_empty());
    }

    #[test]
    fn a_saved_order_is_reconciled_with_the_entries() {
        // An unknown and a repeated key drop out. The missing entries go last.
        let order = order_from_keys(&keys(&["sleep", "gone", "sleep", "today"]));
        assert_eq!(&order[..2], &[Screen::Sleep, Screen::Today]);
        assert_eq!(order.len(), Screen::ALL.len());
        assert_eq!(order_from_keys(&[]), Screen::ALL.to_vec());
        // The keys of one group never move a screen into another group.
        let bar = Sidebar::new(false, &keys(&["settings", "gear", "sleep", "reports"]), &[]);
        assert_eq!(bar.active, Screen::Sleep);
        assert_eq!(bar.keys()[..3], ["sleep", "today", "health"]);
        assert_eq!(bar.order[5], Screen::Gear);
        assert_eq!(bar.order[11], Screen::Reports);
        assert_eq!(bar.order[14], Screen::Settings);
    }

    #[test]
    fn a_move_puts_the_rows_in_place_at_once() {
        let mut bar = bar();
        assert!(bar.apply_move(3, 0));
        assert_eq!(bar.order[..2], [Screen::Food, Screen::Today]);
        // The rows stand in their new places at once.
        assert!(bar.shift.iter().all(|&x| x == 0.0));
        assert!(!bar.apply_move(2, 2));
        // A move across a group does nothing.
        assert!(!bar.apply_move(0, 5));
    }

    #[test]
    fn the_pointer_picks_the_position_under_it() {
        let mut shared = Shared {
            tops: (0..Screen::ALL.len()).map(|i| i as f32 * ROW_H).collect(),
            ..Shared::default()
        };
        // The centre of the held row follows the pointer. A small move, from
        // any point of the row, gives the held row.
        shared.press_y = 75.0;
        assert_eq!(shared.position_at(78.0, 0), 0);
        shared.press_y = 120.0;
        assert_eq!(shared.position_at(126.0, 1), 1);
        assert_eq!(shared.position_at(114.0, 1), 1);
        // Down and up need the same distance: a little more than one pitch.
        shared.press_y = 75.0;
        assert_eq!(shared.position_at(75.0 + ROW_H + 1.0, 0), 1);
        assert_eq!(shared.position_at(75.0 + 2.0 * ROW_H + 1.0, 0), 2);
        shared.press_y = 200.0;
        assert_eq!(shared.position_at(200.0 - ROW_H - 1.0, 4), 3);
        assert_eq!(shared.position_at(5000.0, 0), Screen::ALL.len() - 1);
    }

    /// The sidebar alone in a window, as the shell draws it.
    struct Host(Sidebar);

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.0.tick(window, cx.has_active_drag());
            let t = crate::theme::Palette::Paper.theme();
            render(
                &self.0,
                &[],
                &t,
                cx,
                Actions {
                    select: |_, _, _, _| {},
                    toggle_group: |_, _, _, _| {},
                },
            )
        }
    }

    fn frame(cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[gpui::test]
    fn a_tiny_drag_from_the_lower_half_of_a_row_changes_nothing(cx: &mut gpui::TestAppContext) {
        let (host, cx) = cx.add_window_view(|_, _| Host(Sidebar::new(true, &[], &[])));
        for _ in 0..3 {
            frame(cx);
        }
        // The list starts under the top padding. The first row is under its header.
        let (y, x) = (PAD_Y + HEAD_H + ROW_H * 0.75, 70.0);
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
        assert_eq!(order, Screen::ALL.to_vec());
    }
}
