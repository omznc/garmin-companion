//! The menus of the status bar: what each one lists, and how it opens, moves
//! and acts. `entries` is pure: it turns a `Facts` value into rows, so a test
//! can check each menu without a window.

use std::time::{Duration, Instant};

use crate::scale::px;
use garmin_api::settings::ThemeMode;
use garmin_coach::Provider;
use gpui::{
    anchored, deferred, div, point, prelude::*, Anchor, AnyElement, BoxShadow, ClickEvent,
    ClipboardItem, Context, Div, FocusHandle, KeyDownEvent, SharedString, Window,
};

use crate::account::Sync;
use crate::icons::{icon, icon_spin, Icon};
use crate::shell::Root;
use crate::theme::{Palette, Theme, FS_SMALL};
use crate::ui::{MONO, SANS};

/// The Settings sections that the menus open: the index in `settings::SECTIONS`.
const GARMIN_SECTION: usize = 0;
const COACH_SECTION: usize = 1;
/// The height of an item: the bar is 26 px with a 1 px border.
const BAR_H: f32 = 25.0;
/// A click on the item that just closed its menu must not open it again.
const REOPEN_GUARD: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    Sync,
    Coach,
    Palette,
    Version,
}

impl MenuKind {
    /// For the screenshot variable `GC_OPEN_STATUS_MENU`.
    #[cfg(debug_assertions)]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "sync" => Self::Sync,
            "coach" => Self::Coach,
            "palette" => Self::Palette,
            "version" => Self::Version,
            _ => return None,
        })
    }
}

/// What a row does.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    SyncNow,
    FullSync,
    GarminSettings,
    Coach(Provider),
    Palette(Palette),
    Mode(ThemeMode),
    Themes,
    CheckUpdates,
    Install,
    ReleaseNotes,
    CopyVersion,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mark {
    None,
    Check,
    Spinner,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Item(Act),
    /// A line of text, not a stop for the keyboard.
    Note {
        bad: bool,
    },
    Head,
    Rule,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub row: Row,
    pub label: String,
    pub hint: Option<String>,
    pub mark: Mark,
    pub enabled: bool,
    /// Rows with this flag share one line.
    pub inline: bool,
}

impl Entry {
    fn new(row: Row, label: impl Into<String>) -> Self {
        Self {
            row,
            label: label.into(),
            hint: None,
            mark: Mark::None,
            enabled: true,
            inline: false,
        }
    }

    fn item(act: Act, label: impl Into<String>) -> Self {
        Self::new(Row::Item(act), label)
    }

    fn note(label: impl Into<String>, bad: bool) -> Self {
        Self::new(Row::Note { bad }, label)
    }

    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    fn marked(mut self, on: bool) -> Self {
        if on {
            self.mark = Mark::Check;
        }
        self
    }

    fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }

    /// A stop for the keyboard and the pointer.
    pub fn act(&self) -> Option<&Act> {
        match &self.row {
            Row::Item(act) if self.enabled => Some(act),
            _ => None,
        }
    }
}

/// The update state that the version menu needs.
#[derive(Debug, Clone, Default)]
pub struct UpdateFacts {
    pub busy: bool,
    pub note: Option<(String, bool)>,
    /// The offered version, and whether the app can only point to the release page.
    pub offer: Option<(String, bool)>,
}

/// Everything that the four menus read.
#[derive(Debug, Clone)]
pub struct Facts {
    pub connected: bool,
    pub demo: bool,
    pub sync: Sync,
    pub synced_ago: Option<String>,
    pub provider: Option<Provider>,
    pub credits: Option<u64>,
    pub dark: bool,
    /// The palettes of the current mode, and whether each one is chosen.
    pub palettes: Vec<(Palette, bool)>,
    pub mode: ThemeMode,
    pub update: UpdateFacts,
    pub version: &'static str,
}

/// The coach providers that the menu lists, with the name of each. One place
/// to add a provider: the radio list of the status bar reads only this.
pub fn coach_providers() -> Vec<(Provider, &'static str)> {
    crate::cloud::CHOICES.iter().map(|c| (c.3, c.1)).collect()
}

pub fn entries(kind: MenuKind, f: &Facts) -> Vec<Entry> {
    match kind {
        MenuKind::Sync => sync_entries(f),
        MenuKind::Coach => coach_entries(f),
        MenuKind::Palette => palette_entries(f),
        MenuKind::Version => version_entries(f),
    }
}

/// The line about the last sync: the step, the result, the error or the age.
pub fn sync_line(sync: &Sync, ago: Option<&str>) -> (String, bool) {
    match (sync, ago) {
        (Sync::Running(step), _) => (step.clone(), false),
        (Sync::Done(line), _) => (line.clone(), false),
        (Sync::Failed(error), _) => (format!("Last sync failed: {error}"), true),
        (Sync::Idle, Some(ago)) => (format!("Last sync: {ago}"), false),
        (Sync::Idle, None) => ("No sync yet.".into(), false),
    }
}

fn sync_entries(f: &Facts) -> Vec<Entry> {
    let busy = matches!(f.sync, Sync::Running(_));
    let can = f.connected && !busy;
    let (line, bad) = sync_line(&f.sync, f.synced_ago.as_deref());
    let mut now = Entry::item(Act::SyncNow, if busy { "Syncing..." } else { "Sync now" });
    if busy {
        now.mark = Mark::Spinner;
    }
    vec![
        Entry::note(line, bad),
        Entry::new(Row::Rule, ""),
        now.enabled(can),
        Entry::item(Act::FullSync, "Full re-sync").enabled(can),
        Entry::new(Row::Rule, ""),
        Entry::item(Act::GarminSettings, "Open Garmin settings"),
    ]
}

fn coach_entries(f: &Facts) -> Vec<Entry> {
    let mut rows = vec![Entry::new(Row::Head, "Coach")];
    for (provider, name) in coach_providers() {
        let mut row = Entry::item(Act::Coach(provider), name).marked(f.provider == Some(provider));
        if let (Provider::Cloud, Some(n)) = (provider, f.credits) {
            row = row.hint(format!("{n} left"));
        }
        rows.push(row);
    }
    rows
}

fn palette_entries(f: &Facts) -> Vec<Entry> {
    let head = if f.dark {
        "Dark palettes"
    } else {
        "Light palettes"
    };
    let mut rows = vec![Entry::new(Row::Head, head)];
    for (palette, on) in &f.palettes {
        rows.push(Entry::item(Act::Palette(*palette), palette.name()).marked(*on));
    }
    rows.push(Entry::new(Row::Rule, ""));
    rows.push(Entry::new(Row::Head, "Mode"));
    for (mode, name) in [
        (ThemeMode::System, "System"),
        (ThemeMode::Light, "Light"),
        (ThemeMode::Dark, "Dark"),
    ] {
        let mut row = Entry::item(Act::Mode(mode), name).marked(f.mode == mode);
        row.inline = true;
        rows.push(row);
    }
    rows.push(Entry::new(Row::Rule, ""));
    rows.push(Entry::item(Act::Themes, "Browse VS Code themes..."));
    rows
}

fn version_entries(f: &Facts) -> Vec<Entry> {
    let u = &f.update;
    let mut check = Entry::item(Act::CheckUpdates, "Check for updates").enabled(!u.busy && !f.demo);
    if f.demo {
        check = check.hint("Off in a demo");
    }
    let mut rows = vec![check];
    if let Some((text, bad)) = &u.note {
        rows.push(Entry::note(text.clone(), *bad));
    }
    if let Some((version, manual)) = &u.offer {
        rows.push(Entry::note(
            format!("Version {version} is available."),
            false,
        ));
        let label = if *manual {
            "Open the release page"
        } else {
            "Install and restart"
        };
        rows.push(Entry::item(Act::Install, label).enabled(!u.busy));
    }
    rows.push(Entry::new(Row::Rule, ""));
    rows.push(Entry::item(Act::ReleaseNotes, "Release notes"));
    rows.push(Entry::item(Act::CopyVersion, "Copy version").hint(format!("v{}", f.version)));
    rows
}

/// The next stop for the keyboard from `at`, one step `down` or up. It stops
/// at the ends. `None` when no row can be chosen.
pub fn step(rows: &[Entry], at: Option<usize>, down: bool) -> Option<usize> {
    let stops: Vec<usize> = (0..rows.len())
        .filter(|&i| rows[i].act().is_some())
        .collect();
    let first = *stops.first()?;
    let last = *stops.last()?;
    let Some(at) = at else {
        return Some(if down { first } else { last });
    };
    let next = if down {
        stops.iter().copied().find(|&i| i > at)
    } else {
        stops.iter().copied().rev().find(|&i| i < at)
    };
    next.or(Some(at.clamp(first, last)))
}

/// The page of the release of this version.
pub fn release_url(version: &str) -> String {
    format!("https://github.com/omznc/garmin-companion/releases/tag/v{version}")
}

/* ------------------------------------------------------------------ state --- */

/// The open menu.
pub struct Open {
    pub kind: MenuKind,
    /// The row with the keyboard mark, an index in `entries`.
    pub at: Option<usize>,
    /// The menu holds the focus, so that the keys reach it.
    pub focus: FocusHandle,
}

#[derive(Default)]
pub struct StatusMenu {
    pub open: Option<Open>,
    closed: Option<(MenuKind, Instant)>,
}

fn facts(root: &Root, cx: &Context<Root>) -> Facts {
    let account = &root.account;
    let ask = root.ask.read(cx);
    let themes = &account.themes;
    let dark = root.palette.is_dark();
    let current = if dark { &themes.dark } else { &themes.light };
    let update = &account.update;
    Facts {
        connected: account.connected,
        demo: account.demo,
        sync: account.sync.clone(),
        synced_ago: account.synced_ago.clone(),
        provider: ask.provider(),
        credits: ask.credits_left(),
        dark,
        palettes: crate::themes::palettes(&themes.custom, dark)
            .into_iter()
            .map(|p| (p, p.id() == current.id()))
            .collect(),
        mode: account.mode,
        update: UpdateFacts {
            busy: update.busy(),
            note: update.note.clone(),
            offer: update
                .offer
                .as_ref()
                .map(|o| (o.version.clone(), o.kind == crate::updater::Kind::Manual)),
        },
        version: crate::updater::VERSION,
    }
}

impl Root {
    pub(crate) fn toggle_status_menu(
        &mut self,
        kind: MenuKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let menu = &mut self.status_menu;
        if menu.open.as_ref().is_some_and(|o| o.kind == kind) {
            return self.close_status_menu(cx);
        }
        let just_closed = menu
            .closed
            .is_some_and(|(k, at)| k == kind && at.elapsed() < REOPEN_GUARD);
        if just_closed {
            return;
        }
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let rows = entries(kind, &facts(self, cx));
        self.status_menu.open = Some(Open {
            kind,
            at: step(&rows, None, true),
            focus,
        });
        cx.notify();
    }

    pub(crate) fn close_status_menu(&mut self, cx: &mut Context<Self>) {
        let menu = &mut self.status_menu;
        if let Some(open) = menu.open.take() {
            menu.closed = Some((open.kind, Instant::now()));
            cx.notify();
        }
    }

    fn status_rows(&self, cx: &Context<Self>) -> Option<Vec<Entry>> {
        let open = self.status_menu.open.as_ref()?;
        Some(entries(open.kind, &facts(self, cx)))
    }

    fn status_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rows) = self.status_rows(cx) else {
            return;
        };
        let Some(open) = self.status_menu.open.as_mut() else {
            return;
        };
        match event.keystroke.key.as_str() {
            "escape" => self.close_status_menu(cx),
            key @ ("down" | "up") => {
                open.at = step(&rows, open.at, key == "down");
                cx.notify();
            }
            "enter" => {
                let act = open.at.and_then(|i| rows.get(i)).and_then(Entry::act);
                if let Some(act) = act.cloned() {
                    self.run_status_act(act, window, cx);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn run_status_act(&mut self, act: Act, window: &mut Window, cx: &mut Context<Self>) {
        // The check shows its result in the menu, so that menu stays open.
        if act != Act::CheckUpdates {
            self.close_status_menu(cx);
        }
        match act {
            Act::SyncNow => self.start_sync(false, cx),
            Act::FullSync => self.start_sync(true, cx),
            Act::GarminSettings => self.show_settings_section(GARMIN_SECTION, cx),
            Act::Coach(provider) => self.choose_coach(provider, cx),
            Act::Palette(palette) => self.choose_palette(palette, window, cx),
            Act::Mode(mode) => self.set_theme_mode(mode, window, cx),
            Act::Themes => self.open_themes("", window, cx),
            Act::CheckUpdates => self.check_for_updates(cx),
            Act::Install => self.install_update(cx),
            Act::ReleaseNotes => cx.open_url(&release_url(crate::updater::VERSION)),
            Act::CopyVersion => cx.write_to_clipboard(ClipboardItem::new_string(format!(
                "v{}",
                crate::updater::VERSION
            ))),
        }
        cx.notify();
    }

    pub(crate) fn show_settings_section(&mut self, at: usize, cx: &mut Context<Self>) {
        self.account.settings_page = at;
        self.show_settings(cx);
    }

    /// Switch the provider as Settings does. A key that is not saved yet needs
    /// the Coach section of Settings.
    fn choose_coach(&mut self, provider: Provider, cx: &mut Context<Self>) {
        if !self.account.cloud.live {
            self.cloud_open(cx);
        }
        if self.account.cloud.needs_key(provider) {
            return self.show_settings_section(COACH_SECTION, cx);
        }
        self.cloud_choose(provider, cx);
    }
}

/* ---------------------------------------------------------------- drawing --- */

fn lift(t: &Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: t.shadow,
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(2.0),
        spread_radius: px(0.0),
        inset: false,
    }]
}

fn mark(mark: Mark, t: &Theme) -> Div {
    let slot = div().flex_none().size(px(12.0));
    match mark {
        Mark::None => slot,
        Mark::Check => slot.child(icon(Icon::Check, 12.0, t, t.acc)),
        Mark::Spinner => slot.child(icon_spin(Icon::Sync, 12.0, t.acc, "status-menu-spin")),
    }
}

fn row(i: usize, e: &Entry, keyed: bool, t: &Theme, cx: &mut Context<Root>) -> AnyElement {
    let (hover, fg, faint) = (t.hover, t.fg, t.faint);
    let base = div()
        .id(("status-row", i))
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(8.0))
        .py(px(5.0))
        .rounded(px(4.0))
        .text_color(if e.enabled { t.mutd } else { faint });
    match &e.row {
        Row::Rule => div()
            .h(px(1.0))
            .my(px(4.0))
            .mx(px(-4.0))
            .bg(t.line)
            .into_any_element(),
        Row::Head => div()
            .px(px(8.0))
            .pt(px(4.0))
            .pb(px(2.0))
            .text_size(px(11.0))
            .text_color(t.faint)
            .child(e.label.clone())
            .into_any_element(),
        Row::Note { bad } => div()
            .px(px(8.0))
            .py(px(4.0))
            .max_w(px(240.0))
            .text_size(px(12.0))
            .text_color(if *bad { t.warn } else { t.mutd })
            .child(e.label.clone())
            .into_any_element(),
        Row::Item(act) => {
            let act = act.clone();
            let live = e.enabled;
            let body = base
                .when(live, |d| {
                    d.cursor_pointer()
                        .hover(move |s| s.bg(hover).text_color(fg))
                        .on_click(cx.listener(move |root, _: &ClickEvent, window, cx| {
                            root.run_status_act(act.clone(), window, cx)
                        }))
                })
                .when(live && keyed, |d| d.bg(hover).text_color(fg))
                // An inline row is a segment: the chosen one has the active colour.
                .when(e.inline, |d| {
                    d.justify_center()
                        .when(e.mark == Mark::Check, |d| d.bg(t.active).text_color(fg))
                })
                .when(!e.inline, |d| d.child(mark(e.mark, t)))
                .child(
                    div()
                        .flex_1()
                        .when(e.inline, |d| d.text_center())
                        .child(e.label.clone()),
                )
                .children(e.hint.clone().map(|h| {
                    div()
                        .font_family(MONO)
                        .text_size(px(11.0))
                        .text_color(faint)
                        .child(h)
                }));
            body.into_any_element()
        }
    }
}

/// The rows of one menu. Rows with `inline` share one line.
fn body(rows: &[Entry], at: Option<usize>, t: &Theme, cx: &mut Context<Root>) -> Div {
    let mut list = div().flex().flex_col();
    let mut line: Option<Div> = None;
    for (i, e) in rows.iter().enumerate() {
        let el = row(i, e, at == Some(i), t, cx);
        if e.inline {
            let cell = div().flex_1().child(el);
            line = Some(
                line.unwrap_or_else(|| div().flex().gap(px(2.0)))
                    .child(cell),
            );
            continue;
        }
        if let Some(done) = line.take() {
            list = list.child(done);
        }
        list = list.child(el);
    }
    if let Some(done) = line {
        list = list.child(done);
    }
    list
}

/// The menu of `kind`, above the item that holds it. `None` while it is closed.
pub fn menu(root: &Root, kind: MenuKind, t: &Theme, cx: &mut Context<Root>) -> Option<AnyElement> {
    let open = root.status_menu.open.as_ref().filter(|o| o.kind == kind)?;
    let (focus, at) = (open.focus.clone(), open.at);
    let rows = root.status_rows(cx)?;
    let panel =
        div()
            .id(SharedString::from(format!("status-menu-{kind:?}")))
            .track_focus(&focus)
            .occlude()
            .w(px(236.0))
            .p(px(4.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(t.line)
            .bg(t.bg)
            .shadow(lift(t))
            .max_h(px(440.0))
            .overflow_y_scroll()
            .font_family(SANS)
            .text_size(px(FS_SMALL))
            .on_key_down(cx.listener(|root, event: &KeyDownEvent, window, cx| {
                root.status_key(event, window, cx)
            }))
            .on_mouse_down_out(cx.listener(|root, _, _, cx| root.close_status_menu(cx)))
            .child(body(&rows, at, t, cx));
    Some(
        deferred(
            anchored()
                .anchor(Anchor::BottomLeft)
                // The anchor starts under the item: lift the menu by the bar and a gap.
                .offset(point(px(0.0), px(-(BAR_H + 4.0))))
                .snap_to_window_with_margin(px(8.0))
                .child(panel),
        )
        .with_priority(2)
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            connected: true,
            demo: false,
            sync: Sync::Idle,
            synced_ago: Some("5 min ago".into()),
            provider: Some(Provider::Cloud),
            credits: Some(24),
            dark: true,
            palettes: vec![(Palette::Ink, true), (Palette::Moss, false)],
            mode: ThemeMode::System,
            update: UpdateFacts::default(),
            version: "1.2.3",
        }
    }

    fn acts(rows: &[Entry]) -> Vec<Act> {
        rows.iter().filter_map(|e| e.act().cloned()).collect()
    }

    #[test]
    fn an_idle_sync_menu_offers_both_syncs() {
        let rows = entries(MenuKind::Sync, &facts());
        assert_eq!(rows[0].label, "Last sync: 5 min ago");
        assert_eq!(
            acts(&rows),
            [Act::SyncNow, Act::FullSync, Act::GarminSettings]
        );
    }

    #[test]
    fn a_running_sync_shows_the_step_and_blocks_a_second_one() {
        let f = Facts {
            sync: Sync::Running("Reading sleep (2 of 4)".into()),
            ..facts()
        };
        let rows = entries(MenuKind::Sync, &f);
        assert_eq!(rows[0].label, "Reading sleep (2 of 4)");
        assert_eq!(acts(&rows), [Act::GarminSettings]);
        assert!(rows.iter().any(|e| e.mark == Mark::Spinner));
    }

    #[test]
    fn a_failed_sync_shows_the_error() {
        let (line, bad) = sync_line(&Sync::Failed("token expired".into()), None);
        assert!(bad && line.contains("token expired"));
        assert_eq!(sync_line(&Sync::Idle, None).0, "No sync yet.");
        let f = Facts {
            connected: false,
            ..facts()
        };
        assert_eq!(acts(&entries(MenuKind::Sync, &f)), [Act::GarminSettings]);
    }

    #[test]
    fn the_coach_menu_checks_the_current_provider() {
        let rows = entries(MenuKind::Coach, &facts());
        let checked: Vec<_> = rows.iter().filter(|e| e.mark == Mark::Check).collect();
        assert_eq!(checked.len(), 1);
        assert_eq!(checked[0].label, "Cloud");
        assert_eq!(checked[0].hint.as_deref(), Some("24 left"));
        assert_eq!(acts(&rows).len(), coach_providers().len());
        let own = Facts {
            provider: Some(Provider::Ollama),
            ..facts()
        };
        let rows = entries(MenuKind::Coach, &own);
        assert!(rows
            .iter()
            .any(|e| e.label == "Ollama" && e.mark == Mark::Check));
    }

    #[test]
    fn the_palette_menu_lists_the_mode_palettes_and_the_modes() {
        let rows = entries(MenuKind::Palette, &facts());
        assert_eq!(rows[0].label, "Dark palettes");
        let a = acts(&rows);
        assert_eq!(a[0], Act::Palette(Palette::Ink));
        assert!(a.contains(&Act::Mode(ThemeMode::Dark)));
        assert_eq!(a.last(), Some(&Act::Themes));
        let on: Vec<_> = rows
            .iter()
            .filter(|e| e.mark == Mark::Check)
            .map(|e| e.label.as_str())
            .collect();
        assert_eq!(on, ["Ink", "System"]);
    }

    #[test]
    fn the_version_menu_shows_a_result_and_an_offer() {
        let rows = entries(MenuKind::Version, &facts());
        assert_eq!(
            acts(&rows),
            [Act::CheckUpdates, Act::ReleaseNotes, Act::CopyVersion]
        );
        let f = Facts {
            update: UpdateFacts {
                busy: false,
                note: None,
                offer: Some(("2.0.0".into(), false)),
            },
            ..facts()
        };
        let rows = entries(MenuKind::Version, &f);
        assert!(rows
            .iter()
            .any(|e| e.label == "Version 2.0.0 is available."));
        assert!(acts(&rows).contains(&Act::Install));
        let demo = Facts {
            demo: true,
            ..facts()
        };
        assert!(!acts(&entries(MenuKind::Version, &demo)).contains(&Act::CheckUpdates));
    }

    #[test]
    fn the_keyboard_skips_notes_and_stops_at_the_ends() {
        let rows = entries(MenuKind::Sync, &facts());
        let first = step(&rows, None, true).unwrap();
        assert_eq!(rows[first].label, "Sync now");
        let second = step(&rows, Some(first), true).unwrap();
        assert_eq!(rows[second].label, "Full re-sync");
        let last = step(&rows, Some(second), true).unwrap();
        assert_eq!(rows[last].label, "Open Garmin settings");
        assert_eq!(step(&rows, Some(last), true), Some(last));
        assert_eq!(step(&rows, Some(first), false), Some(first));
        assert_eq!(step(&rows, None, false), Some(last));
    }

    #[test]
    fn the_release_page_names_the_tag() {
        assert_eq!(
            release_url("1.0.6"),
            "https://github.com/omznc/garmin-companion/releases/tag/v1.0.6"
        );
    }
}
