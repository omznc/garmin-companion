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
//! `--palette` also takes `custom:<slug>`, a theme from the themes folder.
//! `--install-theme <query>` (debug builds) installs the top open-vsx result and exits.
//! `--ask "question"` sends a question at once. `--draft "text"` leaves text in the
//! composer. `--coach ollama:llama3.2` saves the model into the `--db` copy and exits.
//! Ctrl+K or Ctrl+Shift+P opens the command palette (Cmd on macOS). A debug build
//! opens it at start when `GC_OPEN_PALETTE` is set, with its value as the query.
//! A debug build opens the theme dialog and runs its search when `GC_OPEN_THEMES` is set.
//! Without `--db`, a persona runs the Ask screen as a canned demo.

// No console window on Windows in a release build.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod account;
mod activities;
mod appearance;
mod ask;
mod autosync;
mod blocks;
mod body;
mod calendar;
mod charts;
mod chrome;
mod claude_code;
mod cli_agent;
mod cloud;
mod codex;
mod command_palette;
mod commands;
mod composer;
mod controls;
mod data;
mod editor;
mod field;
mod focus;
mod fuzzy;
mod icons;
mod insight;
mod instance;
mod kit;
mod map;
mod markdown;
mod mcp;
mod motion;
mod opencode;
mod openvsx;
mod panel;
mod panes;
mod places;
mod scale;
mod screen;
mod scroll;
mod settings;
mod setup;
mod shell;
mod sidebar;
mod spring;
mod statusbar;
mod statusmenu;
mod tabs;
mod theme;
mod theme_dialog;
mod themes;
mod titlebar;
mod training;
mod ui;
mod update_ui;
mod updater;
mod updater_pkg;
mod views;
mod vscode;
mod wellness;
mod window;

use std::borrow::Cow;

use anyhow::Context as _;

use crate::scale::px;
use gpui::{
    point, size, App, AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowDecorations,
    WindowOptions,
};

use data::Source;
use theme::Palette;

/// The fonts of the design, in the binary. IBM Plex is static TrueType from the
/// IBM release (SIL OFL). It has the Latin Extended-A glyphs that Bosnian names need.
const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf"),
];

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

/// A built-in name, `custom:<slug>`, or the slug or name of a theme in the themes folder.
fn parse_palette(name: &str) -> Option<Palette> {
    let custom = garmin_core::theme::list().unwrap_or_default();
    Palette::from_id(name, &custom).or_else(|| {
        let theme = custom
            .iter()
            .find(|t| t.slug == name || t.name.eq_ignore_ascii_case(name))?;
        Palette::custom(theme)
    })
}

/// The command line, read once at the start.
struct Cli {
    palette: Palette,
    explicit_palette: bool,
    /// The size from `--size`. Without it the saved or the default size applies.
    size: Option<(f32, f32)>,
    source: Source,
    screen: Option<String>,
    start_activity: Option<i64>,
    coach: Option<String>,
    question: Option<String>,
    draft: Option<String>,
    start_sync: bool,
    background: bool,
    /// The focus to start with, from `--focus YYYY-MM-DD[..YYYY-MM-DD]`.
    focus: Option<focus::Focus>,
}

/// A day, or two days with `..` between them.
fn parse_focus(text: &str) -> anyhow::Result<focus::Focus> {
    let day = |d: &str| {
        d.parse::<chrono::NaiveDate>()
            .with_context(|| format!("--focus needs YYYY-MM-DD[..YYYY-MM-DD], got {text}"))
    };
    match text.split_once("..") {
        Some((a, b)) => Ok(focus::Focus::range(day(a)?, day(b)?)),
        None => Ok(focus::Focus::day(day(text)?)),
    }
}

fn parse_args(args: &[String]) -> anyhow::Result<Cli> {
    let palette = option(args, "--palette")
        .and_then(|n| parse_palette(&n))
        .unwrap_or(Palette::Paper);
    let size = option(args, "--size").and_then(|s| parse_size(&s));
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
            screen::Screen::from_label(name).is_some(),
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
        background: args.iter().any(|a| a == "--background"),
        focus: required(args, "--focus")?
            .map(|f| parse_focus(&f))
            .transpose()?,
    })
}

fn log_error<T>(what: &str, result: anyhow::Result<T>) -> anyhow::Result<T> {
    if let Err(error) = &result {
        eprintln!("could not load {what}: {error:#}");
    }
    result
}

/// The value, or the view of an empty cache when the default cache does not
/// read. A wrong `--db` path must stop here: `open_db` creates a missing file.
fn or_empty<T>(
    source: &Source,
    what: &str,
    result: anyhow::Result<T>,
    empty: fn() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    if matches!(source, Source::Default) {
        log_error(what, result).or_else(|_| empty())
    } else {
        result
    }
}

fn empty_db() -> anyhow::Result<garmin_core::Db> {
    Ok(garmin_api::open_db(std::path::Path::new(":memory:"))?)
}

fn empty_today() -> anyhow::Result<data::Today> {
    let view = garmin_api::today_view(&empty_db()?, chrono::Local::now().date_naive())?;
    Ok(map::today(&view, None))
}

fn empty_body() -> anyhow::Result<body::BodyData> {
    let views = garmin_api::body::body_views(&empty_db()?, chrono::Local::now().date_naive())?;
    Ok(body::BodyData(views))
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // The MCP server first: stdout is its transport, and it is not an app start.
    if args.get(1).is_some_and(|a| a == "mcp") {
        return mcp::run(&args[2..]);
    }
    #[cfg(debug_assertions)]
    if args.iter().any(|a| a == "--update-selftest") {
        return updater::selftest();
    }
    #[cfg(debug_assertions)]
    if let Some(query) = option(&args, "--install-theme") {
        return openvsx::selftest(&query);
    }
    let Cli {
        palette,
        explicit_palette,
        size: wanted_size,
        source,
        screen,
        start_activity,
        coach,
        question,
        draft,
        start_sync,
        background,
        focus,
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
            Source::Db { path, .. } => ask::Source::Copy(path.clone()),
            Source::Persona(_) => ask::Source::Demo,
        },
        question,
        draft,
    };
    // One copy only. A second launch brings the window of the first one forward.
    let second_launch = if matches!(source, Source::Default) {
        instance::claim()
    } else {
        instance::Claim::Off
    };
    if matches!(second_launch, instance::Claim::Second) {
        return Ok(());
    }
    // A bad cache must not close the app before a window opens. On Windows
    // nobody sees stderr. Show the empty views, and log the reason.
    let today = or_empty(&source, "Today", data::load_today(&source), empty_today)?;
    let store = activities::Store::for_source(&source)?;
    let wellness = log_error("Health and Sleep", wellness::load(&source)).ok();
    let body = or_empty(
        &source,
        "the body screens",
        body::load_body(&source),
        empty_body,
    )?;
    let training = log_error("Insights, Fitness and Reports", training::load(&source, 0)).ok();

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
            // A CLI coach run must not outlive the app: it spends model quota.
            cx.on_app_quit(|_| async { cli_agent::kill_all() }).detach();

            // A demo reads and writes no saved state.
            let demo = !matches!(source, data::Source::Default);
            let saved = if demo {
                window::State::default()
            } else {
                window::State::load()
            };
            scale::set(saved.scale);
            // For screenshots: `GC_SCALE=1.25` starts at a scale.
            #[cfg(debug_assertions)]
            if let Some(factor) = std::env::var("GC_SCALE").ok().and_then(|v| v.parse().ok()) {
                scale::set(factor);
            }
            let min = (MIN_SIZE.0 * scale::get(), MIN_SIZE.1 * scale::get());
            let visible = cx
                .primary_display()
                .map_or(Bounds::default(), |d| d.visible_bounds());
            let bounds = window::start_bounds(&saved, wanted_size, visible, min);
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
            let account = account::Account::new(demo, cx);
            // A palette from the command line wins. Otherwise the saved mode picks.
            let palette = if explicit_palette {
                palette
            } else {
                account::palette_for(
                    account.mode,
                    cx.window_appearance(),
                    account.themes.light,
                    account.themes.dark,
                )
            };
            let opened = cx.open_window(options, |window, cx| {
                cx.new(|cx| {
                    // A returning window reads the balance while a checkout is watched.
                    cx.observe_window_activation(window, |root: &mut shell::Root, window, cx| {
                        if window.is_window_active() {
                            root.on_activated(cx);
                            // A window that sat open past midnight moves to the new day.
                            if root.focus_is_stale() {
                                root.reload(cx);
                                cx.notify();
                            }
                        }
                    })
                    .detach();
                    let ask = shell::new_ask(ask_start, palette, cx);
                    let mut root =
                        shell::Root::new(today, palette, store, body, source, account, ask, saved);
                    if let Some(wellness) = wellness {
                        root = root.with_wellness(wellness);
                    }
                    if let Some(training) = training {
                        root = root.with_training(training);
                    }
                    if let Some(focus) = focus {
                        root.set_focus(focus, cx);
                    }
                    root.start(cx);
                    // For screenshots: `GC_OPEN_CALENDAR=1` opens the breadcrumb popover.
                    #[cfg(debug_assertions)]
                    if std::env::var("GC_OPEN_CALENDAR").is_ok() {
                        root.toggle_calendar_popover(window, cx);
                    }
                    if let Some(name) = &screen {
                        root.open(name, cx);
                    }
                    // For screenshots: `GC_EDIT_LAYOUT=<n>` starts the edit mode of Today
                    // with card `n` selected. A value that is not a number selects none.
                    #[cfg(debug_assertions)]
                    if let Ok(value) = std::env::var("GC_EDIT_LAYOUT") {
                        root.start_edit(value.parse().ok(), window, cx);
                    }
                    // For screenshots: `GC_SETTINGS_PAGE=2` picks the Appearance section.
                    #[cfg(debug_assertions)]
                    if let Some(at) = std::env::var("GC_SETTINGS_PAGE")
                        .ok()
                        .and_then(|v| v.parse().ok())
                    {
                        root.account.settings_page = at;
                    }
                    // For screenshots: `GC_OPEN_RECENTS=1` shows the thread list, and
                    // `GC_OPEN_THREAD=<id>` opens a saved thread.
                    #[cfg(debug_assertions)]
                    {
                        let ask = root.ask.clone();
                        if let Ok(id) = std::env::var("GC_OPEN_THREAD") {
                            ask.update(cx, |ask, cx| ask.open_thread(id, cx));
                        } else if let Ok(state) = std::env::var("GC_OPEN_RECENTS") {
                            ask.update(cx, |ask, cx| {
                                ask.show_recents(cx);
                                ask.preview_row(&state);
                            });
                        }
                    }
                    // For screenshots: `GC_HIGH_CONTRAST=1` turns high contrast on.
                    #[cfg(debug_assertions)]
                    if std::env::var("GC_HIGH_CONTRAST").is_ok() {
                        root.set_high_contrast(true, cx);
                    }
                    // For screenshots: `GC_SHOW_SETUP=1` shows the Setup screen on a demo run.
                    #[cfg(debug_assertions)]
                    if std::env::var("GC_SHOW_SETUP").is_ok() {
                        root.account.connected = false;
                    }
                    // For screenshots: `GC_OPEN_INSIGHT=<id>` opens the detail of an item.
                    #[cfg(debug_assertions)]
                    if let Ok(id) = std::env::var("GC_OPEN_INSIGHT") {
                        root.open_insight(id, cx);
                    }
                    if let Some(id) = start_activity {
                        root.start_at_activity(id, cx);
                    }
                    if start_sync {
                        root.start_sync(false, cx);
                    }
                    // For screenshots: `GC_SYNC_NOTE=<text>` shows a finished sync line.
                    #[cfg(debug_assertions)]
                    if let Ok(line) = std::env::var("GC_SYNC_NOTE") {
                        root.account.sync = account::Sync::Done(line);
                    }
                    root.watch_for_updates(cx);
                    root.watch_for_sync(cx);
                    root.watch_window(window, cx);
                    // For screenshots: `GC_OPEN_PALETTE=<query>` opens the palette at start.
                    #[cfg(debug_assertions)]
                    if let Ok(query) = std::env::var("GC_OPEN_PALETTE") {
                        root.toggle_palette(&query, window, cx);
                    }
                    // `GC_OPEN_STATUS_MENU=sync|coach|palette|version` opens a status bar menu.
                    #[cfg(debug_assertions)]
                    if let Some(kind) = std::env::var("GC_OPEN_STATUS_MENU")
                        .ok()
                        .and_then(|v| statusmenu::MenuKind::from_name(&v))
                    {
                        root.toggle_status_menu(kind, window, cx);
                    }
                    // `GC_OPEN_THEMES=<query>` opens the theme dialog and runs the search.
                    #[cfg(debug_assertions)]
                    if let Ok(query) = std::env::var("GC_OPEN_THEMES") {
                        root.open_themes(&query, window, cx);
                    }
                    root
                })
            });
            let handle = match opened {
                Ok(handle) => handle,
                Err(error) => {
                    eprintln!("could not open the window: {error:#}");
                    cx.quit();
                    return;
                }
            };
            #[cfg(target_os = "linux")]
            window::shrink_when_screen_known(handle.into(), min, cx);
            command_palette::bind_keys(cx);
            cx.on_action(move |_: &command_palette::OpenPalette, cx| {
                let _ = handle.update(cx, |root, window, cx| root.toggle_palette("", window, cx));
            });
            cx.on_action(move |_: &command_palette::ZoomIn, cx| {
                let _ = handle.update(cx, |root, _, cx| root.zoom(Some(true), cx));
            });
            cx.on_action(move |_: &command_palette::ZoomOut, cx| {
                let _ = handle.update(cx, |root, _, cx| root.zoom(Some(false), cx));
            });
            cx.on_action(move |_: &command_palette::ZoomReset, cx| {
                let _ = handle.update(cx, |root, _, cx| root.zoom(None, cx));
            });
            cx.on_action(move |_: &command_palette::OpenSettings, cx| {
                let _ = handle.update(cx, |root, _, cx| root.show_settings(cx));
            });
            if background {
                // The login item passes --background. The app has no tray icon,
                // so the window starts minimised and stays in the task bar.
                let _ = handle.update(cx, |_, window, _| window.minimize_window());
            } else {
                cx.activate(true);
            }
            if let instance::Claim::First(rx) = second_launch {
                cx.spawn(async move |cx| {
                    while rx.recv().await.is_ok() {
                        cx.update(|cx| {
                            cx.activate(true);
                            let _ = handle.update(cx, |_, window, _| window.activate_window());
                        });
                    }
                })
                .detach();
            }
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
    fn the_focus_flag_takes_a_day_or_a_range() {
        let day = |d| chrono::NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
        assert_eq!(
            parse_focus("2026-10-14").unwrap(),
            focus::Focus::day(day(14))
        );
        let range = parse_focus("2026-10-20..2026-10-14").unwrap();
        assert_eq!((range.start, range.end), (day(14), day(20)));
        assert!(parse_focus("soon").is_err());
    }

    #[test]
    fn a_bad_cache_degrades_to_the_empty_views() {
        assert!(or_empty(
            &Source::Default,
            "Today",
            Err(anyhow::anyhow!("bad row")),
            empty_today
        )
        .is_ok());
        assert!(or_empty(
            &Source::Default,
            "body",
            Err(anyhow::anyhow!("bad row")),
            empty_body
        )
        .is_ok());
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
