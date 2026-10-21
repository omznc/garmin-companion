//! The interface scale. The app draws in design pixels. `px` turns a design
//! pixel into a real pixel with the scale, so text, spacing, icons and charts
//! grow together. `dp` does the reverse for a size that the window measured.

use std::sync::atomic::{AtomicU32, Ordering};

use gpui::{Pixels, Window};

/// The choices of the Settings control, in percent.
pub const STEPS: [u32; 5] = [90, 100, 110, 125, 150];

static SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// The scale as a factor: 1.0 is 100 percent.
pub fn get() -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed))
}

/// Set the factor. A value outside the known steps is cut to the nearest end.
pub fn set(factor: f32) {
    let low = STEPS[0] as f32 / 100.0;
    let high = STEPS[STEPS.len() - 1] as f32 / 100.0;
    let factor = if factor.is_finite() { factor } else { 1.0 };
    SCALE.store(factor.clamp(low, high).to_bits(), Ordering::Relaxed);
}

/// The step above or below `now` (a factor). `None` at the end.
fn next_step(now: f32, up: bool) -> Option<f32> {
    let now = (now * 100.0).round() as u32;
    let next = if up {
        STEPS.iter().find(|&&s| s > now)
    } else {
        STEPS.iter().rev().find(|&&s| s < now)
    };
    next.map(|&s| s as f32 / 100.0)
}

/// The next step up or down from the current one. `None` at the end.
pub fn step(up: bool) -> Option<f32> {
    next_step(get(), up)
}

fn scaled(design: f32, factor: f32) -> f32 {
    if design.abs() <= 1.0 {
        design
    } else {
        design * factor
    }
}

/// A design pixel as a real pixel. A hairline (1 px or less) stays as it is.
pub fn px(design: f32) -> Pixels {
    gpui::px(scaled(design, get()))
}

/// A real size, as design pixels.
pub fn dp(real: Pixels) -> f32 {
    f32::from(real) / get()
}

/// The width of the window in design pixels. The breakpoints read this.
pub fn viewport_w(window: &Window) -> f32 {
    dp(window.viewport_size().width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_walk_the_list_and_stop_at_the_ends() {
        assert_eq!(next_step(1.0, true), Some(1.1));
        assert_eq!(next_step(1.0, false), Some(0.9));
        assert_eq!(next_step(1.5, true), None);
        assert_eq!(next_step(0.9, false), None);
    }

    #[test]
    fn a_hairline_stays_crisp_and_the_rest_scales() {
        assert_eq!(scaled(1.0, 1.25), 1.0);
        assert_eq!(scaled(8.0, 1.25), 10.0);
    }
}
