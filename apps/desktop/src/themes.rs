//! The saved palettes and the open-vsx theme store, without the drawing.
//!
//! Two palettes are saved: one for light mode and one for dark mode. A mode
//! picks between them. A VS Code theme from open-vsx is converted to our seven
//! colours and saved in the themes folder (`garmin_core::theme`). It then
//! appears in the picker of its own type. `appearance.rs` draws the picker and
//! `theme_dialog.rs` draws the store.

use std::collections::HashMap;

use garmin_api::settings;
use garmin_core::theme as folder;
use gpui::{App, Context, ScrollHandle, Window};

use crate::cloud::on_thread;
use crate::field::Field;
use crate::openvsx::{self, Found};
use crate::shell::Root;
use crate::theme::Palette;

/// What the left column of the dialog shows.
pub enum Phase {
    Idle,
    /// The text that is searched for.
    Searching(String),
    Empty,
    Failed(String),
    List,
}

/// The themes of one extension, from the time that a row is chosen or prefetched.
pub enum Fetch {
    Loading,
    Ready(Vec<folder::Theme>),
    Failed(String),
}

/// The state of the dialog. A new search starts a new `generation`: a late
/// answer of an old search is dropped.
pub struct Store {
    pub open: bool,
    pub phase: Phase,
    pub found: Vec<Found>,
    pub selected: usize,
    /// The themes of each extension, by `key_of`.
    pub fetched: HashMap<String, Fetch>,
    generation: u64,
    /// One background fetch at a time.
    prefetching: bool,
    /// The extension that is being saved.
    pub installing: Option<String>,
    /// The saved names of each extension that was installed in this session.
    pub installed: HashMap<String, Vec<String>>,
    /// The reason that the last install failed.
    pub failure: Option<String>,
    /// The result list, so that an arrow key can bring the chosen row into view.
    pub scroll: ScrollHandle,
}

/// How many rows from the top get a background fetch. A download can be 24 MB.
const PREFETCH_ROWS: usize = 8;

impl Store {
    /// Drop the answers still in flight, and keep the fetched themes.
    fn drop_pending(&mut self) {
        self.generation += 1;
        self.prefetching = false;
        self.fetched.retain(|_, f| matches!(f, Fetch::Ready(_)));
    }
}

/// The saved palettes, the themes folder and the state of the store.
pub struct Themes {
    pub light: Palette,
    pub dark: Palette,
    /// The themes folder, read at start and after an install or a delete.
    pub custom: Vec<folder::Theme>,
    pub search: Field,
    pub store: Store,
}

impl Themes {
    /// A demo reads the folder but not the saved choice, and saves nothing.
    pub fn new(demo: bool, cx: &mut App) -> Self {
        let custom = folder::list().unwrap_or_default();
        let prefs = if demo {
            settings::Prefs::default()
        } else {
            settings::prefs()
        };
        let mut search = Field::new("Search open-vsx, for example nord", false, cx);
        search.sans = true;
        search.pad_left = 30.0;
        search.flat = true;
        Self {
            light: saved(&prefs.theme_light, &custom, false),
            dark: saved(&prefs.theme_dark, &custom, true),
            custom,
            search,
            store: Store {
                open: false,
                phase: Phase::Idle,
                found: Vec::new(),
                selected: 0,
                fetched: HashMap::new(),
                generation: 0,
                prefetching: false,
                installing: None,
                installed: HashMap::new(),
                failure: None,
                scroll: ScrollHandle::new(),
            },
        }
    }
}

/// The palette for a saved id. A missing theme or one of the wrong type gives
/// Paper or Ink.
fn saved(id: &str, custom: &[folder::Theme], dark: bool) -> Palette {
    Palette::from_id(id, custom)
        .filter(|p| p.is_dark() == dark)
        .unwrap_or(if dark { Palette::Ink } else { Palette::Paper })
}

/// Every palette of one type: the built-in ones, then the custom ones.
pub fn palettes(custom: &[folder::Theme], dark: bool) -> Vec<Palette> {
    let built_in = Palette::ALL.into_iter().filter(|p| p.is_dark() == dark);
    let custom = custom
        .iter()
        .filter_map(Palette::custom)
        .filter(|p| p.is_dark() == dark);
    built_in.chain(custom).collect()
}

/// The prefix of a custom palette id. The rest is the slug in the themes folder.
pub const CUSTOM_ID: &str = "custom:";

/// The cache key of an extension.
pub fn key_of(found: &Found) -> String {
    format!("{}.{}", found.namespace, found.name)
}

/// Which palettes the themes of an extension go to: `(light, dark)`.
fn modes(themes: &[folder::Theme]) -> (bool, bool) {
    let dark = |t: &&folder::Theme| t.appearance == folder::Appearance::Dark;
    (
        themes.iter().any(|t| !dark(&t)),
        themes.iter().any(|t| dark(&t)),
    )
}

/// The short mark of a result row: "Dark", "Light" or both.
pub fn mode_tag(themes: &[folder::Theme]) -> &'static str {
    match modes(themes) {
        (true, true) => "Light + Dark",
        (true, false) => "Light",
        _ => "Dark",
    }
}

/// Where an install puts the themes. It follows what `openvsx::save` does.
pub fn goes_to(themes: &[folder::Theme]) -> &'static str {
    match modes(themes) {
        (true, true) => "light and dark palettes",
        (true, false) => "light palette",
        _ => "dark palette",
    }
}

pub fn downloads(count: u64) -> String {
    match count {
        0..=999 => format!("{count} installs"),
        1000..=999_999 => format!("{}k installs", count / 1000),
        _ => format!("{}M installs", count / 1_000_000),
    }
}

impl Root {
    /// Take the palette that the mode and the window pick.
    pub(crate) fn apply_palette(&mut self, _window: &Window, cx: &mut Context<Self>) {
        self.refresh_palette(cx);
    }

    /// Like `apply_palette`, for code that has no window at hand.
    fn refresh_palette(&mut self, cx: &mut Context<Self>) {
        let themes = &self.account.themes;
        self.palette = crate::account::palette_for(
            self.account.mode,
            cx.window_appearance(),
            themes.light,
            themes.dark,
        );
        let palette = self.palette;
        self.ask.update(cx, |ask, cx| ask.set_palette(palette, cx));
    }

    /// Save the two palettes, except in a demo.
    fn save_palettes(&mut self) {
        if self.account.demo {
            return;
        }
        let themes = &self.account.themes;
        if let Err(e) = settings::set_palettes(themes.light.id(), themes.dark.id()) {
            eprintln!("could not save the palettes: {e}");
        }
    }

    /// Choose a palette for its own type: a dark one is the dark palette.
    pub(crate) fn choose_palette(
        &mut self,
        palette: Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let themes = &mut self.account.themes;
        if palette.is_dark() {
            themes.dark = palette;
        } else {
            themes.light = palette;
        }
        self.save_palettes();
        self.apply_palette(window, cx);
        cx.notify();
    }

    pub(crate) fn delete_theme(&mut self, slug: String, window: &Window, cx: &mut Context<Self>) {
        let themes = &mut self.account.themes;
        if let Err(e) = folder::delete(&slug) {
            eprintln!("could not delete the theme: {e:#}");
            return cx.notify();
        }
        // An extension with a deleted theme is no longer installed.
        themes.store.installed.retain(|_, names| {
            names
                .iter()
                .all(|n| folder::slugify(n.trim_end_matches(" (replaced)")) != slug)
        });
        themes.custom = folder::list().unwrap_or_default();
        let (light, dark) = (themes.light.id(), themes.dark.id());
        themes.light = saved(&light, &themes.custom, false);
        themes.dark = saved(&dark, &themes.custom, true);
        self.save_palettes();
        self.apply_palette(window, cx);
        cx.notify();
    }

    /// Open the dialog. A text starts a search at once.
    pub fn open_themes(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let themes = &mut self.account.themes;
        themes.store.open = true;
        if !query.is_empty() {
            themes.search.line.clear();
            themes.search.line.insert(query);
        }
        themes.search.focus.focus(window, cx);
        if !query.is_empty() {
            self.search_themes(cx);
        }
        cx.notify();
    }

    pub(crate) fn close_themes(&mut self, cx: &mut Context<Self>) {
        let store = &mut self.account.themes.store;
        store.open = false;
        store.drop_pending();
        cx.notify();
    }

    pub(crate) fn search_themes(&mut self, cx: &mut Context<Self>) {
        let themes = &mut self.account.themes;
        let query = themes.search.line.text().trim().to_string();
        if query.is_empty() {
            return;
        }
        let store = &mut themes.store;
        // The old results and the fetches in flight belong to the old generation.
        // A fetched extension stays in the cache for the next search.
        store.drop_pending();
        store.phase = Phase::Searching(query.clone());
        store.found.clear();
        store.selected = 0;
        store.failure = None;
        let job = async move { openvsx::search(&query).await.map_err(|e| format!("{e:#}")) };
        let generation = store.generation;
        self.store_call(cx, Some(generation), job, |root, out, cx| {
            let store = &mut root.account.themes.store;
            match out {
                Ok(found) if found.is_empty() => store.phase = Phase::Empty,
                Ok(found) => {
                    store.found = found;
                    store.phase = Phase::List;
                }
                Err(e) => store.phase = Phase::Failed(e),
            }
            if matches!(store.phase, Phase::List) {
                root.select_result(0, cx);
            }
        });
        cx.notify();
    }

    /// Move the choice by one row, around the ends.
    pub(crate) fn step_result(&mut self, down: bool, cx: &mut Context<Self>) {
        let store = &self.account.themes.store;
        let len = store.found.len();
        if len == 0 || !matches!(store.phase, Phase::List) {
            return;
        }
        let at = if down {
            (store.selected + 1) % len
        } else {
            (store.selected + len - 1) % len
        };
        self.select_result(at, cx);
    }

    /// Choose a row and get its themes, if they are not here yet.
    pub(crate) fn select_result(&mut self, index: usize, cx: &mut Context<Self>) {
        let store = &mut self.account.themes.store;
        let Some(found) = store.found.get(index) else {
            return;
        };
        store.selected = index;
        store.failure = None;
        // The first row of the list is the eyebrow.
        store.scroll.scroll_to_item(index + 1);
        // A failed fetch tries again when the row is chosen again.
        if matches!(store.fetched.get(&key_of(found)), Some(Fetch::Failed(_))) {
            store.fetched.remove(&key_of(found));
        }
        self.fetch_themes(index, false, cx);
        cx.notify();
    }

    /// Fetch the themes of a row. `background` is the one prefetch at a time.
    fn fetch_themes(&mut self, index: usize, background: bool, cx: &mut Context<Self>) {
        let store = &mut self.account.themes.store;
        let Some(found) = store.found.get(index).cloned() else {
            return;
        };
        let key = key_of(&found);
        if store.fetched.contains_key(&key) {
            return self.prefetch_next(cx);
        }
        store.fetched.insert(key.clone(), Fetch::Loading);
        store.prefetching |= background;
        let generation = store.generation;
        let job = async move {
            openvsx::fetch_themes(&found)
                .await
                .map_err(|e| format!("{e:#}"))
        };
        self.store_call(cx, Some(generation), job, move |root, out, cx| {
            let store = &mut root.account.themes.store;
            store.prefetching &= !background;
            store.fetched.insert(
                key,
                match out {
                    Ok(themes) => Fetch::Ready(themes),
                    Err(e) => Fetch::Failed(e),
                },
            );
            root.prefetch_next(cx);
        });
    }

    /// Start the fetch for the first shown row that has none.
    fn prefetch_next(&mut self, cx: &mut Context<Self>) {
        let store = &self.account.themes.store;
        if store.prefetching {
            return;
        }
        let next = store
            .found
            .iter()
            .take(PREFETCH_ROWS)
            .position(|f| !store.fetched.contains_key(&key_of(f)));
        if let Some(index) = next {
            self.fetch_themes(index, true, cx);
        }
    }

    /// Save the themes of the chosen row. They are in the cache already.
    pub(crate) fn install_selected(&mut self, cx: &mut Context<Self>) {
        let store = &mut self.account.themes.store;
        let Some(found) = store.found.get(store.selected) else {
            return;
        };
        let key = key_of(found);
        let Some(Fetch::Ready(themes)) = store.fetched.get(&key) else {
            return;
        };
        if store.installing.is_some() {
            return;
        }
        let themes = themes.clone();
        store.installing = Some(key.clone());
        store.failure = None;
        let job = async move { openvsx::save(themes).map_err(|e| format!("{e:#}")) };
        // A new search must not lose the answer: the key still says what it was.
        self.store_call(cx, None, job, move |root, out, cx| {
            let themes = &mut root.account.themes;
            themes.store.installing = None;
            match out {
                Ok(names) => {
                    themes.store.installed.insert(key, names);
                    themes.custom = folder::list().unwrap_or_default();
                    // The install may have replaced the active custom palette.
                    let (light, dark) = (themes.light.id(), themes.dark.id());
                    let light = saved(&light, &themes.custom, false);
                    let dark = saved(&dark, &themes.custom, true);
                    if (light, dark) != (themes.light, themes.dark) {
                        themes.light = light;
                        themes.dark = dark;
                        root.refresh_palette(cx);
                    }
                }
                Err(e) => themes.store.failure = Some(e),
            }
        });
        cx.notify();
    }

    /// Run a network or disk job off the main thread, then give the result to
    /// `done`. With a `generation`, an answer of an older search is dropped.
    fn store_call<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        generation: Option<u64>,
        job: impl std::future::Future<Output = Result<T, String>> + Send + 'static,
        done: impl FnOnce(&mut Root, Result<T, String>, &mut Context<Root>) + 'static,
    ) {
        cx.spawn(async move |this, cx| {
            let out = on_thread(job).await;
            let _ = this.update(cx, |root, cx| {
                let current = root.account.themes.store.generation;
                if generation.is_some_and(|g| g != current) {
                    return;
                }
                done(root, out, cx);
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(slug: &str, dark: bool) -> folder::Theme {
        folder::Theme {
            slug: slug.into(),
            name: slug.into(),
            appearance: if dark {
                folder::Appearance::Dark
            } else {
                folder::Appearance::Light
            },
            note: String::new(),
            colors: folder::Colors {
                bg: "#101010".into(),
                bg2: "#181818".into(),
                fg: "#eeeeee".into(),
                muted: "#aaaaaa".into(),
                faint: "#777777".into(),
                acc: "#88aaff".into(),
                warn: "#ccaa55".into(),
            },
            icon_tint_alpha: None,
        }
    }

    #[test]
    fn a_saved_id_gives_a_palette_of_the_right_type_or_the_default() {
        let list = [custom("nord", true), custom("day", false)];
        assert_eq!(saved("", &list, false), Palette::Paper);
        assert_eq!(saved("", &list, true), Palette::Ink);
        assert_eq!(saved("linen", &list, false), Palette::Linen);
        assert_eq!(saved("moss", &list, false), Palette::Paper, "wrong type");
        assert_eq!(saved("custom:gone", &list, true), Palette::Ink, "deleted");
        let nord = saved("custom:nord", &list, true);
        assert_eq!(nord.id(), "custom:nord");
        assert_eq!(nord.name(), "nord");
        assert!(nord.is_dark());
    }

    #[test]
    fn a_picker_lists_built_in_then_custom_of_its_own_type() {
        let list = [custom("nord", true), custom("day", false)];
        let names =
            |dark| -> Vec<&str> { palettes(&list, dark).iter().map(|p| p.name()).collect() };
        assert_eq!(names(false), ["Paper", "Newsprint", "Linen", "day"]);
        assert_eq!(names(true), ["Ink", "Nocturne", "Moss", "nord"]);
    }

    #[test]
    fn install_counts_are_short() {
        assert_eq!(downloads(12), "12 installs");
        assert_eq!(downloads(4_200), "4k installs");
        assert_eq!(downloads(4_200_000), "4M installs");
    }

    #[test]
    fn the_tag_and_the_target_follow_the_modes_of_the_themes() {
        let dark = [custom("a", true), custom("b", true)];
        assert_eq!((mode_tag(&dark), goes_to(&dark)), ("Dark", "dark palette"));
        let light = [custom("a", false)];
        assert_eq!(
            (mode_tag(&light), goes_to(&light)),
            ("Light", "light palette")
        );
        let both = [custom("a", true), custom("b", false)];
        assert_eq!(mode_tag(&both), "Light + Dark");
        assert_eq!(goes_to(&both), "light and dark palettes");
    }
}
