//! The widths of the sidebar and the coach panel, and the handles that the
//! user drags to change them.
//!
//! A drag changes the width live. A double click on a handle sets the
//! default width. The widths are saved in `window.json`.

use crate::scale::px;
use gpui::{div, prelude::*, AnyElement, Context, MouseButton};

use crate::shell::Root;
use crate::theme::Theme;

pub const SIDEBAR_DEFAULT: f32 = 224.0;
pub const SIDEBAR_RANGE: (f32, f32) = (180.0, 360.0);
pub const COACH_DEFAULT: f32 = 340.0;
pub const COACH_RANGE: (f32, f32) = (280.0, 560.0);
/// The main column never gets narrower than this. The coach panel hides first.
pub const MIN_MAIN: f32 = 560.0;
/// At or below this window width the sidebar hides.
pub const SIDEBAR_HIDES_AT: f32 = 900.0;
/// The width of the hit area of a handle.
const HANDLE_W: f32 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Sidebar,
    Coach,
}

impl Side {
    fn range(self) -> (f32, f32) {
        match self {
            Side::Sidebar => SIDEBAR_RANGE,
            Side::Coach => COACH_RANGE,
        }
    }

    fn default_width(self) -> f32 {
        match self {
            Side::Sidebar => SIDEBAR_DEFAULT,
            Side::Coach => COACH_DEFAULT,
        }
    }
}

/// A saved width kept inside the limits. A missing value is the default.
pub fn clamp_width(side: Side, saved: Option<f32>) -> f32 {
    let (low, high) = side.range();
    saved
        .filter(|w| w.is_finite())
        .map_or(side.default_width(), |w| w.clamp(low, high))
}

/// A drag in flight: the side, the pointer x and the width at the press.
#[derive(Clone, Copy)]
pub struct Drag {
    side: Side,
    start_x: f32,
    start_w: f32,
}

impl Drag {
    /// The width for a pointer at `x`. The coach panel grows to the left.
    fn width_at(&self, x: f32) -> f32 {
        let delta = match self.side {
            Side::Sidebar => x - self.start_x,
            Side::Coach => self.start_x - x,
        };
        clamp_width(self.side, Some(self.start_w + delta))
    }
}

/// What the window shows at this width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// The sidebar width, or zero when it hides.
    pub sidebar: f32,
    /// The coach panel width, or zero when it hides.
    pub coach: f32,
}

/// What the last frame showed. A part that is hidden needs `HYSTERESIS`
/// more room to come back, so a window edge that jitters at a threshold
/// does not flip the layout each frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub sidebar: bool,
    pub coach: bool,
    /// The block grid has more than one lane.
    pub wide: bool,
}

impl Shown {
    /// The state before the first frame: the plain thresholds apply.
    pub const ALL: Shown = Shown {
        sidebar: true,
        coach: true,
        wide: true,
    };
}

/// The extra room, in design pixels, that a hidden part needs to come back.
pub const HYSTERESIS: f32 = 24.0;

/// True when `value` reaches `limit`. A part that was hidden needs
/// `HYSTERESIS` more.
fn reaches(value: f32, limit: f32, was: bool) -> bool {
    value >= limit + if was { 0.0 } else { HYSTERESIS }
}

/// The widths that fit. The coach panel hides first, when the main column
/// would be narrower than `MIN_MAIN`. Each choice reads only the window
/// width, the pane widths that the user chose, and the choice of the last frame.
pub fn layout(window: f32, sidebar_w: f32, coach_w: f32, coach_open: bool, was: Shown) -> Layout {
    // "Over 900" is "at least 900.5" for a whole number of pixels.
    let sidebar = if reaches(window, SIDEBAR_HIDES_AT + 0.5, was.sidebar)
        && reaches(window - sidebar_w, MIN_MAIN, was.sidebar)
    {
        sidebar_w
    } else {
        0.0
    };
    let coach = if coach_open && reaches(window - sidebar - coach_w, MIN_MAIN, was.coach) {
        coach_w
    } else {
        0.0
    };
    Layout { sidebar, coach }
}

/// True when the block grid has room for more than one lane.
pub fn grid_is_wide(available: f32, narrow_below: f32, was: bool) -> bool {
    reaches(available, narrow_below, was)
}

impl Root {
    pub(crate) fn layout(&self, window_width: f32) -> Layout {
        let was = self.shown.get();
        // The Add blocks panel has a fixed width and shows at once.
        let (coach_w, open) = if self.editing() {
            (crate::blocks::edit::PANEL_W, true)
        } else {
            (self.coach_w, self.coach_open)
        };
        let now = layout(window_width, self.sidebar_w, coach_w, open, was);
        // Asking again in the same frame gives the same answer.
        self.shown.set(Shown {
            sidebar: now.sidebar > 0.0,
            coach: now.coach > 0.0,
            ..was
        });
        now
    }

    /// True when the grid of `available` pixels has more than one lane.
    pub(crate) fn grid_is_wide(&self, available: f32) -> bool {
        let was = self.shown.get();
        let wide = grid_is_wide(available, crate::shell::NARROW_GRID, was.wide);
        self.shown.set(Shown { wide, ..was });
        wide
    }

    fn set_pane_width(&mut self, side: Side, width: f32) {
        match side {
            Side::Sidebar => self.sidebar_w = width,
            Side::Coach => self.coach_w = width,
        }
    }

    fn pane_press(&mut self, side: Side, x: f32, clicks: usize, cx: &mut Context<Self>) {
        let start_w = match side {
            Side::Sidebar => self.sidebar_w,
            Side::Coach => self.coach_w,
        };
        if clicks >= 2 {
            self.pane_drag = None;
            self.set_pane_width(side, side.default_width());
            self.save_window_state();
        } else {
            self.pane_drag = Some(Drag {
                side,
                start_x: x,
                start_w,
            });
        }
        cx.notify();
    }

    /// The pointer moves over the window. `pressed` is true while the left
    /// button is down.
    pub(crate) fn pane_move(&mut self, x: f32, pressed: bool, cx: &mut Context<Self>) {
        let Some(drag) = self.pane_drag else {
            return;
        };
        if !pressed {
            return self.pane_release(cx);
        }
        self.set_pane_width(drag.side, drag.width_at(x));
        cx.notify();
    }

    pub(crate) fn pane_release(&mut self, cx: &mut Context<Self>) {
        if self.pane_drag.take().is_some() {
            self.save_window_state();
            cx.notify();
        }
    }
}

/// The drag handle on the inner edge of a pane: the right edge of the
/// sidebar and the left edge of the coach panel.
pub fn handle(root: &Root, side: Side, t: &Theme, cx: &mut Context<Root>) -> AnyElement {
    let group = match side {
        Side::Sidebar => "edge-sidebar",
        Side::Coach => "edge-coach",
    };
    let dragging = root.pane_drag.is_some_and(|d| d.side == side);
    let acc = t.acc;
    let line = div()
        .h_full()
        .w(px(1.0))
        .when(dragging, |d| d.bg(acc))
        .group_hover(group, move |s| s.bg(acc));
    let edge = div()
        .id(group)
        .group(group)
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(HANDLE_W))
        .flex()
        .cursor_ew_resize()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |root, event: &gpui::MouseDownEvent, _, cx| {
                root.pane_press(
                    side,
                    crate::scale::dp(event.position.x),
                    event.click_count,
                    cx,
                )
            }),
        );
    match side {
        Side::Sidebar => edge.right_0().justify_end().child(line),
        Side::Coach => edge.left_0().child(line),
    }
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_width_stays_inside_the_limits() {
        assert_eq!(clamp_width(Side::Sidebar, None), 224.0);
        assert_eq!(clamp_width(Side::Sidebar, Some(100.0)), 180.0);
        assert_eq!(clamp_width(Side::Sidebar, Some(900.0)), 360.0);
        assert_eq!(clamp_width(Side::Coach, Some(f32::NAN)), 340.0);
        assert_eq!(clamp_width(Side::Coach, Some(300.0)), 300.0);
    }

    #[test]
    fn the_sidebar_grows_right_and_the_coach_panel_grows_left() {
        let sidebar = Drag {
            side: Side::Sidebar,
            start_x: 224.0,
            start_w: 224.0,
        };
        assert_eq!(sidebar.width_at(274.0), 274.0);
        assert_eq!(sidebar.width_at(0.0), 180.0);
        let coach = Drag {
            side: Side::Coach,
            start_x: 1100.0,
            start_w: 340.0,
        };
        assert_eq!(coach.width_at(1050.0), 390.0);
        assert_eq!(coach.width_at(0.0), 560.0);
        assert_eq!(coach.width_at(1400.0), 280.0);
    }

    #[test]
    fn the_coach_panel_hides_before_the_main_column_gets_narrow() {
        let all = Shown::ALL;
        let wide = layout(1440.0, 224.0, 340.0, true, all);
        assert_eq!((wide.sidebar, wide.coach), (224.0, 340.0));
        // 1100 - 224 - 340 = 536: under the minimum, so the panel hides.
        let tight = layout(1100.0, 224.0, 340.0, true, all);
        assert_eq!((tight.sidebar, tight.coach), (224.0, 0.0));
        // A wider sidebar takes room from the coach panel first.
        assert_eq!(layout(1200.0, 360.0, 340.0, true, all).coach, 0.0);
        assert_eq!(layout(1200.0, 224.0, 340.0, false, all).coach, 0.0);
    }

    #[test]
    fn the_sidebar_hides_in_a_small_window() {
        let all = Shown::ALL;
        assert_eq!(layout(900.0, 224.0, 340.0, true, all).sidebar, 0.0);
        assert_eq!(layout(901.0, 224.0, 340.0, true, all).sidebar, 224.0);
        assert_eq!(layout(901.0, 360.0, 340.0, true, all).sidebar, 0.0);
    }

    /// Walk the widths up and down, like a live resize, and count how often
    /// the shown parts change.
    fn flips(from: f32, to: f32, step: f32) -> (usize, Vec<Layout>) {
        let mut was = Shown::ALL;
        let mut seen = Vec::new();
        let mut w = from;
        while (step > 0.0 && w <= to) || (step < 0.0 && w >= to) {
            let l = layout(w, 224.0, 340.0, true, was);
            was = Shown {
                sidebar: l.sidebar > 0.0,
                coach: l.coach > 0.0,
                ..was
            };
            seen.push(l);
            w += step;
        }
        let changes = seen.windows(2).filter(|p| p[0] != p[1]).count();
        (changes, seen)
    }

    #[test]
    fn a_sweep_changes_each_pane_once_and_never_leaves_the_main_column_short() {
        let (up, seen) = flips(700.0, 1600.0, 1.0);
        // Coach in at 924 (no sidebar yet), sidebar in at 925 (coach out),
        // coach in at 1148: three changes in a rising sweep, none repeated.
        assert_eq!(up, 3);
        let (down, _) = flips(1600.0, 700.0, -1.0);
        // Coach out at 1123, sidebar out at 900.
        assert_eq!(down, 2);
        for (i, l) in seen.iter().enumerate() {
            let w = 700.0 + i as f32;
            assert!(w - l.sidebar - l.coach >= MIN_MAIN.min(w), "{w}");
        }
    }

    #[test]
    fn a_jitter_at_a_threshold_does_not_flip_the_panes() {
        // The coach panel fits at 1124. Jitter around it for many frames.
        let mut was = Shown::ALL;
        let mut shown = Vec::new();
        for i in 0..40 {
            let w = 1124.0 + if i % 2 == 0 { -1.0 } else { 1.0 };
            let l = layout(w, 224.0, 340.0, true, was);
            was = Shown {
                sidebar: l.sidebar > 0.0,
                coach: l.coach > 0.0,
                ..was
            };
            shown.push(l.coach > 0.0);
        }
        assert!(shown.iter().all(|s| *s == shown[0]), "{shown:?}");
        // The panel that hid needs 24 px more to return.
        let hidden = Shown {
            coach: false,
            ..Shown::ALL
        };
        assert_eq!(layout(1140.0, 224.0, 340.0, true, hidden).coach, 0.0);
        assert_eq!(layout(1148.0, 224.0, 340.0, true, hidden).coach, 340.0);
        // The same answer when asked again.
        let again = Shown {
            coach: true,
            ..hidden
        };
        assert_eq!(layout(1148.0, 224.0, 340.0, true, again).coach, 340.0);
    }

    #[test]
    fn the_grid_narrows_with_hysteresis() {
        assert!(grid_is_wide(640.0, 640.0, true));
        assert!(!grid_is_wide(639.0, 640.0, true));
        assert!(!grid_is_wide(650.0, 640.0, false));
        assert!(grid_is_wide(664.0, 640.0, false));
    }
}
