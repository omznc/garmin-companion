//! Garmin Companion for the desktop, on gpui-ce.
//!
//! Phase 1 of the rewrite: one live screen, Today, in the look of the current
//! app. Run it with `cargo run -p garmin-companion-desktop`. Options:
//! `--palette <name>` picks the start palette and `--size <W>x<H>` sets the
//! window size. `--db <copy>` reads a copy of the cache through `garmin-api`.
//! `--date YYYY-MM-DD` sets the day for that read, for a demo on an old cache.
//! `--screen <name>` opens on a sidebar entry, for example `sleep`, `weight`, `ask` or `settings`.
//! `--open <name>` is the same flag. `--activity <id>` opens the Activities entry on one
//! session. `--sync` starts a sync at once.
//! `--persona golfer` shows a made-up user from `garmin-api`. With neither `--db`
//! nor `--persona`, the app reads the cache in the data folder and asks for a
//! Garmin sign-in when there is none. `--persona` and `--db` run a demo: the
//! sync is a fake and no setting is saved.
//! `--ask "question"` sends a question at once. `--draft "text"` leaves text in the
//! composer. `--coach ollama:llama3.2` saves the model into the `--db` copy and exits.
//! Without `--db`, a persona runs the Ask screen as a canned demo.

mod account;
mod activities;
mod ask;
mod body;
mod chrome;
mod cloud;
mod composer;
mod controls;
mod data;
mod editor;
mod field;
mod icons;
mod map;
mod markdown;
mod motion;
mod places;
mod scroll;
mod settings;
mod setup;
mod shell;
mod sidebar;
mod spring;
mod theme;
mod today;
mod training;
mod ui;
mod update_ui;
mod updater;
mod wellness;

use std::borrow::Cow;

use anyhow::Context as _;

use gpui::{
    point, px, size, App, AppContext as _, Bounds, TitlebarOptions, WindowBounds,
    WindowDecorations, WindowOptions,
};

use data::Source;
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

/// The value of a flag that must have one. A flag with no value is an error,
/// so a typo does not fall back to the sample without a word.
fn required(args: &[String], flag: &str) -> anyhow::Result<Option<String>> {
    match option(args, flag) {
        Some(value) => Ok(Some(value)),
        None if args.iter().any(|a| a == flag) => anyhow::bail!("{flag} needs a value"),
        None => Ok(None),
    }
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

/// The command line, read once at the start.
struct Cli {
    palette: Palette,
    explicit_palette: bool,
    size: (f32, f32),
    source: Source,
    screen: Option<String>,
    start_activity: Option<i64>,
    coach: Option<String>,
    question: Option<String>,
    draft: Option<String>,
    start_sync: bool,
}

fn parse_args(args: &[String]) -> anyhow::Result<Cli> {
    let palette = option(args, "--palette")
        .and_then(|n| parse_palette(&n))
        .unwrap_or(Palette::Paper);
    let size = option(args, "--size")
        .and_then(|s| parse_size(&s))
        .unwrap_or(DEFAULT_SIZE);
    let db = required(args, "--db")?;
    let date = required(args, "--date")?
        .map(|d| {
            d.parse::<chrono::NaiveDate>()
                .with_context(|| format!("--date needs YYYY-MM-DD, got {d}"))
        })
        .transpose()?;
    let persona = required(args, "--persona")?;
    let screen = match required(args, "--screen")? {
        Some(name) => Some(name),
        None => required(args, "--open")?,
    };
    if let Some(name) = &screen {
        anyhow::ensure!(
            sidebar::NAV
                .iter()
                .any(|(l, _)| l.eq_ignore_ascii_case(name)),
            "--screen {name} is not a screen"
        );
    }
    let source = match (db, date, persona) {
        (Some(_), _, Some(_)) => anyhow::bail!("--persona and --db cannot go together"),
        (Some(path), date, None) => data::Source::Db {
            path: path.into(),
            date,
        },
        (None, Some(_), _) => anyhow::bail!("--date needs --db"),
        (None, None, Some(persona)) => data::Source::Persona(persona),
        (None, None, None) => data::Source::Default,
    };
    let start_activity = required(args, "--activity")?
        .map(|v| {
            v.parse::<i64>()
                .with_context(|| format!("--activity needs a number, got {v}"))
        })
        .transpose()?;
    Ok(Cli {
        palette,
        explicit_palette: option(args, "--palette").is_some(),
        size,
        source,
        screen,
        start_activity,
        coach: required(args, "--coach")?,
        question: required(args, "--ask")?,
        draft: required(args, "--draft")?,
        start_sync: args.iter().any(|a| a == "--sync"),
    })
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let Cli {
        palette,
        explicit_palette,
        size: (width, height),
        source,
        screen,
        start_activity,
        coach,
        question,
        draft,
        start_sync,
    } = parse_args(&args)?;
    if let Some(spec) = coach {
        let Source::Db { path, .. } = &source else {
            anyhow::bail!("--coach needs --db");
        };
        println!("{}", ask::setup(path, &spec)?);
        return Ok(());
    }
    let ask_start = shell::AskStart {
        source: match &source {
            Source::Default => ask::Source::Db(
                garmin_core::db::default_path().context("could not locate a data directory")?,
            ),
            Source::Db { path, .. } => ask::Source::Db(path.clone()),
            Source::Persona(_) => ask::Source::Demo,
        },
        question,
        draft,
    };
    let today = data::load_today(&source)?;
    let store = activities::Store::for_source(&source)?;
    let wellness = wellness::load(&source)?;
    let body = body::load_body(&source)?;
    let training = training::load(&source, 0)?;

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
                // Matches StartupWMClass in the .desktop file, so the shell links the window to the launcher.
                app_id: Some("garmin-companion".into()),
                ..Default::default()
            };
            let demo = !matches!(source, data::Source::Default);
            let account = account::Account::new(demo, cx);
            // A palette from the command line wins. Otherwise the saved mode picks.
            let palette = if explicit_palette {
                palette
            } else {
                account::palette_for(account.mode, cx.window_appearance())
            };
            let opened = cx.open_window(options, |_, cx| {
                cx.new(|cx| {
                    let ask = shell::new_ask(ask_start, palette, cx);
                    let mut root =
                        shell::Root::new(today, palette, store, body, source, account, ask)
                            .with_wellness(wellness)
                            .with_training(training);
                    if let Some(name) = &screen {
                        root.open(name, cx);
                    }
                    if let Some(id) = start_activity {
                        root.start_at_activity(id, cx);
                    }
                    if start_sync {
                        root.start_sync(false, cx);
                    }
                    root.watch_for_updates(cx);
                    root
                })
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

    #[test]
    fn a_flag_with_no_value_is_an_error() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(required(&args(&["app", "--db"]), "--db").is_err());
        assert!(required(&args(&["app", "--db", "--palette", "ink"]), "--db").is_err());
        assert_eq!(required(&args(&["app"]), "--db").unwrap(), None);
        assert_eq!(
            required(&args(&["app", "--db", "x"]), "--db")
                .unwrap()
                .as_deref(),
            Some("x")
        );
    }
}
