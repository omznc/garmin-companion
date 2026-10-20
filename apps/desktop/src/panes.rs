//! The widths of the sidebar and the coach panel, and the handles that the
//! user drags to change them.
//!
//! A drag changes the width live. A double click on a handle sets the
//! default width. The widths are saved in `window.json`.

use gpui::{div, prelude::*, px, AnyElement, Context, MouseButton};

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

/// The widths that fit. The coach panel hides first, when the main column
/// would be narrower than `MIN_MAIN`.
pub fn layout(window: f32, sidebar_w: f32, coach_w: f32, coach_open: bool) -> Layout {
    let sidebar = if window <= SIDEBAR_HIDES_AT || window - sidebar_w < MIN_MAIN {
        0.0
    } else {
        sidebar_w
    };
    let coach = if coach_open && window - sidebar - coach_w >= MIN_MAIN {
        coach_w
    } else {
        0.0
    };
    Layout { sidebar, coach }
}

impl Root {
    pub(crate) fn layout(&self, window_width: f32) -> Layout {
        layout(window_width, self.sidebar_w, self.coach_w, self.coach_open)
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
                root.pane_press(side, f32::from(event.position.x), event.click_count, cx)
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
        let wide = layout(1440.0, 224.0, 340.0, true);
        assert_eq!((wide.sidebar, wide.coach), (224.0, 340.0));
        // 1100 - 224 - 340 = 536: under the minimum, so the panel hides.
        let tight = layout(1100.0, 224.0, 340.0, true);
        assert_eq!((tight.sidebar, tight.coach), (224.0, 0.0));
        // A wider sidebar takes room from the coach panel first.
        assert_eq!(layout(1200.0, 360.0, 340.0, true).coach, 0.0);
        assert_eq!(layout(1200.0, 224.0, 340.0, false).coach, 0.0);
    }

    #[test]
    fn the_sidebar_hides_in_a_small_window() {
        assert_eq!(layout(900.0, 224.0, 340.0, true).sidebar, 0.0);
        assert_eq!(layout(901.0, 224.0, 340.0, true).sidebar, 224.0);
        assert_eq!(layout(901.0, 360.0, 340.0, true).sidebar, 0.0);
    }
}
