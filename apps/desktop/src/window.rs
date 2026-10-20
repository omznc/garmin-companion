//! The window's saved state: size, position, the coach panel and the closed
//! sidebar groups. It lives in `window.json` in the data folder, next to the
//! preferences. A read error gives the default state. A write error is logged.

use std::path::PathBuf;

use gpui::{point, px, size, Bounds, Pixels};
use serde::{Deserialize, Serialize};

/// The first size of a new window. A small screen clamps it.
pub const DEFAULT_SIZE: (f32, f32) = (1440.0, 900.0);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub coach_open: bool,
    /// The keys of the sidebar groups that the user closed.
    pub collapsed: Vec<String>,
    /// The calendar block of the sidebar is closed.
    pub calendar_closed: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            x: None,
            y: None,
            coach_open: true,
            collapsed: Vec::new(),
            calendar_closed: false,
        }
    }
}

fn path() -> Option<PathBuf> {
    garmin_core::paths::base_dir()
        .ok()
        .map(|dir| dir.join("window.json"))
}

impl State {
    /// The saved state, or the default one when the file is missing or bad.
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = path() else {
            return;
        };
        let written = serde_json::to_string(self)
            .map_err(anyhow::Error::from)
            .and_then(|text| std::fs::write(&path, text).map_err(anyhow::Error::from));
        if let Err(error) = written {
            eprintln!("could not save the window state: {error:#}");
        }
    }

    /// Keep the bounds of a window that is not zoomed.
    pub fn remember(&mut self, bounds: Bounds<Pixels>) {
        self.width = Some(f32::from(bounds.size.width));
        self.height = Some(f32::from(bounds.size.height));
        self.x = Some(f32::from(bounds.origin.x));
        self.y = Some(f32::from(bounds.origin.y));
    }
}

/// The size, cut down to the visible area of the screen. A zero area (no
/// screen known) leaves the size as it is.
pub fn clamp(wanted: (f32, f32), visible: (f32, f32), min: (f32, f32)) -> (f32, f32) {
    let fit = |want: f32, room: f32, low: f32| {
        if room > 0.0 {
            want.min(room).max(low.min(room))
        } else {
            want
        }
    };
    (
        fit(wanted.0, visible.0, min.0),
        fit(wanted.1, visible.1, min.1),
    )
}

/// The bounds for the first window. `explicit` is the size from the command
/// line and wins over the saved one. The saved position stays only when it
/// lies inside the visible area.
pub fn start_bounds(
    saved: &State,
    explicit: Option<(f32, f32)>,
    visible: Bounds<Pixels>,
    min: (f32, f32),
) -> Bounds<Pixels> {
    let from_saved = saved.width.zip(saved.height);
    let wanted = explicit.or(from_saved).unwrap_or(DEFAULT_SIZE);
    let room = (
        f32::from(visible.size.width),
        f32::from(visible.size.height),
    );
    let (w, h) = clamp(wanted, room, min);
    let centred = || {
        point(
            visible.origin.x + (visible.size.width - px(w)) / 2.0,
            visible.origin.y + (visible.size.height - px(h)) / 2.0,
        )
    };
    let origin = match (saved.x, saved.y, explicit) {
        (Some(x), Some(y), None) => {
            let (left, top) = (f32::from(visible.origin.x), f32::from(visible.origin.y));
            if x >= left && y >= top && x + w <= left + room.0 && y + h <= top + room.1 {
                point(px(x), px(y))
            } else {
                centred()
            }
        }
        _ => centred(),
    };
    Bounds::new(origin, size(px(w), px(h)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: (f32, f32) = (720.0, 540.0);

    fn screen(w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(0.0), px(0.0)), size(px(w), px(h)))
    }

    #[test]
    fn the_default_size_shrinks_to_a_small_screen() {
        assert_eq!(clamp(DEFAULT_SIZE, (1366.0, 728.0), MIN), (1366.0, 728.0));
        assert_eq!(clamp(DEFAULT_SIZE, (2560.0, 1400.0), MIN), DEFAULT_SIZE);
        assert_eq!(clamp(DEFAULT_SIZE, (0.0, 0.0), MIN), DEFAULT_SIZE);
    }

    #[test]
    fn the_command_line_size_wins_over_the_saved_one() {
        let saved = State {
            width: Some(1000.0),
            height: Some(700.0),
            x: Some(10.0),
            y: Some(10.0),
            ..State::default()
        };
        let own = start_bounds(&saved, Some((1100.0, 800.0)), screen(1920.0, 1080.0), MIN);
        assert_eq!(f32::from(own.size.width), 1100.0);
        let kept = start_bounds(&saved, None, screen(1920.0, 1080.0), MIN);
        assert_eq!(f32::from(kept.origin.x), 10.0);
        // A saved spot outside the screen falls back to the centre.
        let off = start_bounds(&saved, None, screen(900.0, 600.0), MIN);
        assert!(f32::from(off.origin.x) >= 0.0 && f32::from(off.size.width) <= 900.0);
    }

    #[test]
    fn a_bad_file_gives_the_default_state() {
        assert!(serde_json::from_str::<State>("{\"coachOpen\": 3").is_err());
        let partial: State = serde_json::from_str("{\"width\": 900.0}").unwrap();
        assert!(partial.coach_open && partial.width == Some(900.0));
    }
}
