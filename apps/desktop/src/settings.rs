//! The Settings screen: a search field, a short list of sections at the left
//! and the rows of the chosen section at the right, as in the settings of Zed.
//!
//! A row is a label with one muted line under it, and its controls at the
//! right. A search shows the rows of every section that match the words. A
//! change applies at once.

use crate::scale::px;
use garmin_api::settings::SettingsView;
use gpui::{canvas, div, prelude::*, AnyElement, ClickEvent, Context, Div, Stateful, Window};

use crate::account::Sync;
use crate::icons::Icon;
use crate::panel::{self, Kind, Tone};
use crate::shell::Root;
use crate::sidebar::SIDEBAR_W;
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

/// At or below this content width the list of sections stands over the rows.
const NARROW_AT: f32 = 640.0;

/// The width of the list of sections.
const NAV_W: f32 = 148.0;

/// The sections, in the order of the list.
const SECTIONS: [&str; 5] = ["Garmin account", "Coach", "Appearance", "Updates", "About"];

/// The width of the content for a window `viewport` pixels wide.
fn content_width(viewport: f32) -> f32 {
    // The page pads 20 px on each side.
    (viewport - SIDEBAR_W - 2.0 * 20.0).min(crate::shell::PAGE_MAX)
}

/// A row, with the words that the search matches.
type Item = (String, Div);

fn sync_note(root: &Root, t: &Theme) -> Option<Div> {
    match &root.account.sync {
        Sync::Idle => None,
        Sync::Running(line) => Some(panel::note(Tone::Run, line.clone(), t)),
        Sync::Done(line) => Some(panel::note(Tone::Ok, line.clone(), t)),
        Sync::Failed(error) => Some(panel::note(Tone::Err, error.clone(), t)),
    }
}

fn connection_row(view: &SettingsView, t: &Theme) -> Item {
    let line = format!("{} {}", view.account_line, view.cache_line);
    (
        format!("connection garmin account tokens {line}"),
        panel::row(true, "Connection", Some(&line), div(), t),
    )
}

fn folder_row(folder: &str, t: &Theme) -> Item {
    // A path has no break points. A long one ends in an ellipsis.
    let path = panel::help(folder.to_string(), t)
        .font_family(MONO)
        .whitespace_nowrap()
        .overflow_hidden()
        .text_ellipsis();
    (
        format!("data folder database cache {folder}"),
        panel::row_with(false, "Data folder", Some(path), div(), t),
    )
}

fn sync_row(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Item {
    let busy = root.account.syncing();
    let (label, lead) = if busy {
        ("Syncing...", Some(Icon::Spinner))
    } else {
        ("Sync recent", None)
    };
    let buttons = panel::actions()
        .child(
            panel::button("sync-recent", label, Kind::Primary, !busy, lead, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.start_sync(false, cx))),
        )
        .child(
            panel::button("sync-full", "Full re-sync", Kind::Plain, !busy, None, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.start_sync(true, cx))),
        );
    let mut value = panel::stack().child(buttons);
    if let Some(note) = sync_note(root, t) {
        value = value.child(note);
    }
    let help = "Recent brings in the last days. A full re-sync reads all of your history again.";
    (
        "sync recent full re-sync history".to_string(),
        panel::row(false, "Sync", Some(help), value, t),
    )
}

fn disconnect_row(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Item {
    let busy = root.account.syncing();
    let value = panel::stack().child(
        panel::button(
            "sign-out",
            "Disconnect from Garmin",
            Kind::Danger,
            !busy,
            Some(Icon::SignOut),
            t,
        )
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.sign_out(cx))),
    );
    let help = "Signs this computer out of Garmin. To sync again, you must connect again.";
    (
        "disconnect sign out garmin".to_string(),
        panel::row(false, "Disconnect", Some(help), value, t),
    )
}

fn garmin_rows(root: &Root, view: &SettingsView, t: &Theme, cx: &mut Context<Root>) -> Vec<Item> {
    let mut rows = vec![connection_row(view, t)];
    if let Some(folder) = &view.data_folder {
        rows.push(folder_row(folder, t));
    }
    rows.push(sync_row(root, t, cx));
    rows.push(disconnect_row(root, t, cx));
    rows
}

fn updates_rows(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Vec<Item> {
    vec![(
        "updates check for updates version release".to_string(),
        crate::update_ui::settings_row(root, t, cx),
    )]
}

fn about_rows(view: &SettingsView, t: &Theme) -> Vec<Item> {
    let text = "Garmin Companion reads your Garmin data into a database on this computer. \
                You can delete that file at any time. The app is open source.";
    let version = format!("Version {}", view.version);
    vec![
        (
            format!("about garmin companion open source {version}"),
            panel::row(false, "Garmin Companion", Some(text), div(), t),
        ),
        (
            format!("what leaves this machine privacy {}", view.privacy_line),
            panel::row(
                false,
                "What leaves this machine",
                Some(&view.privacy_line),
                div(),
                t,
            ),
        ),
    ]
}

fn coach_rows(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Vec<Item> {
    crate::cloud::rows(root, t, window, cx)
        .into_iter()
        .map(|(words, row)| (words.to_string(), row))
        .collect()
}

fn appearance_rows(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Vec<Item> {
    crate::appearance::rows(root, t, cx)
        .into_iter()
        .map(|(words, row)| (words.to_string(), row))
        .collect()
}

/// The rows of the section at `at`.
fn rows_of(
    at: usize,
    root: &Root,
    view: &SettingsView,
    t: &Theme,
    window: &Window,
    cx: &mut Context<Root>,
) -> Vec<Item> {
    match at {
        0 => garmin_rows(root, view, t, cx),
        1 => coach_rows(root, t, window, cx),
        2 => appearance_rows(root, t, cx),
        3 => updates_rows(root, t, cx),
        _ => about_rows(view, t),
    }
}

/// `.nav`: one entry of the list of sections.
fn nav_item(at: usize, on: bool, t: &Theme, cx: &mut Context<Root>) -> Stateful<Div> {
    let (hover, fg) = (t.hover, t.fg);
    div()
        .id(("settings-section", at))
        .flex_none()
        .flex()
        .items_center()
        .h(px(26.0))
        .px(px(8.0))
        .rounded(px(4.0))
        .cursor_pointer()
        .font_family(SANS)
        .text_size(px(12.5))
        .whitespace_nowrap()
        .text_color(if on { t.fg } else { t.mutd })
        .when(on, |d| d.bg(t.active))
        .when(!on, |d| d.hover(move |s| s.bg(hover).text_color(fg)))
        .child(SECTIONS[at])
        .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| {
            root.account.settings_page = at;
            let tops = root.account.settings_tops.borrow();
            if let (Some(first), Some(top)) = (tops.first(), tops.get(at)) {
                root.scroll.scroll_to(top - first);
            }
            cx.notify();
        }))
}

fn nav(root: &Root, narrow: bool, searching: bool, t: &Theme, cx: &mut Context<Root>) -> Div {
    let list = div().flex().gap(px(2.0));
    let mut list = if narrow {
        list.flex_wrap()
    } else {
        list.flex_col()
    };
    for at in 0..SECTIONS.len() {
        let on = !searching && root.account.settings_page == at;
        list = list.child(nav_item(at, on, t, cx));
    }
    list
}

/// A section: its label and its rows.
fn section(title: &str, rows: Vec<Div>, t: &Theme) -> Div {
    rows.into_iter()
        .fold(panel::section(title, None, t), |s, r| s.child(r))
        .pb(px(14.0))
}

/// A section with a marker that records the window y of its top.
fn marked(at: usize, body: Div, root: &Root) -> Div {
    let tops = root.account.settings_tops.clone();
    let marker = canvas(
        |_, _, _| (),
        move |bounds, (), _, _| {
            if let Some(top) = tops.borrow_mut().get_mut(at) {
                *top = f32::from(bounds.origin.y);
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_0();
    div().relative().child(marker).child(body)
}

/// The right side: the chosen section, or the rows that match the search.
fn content(
    root: &Root,
    view: &SettingsView,
    query: &str,
    t: &Theme,
    window: &Window,
    cx: &mut Context<Root>,
) -> Div {
    let mut col = div().flex().flex_col().flex_1().min_w_0();
    if query.is_empty() {
        root.account
            .settings_tops
            .borrow_mut()
            .resize(SECTIONS.len(), 0.0);
        for (at, title) in SECTIONS.iter().enumerate() {
            let rows = rows_of(at, root, view, t, window, cx);
            let body = section(title, rows.into_iter().map(|(_, r)| r).collect(), t);
            col = col.child(marked(at, body, root));
        }
        return col;
    }
    let mut found = false;
    for (at, title) in SECTIONS.iter().enumerate() {
        let rows: Vec<Div> = rows_of(at, root, view, t, window, cx)
            .into_iter()
            .filter(|(words, _)| words.to_lowercase().contains(query))
            .map(|(_, r)| r)
            .collect();
        if !rows.is_empty() {
            found = true;
            col = col.child(section(title, rows, t));
        }
    }
    if !found {
        col = col.child(
            div()
                .py(px(12.0))
                .text_size(px(12.5))
                .text_color(t.faint)
                .child("No setting matches that search."),
        );
    }
    col
}

pub fn render(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> AnyElement {
    let Some(view) = root.account.settings.clone() else {
        return div().into_any_element();
    };
    let narrow = content_width(crate::scale::viewport_w(window)) <= NARROW_AT;
    panel::set_narrow(narrow);
    let query = root
        .account
        .settings_search
        .line
        .text()
        .trim()
        .to_lowercase();
    let search = root.account.settings_search.render(
        "settings-search",
        t,
        cx,
        window,
        |root: &mut Root| &mut root.account.settings_search,
        |_, _, _| {},
    );
    let title = div()
        .text_size(px(20.0))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child("Settings");
    let nav = nav(root, narrow, !query.is_empty(), t, cx);
    let content = content(root, &view, &query, t, window, cx);
    if narrow {
        let header = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(title)
            .child(div().w(px(280.0)).max_w_full().child(search));
        return div()
            .flex()
            .flex_col()
            .max_w(px(860.0))
            .gap(px(14.0))
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(nav)
                    .child(content),
            )
            .into_any_element();
    }
    // The page scrolls, so the title and the list follow the offset and stay
    // in view. This is a measured offset in window pixels, not a design size.
    let side = div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .w(px(NAV_W))
        .flex_none()
        .relative()
        .top(gpui::px(-root.scroll.offset_y()))
        .child(title)
        .child(nav);
    let main = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(px(14.0))
        .child(div().w(px(280.0)).max_w_full().child(search))
        .child(content);
    div()
        .flex()
        .max_w(px(860.0))
        .gap(px(24.0))
        .child(side)
        .child(main)
        .into_any_element()
}
