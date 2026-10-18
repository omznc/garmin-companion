//! Garmin Companion for the desktop, on gpui-ce.
//!
//! Phase 1 of the rewrite: one live screen, Today, in the look of the current
//! app. Run it with `cargo run -p garmin-companion-desktop`. Options:
//! `--palette <name>` picks the start palette and `--size <W>x<H>` sets the
//! window size. `--db <copy>` reads a copy of the cache through `garmin-api`.
//! `--date YYYY-MM-DD` sets the day for that read, for a demo on an old cache.

mod chrome;
mod data;
mod icons;
mod map;
mod shell;
mod sidebar;
mod spring;
mod theme;
mod today;
mod ui;

use std::borrow::Cow;

use gpui::{
    point, px, size, App, AppContext as _, Bounds, TitlebarOptions, WindowBounds,
    WindowDecorations, WindowOptions,
};

use theme::Palette;

/// The fonts of the current app, in the binary. They are static TrueType
/// instances of the upstream variable fonts, with the Latin Extended-A glyphs
/// that Bosnian names need.
const FONTS: [&[u8]; 7] = [
    include_bytes!("../assets/fonts/InstrumentSans-Regular.ttf"),
    include_bytes!("../assets/fonts/InstrumentSans-Medium.ttf"),
    include_bytes!("../assets/fonts/InstrumentSans-SemiBold.ttf"),
    include_bytes!("../assets/fonts/InstrumentSerif-Regular.ttf"),
    include_bytes!("../assets/fonts/InstrumentSerif-Italic.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Regular.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Medium.ttf"),
];

const DEFAULT_SIZE: (f32, f32) = (1100.0, 780.0);
const MIN_SIZE: (f32, f32) = (720.0, 540.0);

/// The value after `flag` in the arguments, if there is one. Another flag is not a value.
fn option(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .filter(|v| !v.starts_with("--"))
        .cloned()
}

fn parse_size(text: &str) -> Option<(f32, f32)> {
    let (w, h) = text.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn parse_palette(name: &str) -> Option<Palette> {
    Palette::ALL
        .into_iter()
        .find(|p| p.name().eq_ignore_ascii_case(name))
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let palette = option(&args, "--palette")
        .and_then(|n| parse_palette(&n))
        .unwrap_or(Palette::Paper);
    let (width, height) = option(&args, "--size")
        .and_then(|s| parse_size(&s))
        .unwrap_or(DEFAULT_SIZE);
    let source = match option(&args, "--db") {
        Some(path) => data::Source::Db {
            path: path.into(),
            date: option(&args, "--date").and_then(|d| d.parse().ok()),
        },
        None if args.iter().any(|a| a == "--db") => anyhow::bail!("--db needs a path"),
        None => data::Source::Sample,
    };
    let today = data::load_today(&source)?;

    gpui_platform::application()
        .with_assets(icons::Icons)
        .run(move |cx: &mut App| {
            if let Err(error) = cx
                .text_system()
                .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
            {
                eprintln!("could not load the bundled fonts: {error:#}");
            }
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))),
                // Ask for client side decorations. The window draws its own strip,
                // as the current app does with `decorations: false`.
                window_decorations: Some(WindowDecorations::Client),
                titlebar: Some(TitlebarOptions {
                    title: Some("Garmin Companion".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(20.0), px(13.0))),
                }),
                ..Default::default()
            };
            let opened = cx.open_window(options, |_, cx| {
                cx.new(|_| shell::Root::new(today, palette))
            });
            if let Err(error) = opened {
                eprintln!("could not open the window: {error:#}");
                cx.quit();
            }
            cx.activate(true);
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_options_parse_and_unknown_values_fall_back() {
        let args: Vec<String> = ["app", "--palette", "INK", "--size", "1280x900"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            option(&args, "--palette").and_then(|n| parse_palette(&n)),
            Some(Palette::Ink)
        );
        assert_eq!(
            option(&args, "--size").and_then(|s| parse_size(&s)),
            Some((1280.0, 900.0))
        );
        assert_eq!(parse_palette("nope"), None);
        assert_eq!(parse_size("wide"), None);
        assert_eq!(option(&args, "--missing"), None);
    }
}
