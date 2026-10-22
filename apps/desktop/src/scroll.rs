//! A scroll container with smooth wheel notches.
//!
//! A mouse wheel sends whole notches, so the page would jump. The container
//! keeps a target offset and moves the real offset toward it with a critically
//! damped spring. A new notch moves the target, so fast notches add up and the
//! motion stays continuous.
//!
//! Only a whole notch is smoothed: a line delta that is a multiple of
//! `LINES_PER_NOTCH`. Everything else passes through to gpui untouched. That is
//! a touchpad on Wayland (pixel deltas, with kinetic momentum where gpui makes
//! it), a touchpad on X11 (gpui sends it as fractional lines), and a wheel with
//! fine steps. Such a wheel scrolls without easing. An X11 touchpad delta that is
//! exactly a whole notch would be eased, which is rare.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    canvas, div, point, prelude::*, px, DispatchPhase, Div, ElementId, HitboxBehavior, ScrollDelta,
    ScrollHandle, ScrollWheelEvent, Stateful, Window,
};

use crate::spring::{Spring, SpringConfig};

/// How far one wheel notch scrolls. Chromium on Linux scrolls about this far.
const NOTCH_PX: f32 = 80.0;

/// gpui sends 3 lines for one notch on Linux and, by default, on Windows
/// (`SCROLL_LINES` in `gpui_ce_linux`). macOS sends about 1 line, so a notch is
/// shorter there and is not smoothed: it is not a multiple of this.
const LINES_PER_NOTCH: f32 = 3.0;
const LINE_PX: f32 = NOTCH_PX / LINES_PER_NOTCH;

/// True for a delta of one or more whole notches.
fn is_notch(lines: f32) -> bool {
    lines != 0.0 && (lines.abs() / LINES_PER_NOTCH).fract() < 1e-3
}

/// Critically damped. About 230 ms from a notch to rest.
const WHEEL: SpringConfig = SpringConfig {
    damping: 1.0,
    response: 0.22,
};

/// The offset is zero at the top and negative below it, as in gpui.
struct Inner {
    spring: Spring,
    last_tick: Option<Instant>,
    reduced: bool,
}

impl Inner {
    fn new(reduced: bool) -> Self {
        Self {
            spring: Spring::new(WHEEL),
            last_tick: None,
            reduced,
        }
    }

    /// The offset can change outside the spring: a touchpad delta, or a resize
    /// that clamps it. Follow it, and drop the animation.
    fn follow(&mut self, offset: f32) {
        if (offset - self.spring.x).abs() > 0.5 {
            self.spring.set(offset);
        }
    }

    /// Add a notch to the target. `max` is the scroll range, as a positive number.
    fn notch(&mut self, lines: f32, max: f32) {
        let target = (self.spring.target + lines * LINE_PX).clamp(-max.max(0.0), 0.0);
        if self.reduced {
            self.spring.set(target);
        } else {
            self.spring.to(target);
        }
    }
}

#[derive(Clone)]
pub struct SmoothScroll {
    handle: ScrollHandle,
    inner: Rc<RefCell<Inner>>,
}

impl SmoothScroll {
    pub fn new(reduced: bool) -> Self {
        Self {
            handle: ScrollHandle::new(),
            inner: Rc::new(RefCell::new(Inner::new(reduced))),
        }
    }

    /// Move the offset one frame. Call it in `render`, before the container.
    pub fn tick(&self, window: &mut Window) {
        let now = Instant::now();
        let last = self.inner.borrow_mut().last_tick.replace(now);
        let dt = last.map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32());
        if self.step(dt) {
            window.request_animation_frame();
        } else {
            self.inner.borrow_mut().last_tick = None;
        }
    }

    /// How far the view is above the bottom edge, in pixels. As of the last layout.
    pub fn distance_below(&self) -> f32 {
        f32::from(self.handle.max_offset().y) + f32::from(self.handle.offset().y)
    }

    /// The offset in pixels: zero at the top and negative below it.
    pub fn offset_y(&self) -> f32 {
        f32::from(self.handle.offset().y)
    }

    /// Scroll to `y` pixels below the top. The offset stops at the bottom edge.
    pub fn scroll_to(&self, y: f32) {
        let mut inner = self.inner.borrow_mut();
        let max = f32::from(self.handle.max_offset().y);
        let target = -y.clamp(0.0, max.max(0.0));
        if inner.reduced {
            inner.spring.set(target);
            let x = self.handle.offset().x;
            self.handle.set_offset(point(x, px(target)));
        } else {
            inner.spring.to(target);
        }
    }

    /// Jump to the bottom edge, for a thread that follows new text.
    pub fn pin_bottom(&self) {
        let offset = self.handle.offset();
        let bottom = -f32::from(self.handle.max_offset().y);
        self.handle.set_offset(point(offset.x, px(bottom)));
    }

    /// Move the offset by `dt` seconds. True while it still moves.
    fn step(&self, dt: f32) -> bool {
        let mut inner = self.inner.borrow_mut();
        let offset = self.handle.offset();
        inner.follow(f32::from(offset.y));
        if !inner.spring.is_moving() {
            return false;
        }
        let moving = inner.spring.step(dt);
        self.handle.set_offset(point(offset.x, px(inner.spring.x)));
        moving
    }

    /// The scrolling column. The caller adds the style and the children.
    pub fn area(&self, id: impl Into<ElementId>) -> Stateful<Div> {
        let (handle, inner) = (self.handle.clone(), self.inner.clone());
        // The canvas is a child, so its listener runs before the wheel handler
        // of the column. It takes the line deltas and stops them there. The
        // hitbox keeps a menu or an inner scroller in front of the column from
        // losing its wheel events to the page below.
        let wheel = canvas(
            {
                let handle = handle.clone();
                move |_, window, _| window.insert_hitbox(handle.bounds(), HitboxBehavior::Normal)
            },
            move |_, hitbox, window, _| {
                let (handle, inner) = (handle.clone(), inner.clone());
                let view = window.current_view();
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                    let ScrollDelta::Lines(lines) = event.delta else {
                        return;
                    };
                    if phase != DispatchPhase::Bubble
                        || !is_notch(lines.y)
                        || lines.x != 0.0
                        || !hitbox.should_handle_scroll(window)
                    {
                        return;
                    }
                    let mut inner = inner.borrow_mut();
                    let offset = handle.offset();
                    inner.follow(f32::from(offset.y));
                    inner.notch(lines.y, f32::from(handle.max_offset().y));
                    if inner.reduced {
                        handle.set_offset(point(offset.x, px(inner.spring.x)));
                    }
                    cx.stop_propagation();
                    cx.notify(view);
                });
            },
        )
        .absolute()
        .size_0();
        div()
            .id(id)
            .overflow_y_scroll()
            .track_scroll(&self.handle)
            .child(wheel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, IntoElement, Render, TestAppContext, TouchPhase, VisualTestContext};

    const FRAME: f32 = 1.0 / 60.0;

    #[test]
    fn a_notch_settles_in_about_a_quarter_second_with_no_overshoot() {
        let mut i = Inner::new(false);
        i.notch(-3.0, 1000.0);
        let mut lowest = 0.0_f32;
        let mut frames = 0;
        let mut within_a_pixel = None;
        while i.spring.step(FRAME) {
            lowest = lowest.min(i.spring.x);
            frames += 1;
            assert!(frames < 600, "never settles");
            if within_a_pixel.is_none() && (i.spring.x + NOTCH_PX).abs() < 1.0 {
                within_a_pixel = Some(frames);
            }
        }
        // The last pixel takes the tail. The eye sees the motion end before it.
        let seen = within_a_pixel.unwrap();
        assert!((10..=22).contains(&seen), "{seen} frames");
        assert_eq!(i.spring.x, -NOTCH_PX);
        assert!(lowest >= -NOTCH_PX, "overshot to {lowest}");
    }

    #[test]
    fn a_notch_in_the_middle_of_the_motion_extends_the_target() {
        let mut i = Inner::new(false);
        i.notch(-3.0, 1000.0);
        for _ in 0..5 {
            i.spring.step(FRAME);
        }
        let mid = i.spring.x;
        i.notch(-3.0, 1000.0);
        assert_eq!(i.spring.target, -2.0 * NOTCH_PX);
        assert_eq!(i.spring.x, mid, "the motion restarted");
        while i.spring.step(FRAME) {}
        assert_eq!(i.spring.x, -2.0 * NOTCH_PX);
    }

    #[test]
    fn the_target_stops_at_the_top_and_the_bottom() {
        let mut i = Inner::new(false);
        i.notch(3.0, 300.0);
        assert_eq!(i.spring.target, 0.0);
        for _ in 0..10 {
            i.notch(-3.0, 300.0);
        }
        assert_eq!(i.spring.target, -300.0);
        // A page shorter than the window has nothing to scroll.
        let mut short = Inner::new(false);
        short.notch(-3.0, 0.0);
        assert_eq!(short.spring.target, 0.0);
    }

    #[test]
    fn only_whole_notches_are_smoothed() {
        assert!(is_notch(3.0) && is_notch(-3.0) && is_notch(-6.0));
        assert!(!is_notch(0.0) && !is_notch(0.4) && !is_notch(-1.5) && !is_notch(4.0));
    }

    #[test]
    fn reduced_motion_jumps() {
        let mut i = Inner::new(true);
        i.notch(-6.0, 1000.0);
        assert_eq!(i.spring.x, -2.0 * NOTCH_PX);
        assert!(!i.spring.is_moving());
    }

    #[test]
    fn an_offset_changed_from_outside_cancels_the_animation() {
        let mut i = Inner::new(false);
        i.notch(-3.0, 1000.0);
        i.spring.step(FRAME);
        i.follow(-500.0);
        assert_eq!(i.spring.x, -500.0);
        assert!(!i.spring.is_moving());
    }

    /// A page taller than its window, drawn headless with a fixed frame time.
    struct Page(SmoothScroll);

    impl Render for Page {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0.step(FRAME);
            self.0
                .area("page")
                .w(px(300.0))
                .h(px(200.0))
                .child(div().h(px(2000.0)))
        }
    }

    fn wheel(delta: ScrollDelta) -> ScrollWheelEvent {
        ScrollWheelEvent {
            position: point(px(50.0), px(50.0)),
            delta,
            modifiers: Default::default(),
            touch_phase: TouchPhase::Moved,
        }
    }

    fn frame(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[gpui::test]
    fn notches_and_pixels_move_a_real_layout(cx: &mut TestAppContext) {
        let scroll = SmoothScroll::new(false);
        let (_, cx) = cx.add_window_view({
            let scroll = scroll.clone();
            move |_, _| Page(scroll)
        });
        frame(cx);
        let y = |s: &SmoothScroll| f32::from(s.handle.offset().y);
        assert_eq!(f32::from(scroll.handle.max_offset().y), 1800.0);

        // One notch: the offset is not there on the first frame, and it arrives.
        cx.simulate_event(wheel(ScrollDelta::Lines(point(0.0, -3.0))));
        frame(cx);
        let first = y(&scroll);
        assert!(
            first < 0.0 && first > -NOTCH_PX / 2.0,
            "first frame {first}"
        );
        for _ in 0..40 {
            frame(cx);
        }
        assert_eq!(y(&scroll), -NOTCH_PX);

        // Fast notches add up, and the page does not jump back.
        let mut last = y(&scroll);
        for _ in 0..4 {
            cx.simulate_event(wheel(ScrollDelta::Lines(point(0.0, -3.0))));
            frame(cx);
            assert!(y(&scroll) <= last, "went back");
            last = y(&scroll);
        }
        for _ in 0..60 {
            frame(cx);
        }
        assert_eq!(y(&scroll), -5.0 * NOTCH_PX);

        // A touchpad delta goes through at once, with no easing of ours.
        cx.simulate_event(wheel(ScrollDelta::Pixels(point(px(0.0), px(-30.0)))));
        frame(cx);
        assert_eq!(y(&scroll), -5.0 * NOTCH_PX - 30.0);
        for _ in 0..10 {
            frame(cx);
        }
        assert_eq!(y(&scroll), -5.0 * NOTCH_PX - 30.0);

        // The bottom and the top stop the target.
        for _ in 0..40 {
            cx.simulate_event(wheel(ScrollDelta::Lines(point(0.0, -3.0))));
        }
        for _ in 0..120 {
            frame(cx);
        }
        assert_eq!(y(&scroll), -1800.0);
        for _ in 0..80 {
            cx.simulate_event(wheel(ScrollDelta::Lines(point(0.0, 3.0))));
        }
        for _ in 0..120 {
            frame(cx);
        }
        assert_eq!(y(&scroll), 0.0);
    }

    #[gpui::test]
    fn reduced_motion_jumps_in_a_real_layout(cx: &mut TestAppContext) {
        let scroll = SmoothScroll::new(true);
        let (_, cx) = cx.add_window_view({
            let scroll = scroll.clone();
            move |_, _| Page(scroll)
        });
        frame(cx);
        cx.simulate_event(wheel(ScrollDelta::Lines(point(0.0, -6.0))));
        assert_eq!(f32::from(scroll.handle.offset().y), -2.0 * NOTCH_PX);
    }
}
