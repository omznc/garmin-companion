//! The palette pickers and the open-vsx theme store in Settings.
//!
//! Two palettes are saved: one for light mode and one for dark mode. A mode
//! picks between them. A VS Code theme from open-vsx is converted to our seven
//! colours and saved in the themes folder (`garmin_core::theme`). It then
//! appears in the picker of its own type.

use garmin_api::settings;
use garmin_core::theme as folder;
use gpui::{
    div, prelude::*, px, relative, App, ClickEvent, Context, Div, ElementId, FontWeight,
    SharedString, Window,
};

use crate::cloud::on_thread;
use crate::controls::quiet;
use crate::field::Field;
use crate::openvsx::{self, Found};
use crate::shell::Root;
use crate::theme::{Palette, Theme, FS_CAPTION, FS_SMALL};
use crate::ui::{body, ch};

/// The saved palettes, the themes folder and the state of the store.
pub struct Themes {
    pub light: Palette,
    pub dark: Palette,
    /// The themes folder, read at start and after an install or a delete.
    pub custom: Vec<folder::Theme>,
    pub search: Field,
    pub found: Vec<Found>,
    pub busy: bool,
    /// The last message and whether it is a failure.
    pub note: Option<(String, bool)>,
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
        Self {
            light: saved(&prefs.theme_light, &custom, false),
            dark: saved(&prefs.theme_dark, &custom, true),
            custom,
            search: Field::new("Search open-vsx, for example nord", false, cx),
            found: Vec::new(),
            busy: false,
            note: None,
        }
    }

    fn say(&mut self, text: impl Into<String>, bad: bool) {
        self.note = Some((text.into(), bad));
    }
}

/// The palette for a saved id. A missing theme or one of the wrong type gives
/// Paper or Ink.
fn saved(id: &str, custom: &[folder::Theme], dark: bool) -> Palette {
    Palette::from_id(id, custom)
        .filter(|p| p.is_dark() == dark)
        .unwrap_or(if dark { Palette::Ink } else { Palette::Paper })
}

impl Root {
    /// Take the palette that the mode and the window pick.
    pub(crate) fn apply_palette(&mut self, window: &Window, cx: &mut Context<Self>) {
        let themes = &self.account.themes;
        self.palette = crate::account::palette_for(
            self.account.mode,
            window.appearance(),
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
    fn choose_palette(&mut self, palette: Palette, window: &Window, cx: &mut Context<Self>) {
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

    fn search_themes(&mut self, cx: &mut Context<Self>) {
        let themes = &mut self.account.themes;
        let query = themes.search.line.text().trim().to_string();
        if query.is_empty() || themes.busy {
            return;
        }
        themes.busy = true;
        themes.note = None;
        let job = async move { openvsx::search(&query).await.map_err(|e| format!("{e:#}")) };
        self.themes_call(cx, job, |themes, out| match out {
            Ok(found) if found.is_empty() => {
                themes.say("No theme matches that search.", false);
                themes.found.clear();
            }
            Ok(found) => themes.found = found,
            Err(e) => themes.say(e, true),
        });
    }

    fn install_theme(&mut self, found: Found, cx: &mut Context<Self>) {
        let themes = &mut self.account.themes;
        if themes.busy {
            return;
        }
        themes.busy = true;
        themes.say(format!("Installing {}.", found.display), false);
        let job = async move { openvsx::install(&found).await.map_err(|e| format!("{e:#}")) };
        self.themes_call(cx, job, |themes, out| match out {
            Ok(names) => {
                themes.custom = folder::list().unwrap_or_default();
                themes.say(
                    format!("Installed {}. Pick it above.", names.join(", ")),
                    false,
                );
            }
            Err(e) => themes.say(e, true),
        });
    }

    /// Run a network job, then give the result to `done`. The store is busy meanwhile.
    fn themes_call<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        job: impl std::future::Future<Output = Result<T, String>> + Send + 'static,
        done: impl FnOnce(&mut Themes, Result<T, String>) + 'static,
    ) {
        cx.notify();
        cx.spawn(async move |this, cx| {
            let out = on_thread(job).await;
            let _ = this.update(cx, |root, cx| {
                let themes = &mut root.account.themes;
                themes.busy = false;
                done(themes, out);
                cx.notify();
            });
        })
        .detach();
    }

    fn delete_theme(&mut self, slug: String, window: &Window, cx: &mut Context<Self>) {
        let themes = &mut self.account.themes;
        if let Err(e) = folder::delete(&slug) {
            themes.say(format!("Could not delete the theme: {e:#}"), true);
            return cx.notify();
        }
        themes.custom = folder::list().unwrap_or_default();
        let (light, dark) = (themes.light.id(), themes.dark.id());
        themes.light = saved(&light, &themes.custom, false);
        themes.dark = saved(&dark, &themes.custom, true);
        self.save_palettes();
        self.apply_palette(window, cx);
        cx.notify();
    }
}

/// Every palette of one type: the built-in ones, then the custom ones.
fn palettes(custom: &[folder::Theme], dark: bool) -> Vec<Palette> {
    let built_in = Palette::ALL.into_iter().filter(|p| p.is_dark() == dark);
    let custom = custom
        .iter()
        .filter_map(Palette::custom)
        .filter(|p| p.is_dark() == dark);
    built_in.chain(custom).collect()
}

fn element_id(prefix: &str, key: &str) -> ElementId {
    ElementId::from(SharedString::from(format!("{prefix}-{key}")))
}

/// One row of choices for light or for dark.
fn picker(root: &Root, label: &str, dark: bool, t: &Theme, cx: &mut Context<Root>) -> Div {
    let themes = &root.account.themes;
    let current = if dark { themes.dark } else { themes.light };
    let mut row = div()
        .flex()
        .flex_wrap()
        .items_baseline()
        .gap_x(px(26.0))
        .gap_y(px(6.0));
    for palette in palettes(&themes.custom, dark) {
        // A saved id names the palette. Two custom themes never share one.
        let on = palette.id() == current.id();
        let item = quiet(
            element_id("palette", &palette.id()),
            palette.name(),
            true,
            false,
            t,
        )
        .text_color(if on { t.fg } else { t.faint })
        .when(on, |d| d.font_weight(FontWeight::MEDIUM))
        .on_click(cx.listener(move |root, _: &ClickEvent, window, cx| {
            root.choose_palette(palette, window, cx)
        }));
        row = row.child(item);
    }
    div()
        .mt(px(22.0))
        .child(body(label.to_string(), FS_SMALL, t.mutd).mb(px(8.0)))
        .child(row)
}

/// The themes that the user installed, each with a delete.
fn installed(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Option<Div> {
    let custom = &root.account.themes.custom;
    if custom.is_empty() {
        return None;
    }
    let mut list = div().mt(px(8.0)).flex().flex_col().gap(px(6.0));
    for theme in custom {
        let slug = theme.slug.clone();
        let kind = match theme.appearance {
            folder::Appearance::Dark => "dark",
            folder::Appearance::Light => "light",
        };
        list = list.child(
            div()
                .flex()
                .items_baseline()
                .gap(px(14.0))
                .max_w(ch(62.0, FS_SMALL))
                .child(body(theme.name.clone(), FS_SMALL, t.fg))
                .child(body(kind, FS_CAPTION, t.faint))
                .child(
                    quiet(element_id("theme-delete", &slug), "Delete", true, true, t)
                        .ml_auto()
                        .on_click(cx.listener(move |root, _: &ClickEvent, window, cx| {
                            root.delete_theme(slug.clone(), window, cx)
                        })),
                ),
        );
    }
    Some(
        div()
            .mt(px(22.0))
            .child(body("Your themes".to_string(), FS_SMALL, t.mutd))
            .child(list),
    )
}

fn downloads(count: u64) -> String {
    match count {
        0..=999 => format!("{count} installs"),
        1000..=999_999 => format!("{}k installs", count / 1000),
        _ => format!("{}M installs", count / 1_000_000),
    }
}

/// One search result: the name, the install count and an install action.
fn result_row(found: &Found, busy: bool, t: &Theme, cx: &mut Context<Root>) -> Div {
    let pick = found.clone();
    let key = format!("{}.{}", found.namespace, found.name);
    div()
        .flex()
        .items_baseline()
        .gap(px(14.0))
        .max_w(ch(62.0, FS_SMALL))
        .child(
            body(found.display.clone(), FS_SMALL, t.fg)
                .min_w_0()
                .truncate(),
        )
        .child(body(downloads(found.downloads), FS_CAPTION, t.faint).flex_none())
        .child(
            quiet(
                element_id("theme-install", &key),
                "Install",
                !busy,
                false,
                t,
            )
            .ml_auto()
            .flex_none()
            .on_click(
                cx.listener(move |root, _: &ClickEvent, _, cx| {
                    root.install_theme(pick.clone(), cx)
                }),
            ),
        )
}

/// The search field, its results and the last message.
fn store(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let themes = &root.account.themes;
    let mut block = div()
        .mt(px(30.0))
        .child(body("Get a VS Code theme".to_string(), FS_SMALL, t.mutd))
        .child(
            body(
                "Search open-vsx.org. A theme is saved on this computer as seven colours.",
                FS_CAPTION,
                t.faint,
            )
            .mt(px(4.0))
            .mb(px(10.0)),
        )
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(px(20.0))
                .max_w(ch(62.0, FS_SMALL))
                .child(div().flex_1().child(themes.search.render(
                    "theme-search",
                    t,
                    cx,
                    window,
                    |root: &mut Root| &mut root.account.themes.search,
                    |root, _, cx| root.search_themes(cx),
                )))
                .child(
                    quiet("theme-search-go", "Search", !themes.busy, false, t).on_click(
                        cx.listener(|root, _: &ClickEvent, _, cx| root.search_themes(cx)),
                    ),
                ),
        );
    if !themes.found.is_empty() {
        let mut list = div().mt(px(14.0)).flex().flex_col().gap(px(6.0));
        for found in &themes.found {
            list = list.child(result_row(found, themes.busy, t, cx));
        }
        block = block.child(list);
    }
    if let Some((text, bad)) = &themes.note {
        let colour = if *bad { t.warn } else { t.mutd };
        block = block.child(
            body(text.clone(), FS_CAPTION, colour)
                .mt(px(12.0))
                .line_height(relative(1.6))
                .max_w(ch(62.0, FS_CAPTION)),
        );
    }
    block
}

/// The palettes and the store, below the light and dark switch.
pub fn render(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let mut out = div()
        .child(picker(root, "Light palette", false, t, cx))
        .child(picker(root, "Dark palette", true, t, cx));
    if let Some(list) = installed(root, t, cx) {
        out = out.child(list);
    }
    out.child(store(root, t, window, cx))
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
}
