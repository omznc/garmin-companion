//! The Appearance section of Settings: the mode, the two palettes and the store.
//!
//! The markup follows the Appearance part of the Settings design.

use garmin_api::settings::ThemeMode;
use gpui::{
    div, prelude::*, px, BoxShadow, ClickEvent, Context, Div, ElementId, FontWeight, Hsla,
    SharedString, Stateful,
};

use crate::icons::{icon, Icon};
use crate::panel::{self, Kind};
use crate::shell::Root;
use crate::theme::{Palette, Theme};
use crate::themes::{palettes, CUSTOM_ID};
use crate::ui::body;

fn element_id(prefix: &str, key: &str) -> ElementId {
    ElementId::from(SharedString::from(format!("{prefix}-{key}")))
}

/// The cell of a palette in the grid.
const CELL_W: f32 = 132.0;

/// A one pixel ring inside the border, for a chosen card. It makes the border
/// look 2px thick and leaves the size of the card as it is.
pub fn ring(colour: Hsla) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: colour,
        offset: gpui::point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(1.0),
        inset: true,
    }]
}

fn mode_row(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
    let choices = [
        (
            "theme-system",
            "Match system",
            Icon::Desktop,
            ThemeMode::System,
        ),
        ("theme-light", "Light", Icon::Light, ThemeMode::Light),
        ("theme-dark", "Dark", Icon::Dark, ThemeMode::Dark),
    ];
    let mut control = panel::seg(t);
    for (id, label, lead, mode) in choices {
        let on = root.account.mode == mode;
        control = control.child(panel::seg_item(id, label, on, Some(lead), t).on_click(
            cx.listener(move |root, _: &ClickEvent, window, cx| {
                root.set_theme_mode(mode, window, cx)
            }),
        ));
    }
    let help = "Match system follows the setting of your desktop.";
    panel::row(true, "Mode", Some(help), panel::stack().child(control), t)
}

/// `.mini`: a small picture of a palette in its own colours.
fn mini(p: &Theme) -> Div {
    let bar = |share: f32, h: f32, colour: Hsla| {
        div()
            .w(gpui::relative(share))
            .h(px(h))
            .flex_none()
            .rounded(px(2.0))
            .bg(colour)
    };
    let dot = |colour: Hsla| div().size(px(10.0)).rounded_full().bg(colour);
    let nav = div()
        .w(gpui::relative(0.28))
        .flex_none()
        .px(px(6.0))
        .py(px(8.0))
        .flex()
        .flex_col()
        .gap(px(5.0))
        .bg(p.bg2)
        .child(bar(0.8, 4.0, p.faint))
        .child(bar(0.6, 4.0, p.faint))
        .child(bar(0.7, 4.0, p.acc));
    let page = div()
        .flex_1()
        .p(px(9.0))
        .flex()
        .flex_col()
        .gap(px(5.0))
        .child(bar(0.62, 6.0, p.fg))
        .child(bar(0.88, 4.0, p.mutd))
        .child(bar(0.74, 4.0, p.mutd))
        .child(
            div()
                .mt_auto()
                .flex()
                .gap(px(4.0))
                .child(dot(p.acc))
                .child(dot(p.warn)),
        );
    div()
        .h(px(64.0))
        .flex()
        .rounded(px(7.0))
        .overflow_hidden()
        .border_1()
        .border_color(gpui::hsla(0.0, 0.0, 0.5, 0.18))
        .bg(p.bg)
        .child(nav)
        .child(page)
}

/// `.badge`: the mark on the chosen palette. A ring in the card colour sets it off.
fn badge(t: &Theme) -> Div {
    div()
        .absolute()
        .top(px(10.0))
        .left(px(10.0))
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(t.bg)
        .child(
            div()
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(t.acc)
                .child(icon(Icon::Check, 11.0, t, t.bg)),
        )
}

/// `.x`: the delete on a custom palette, at the top-right corner of the cell.
fn delete_button(slug: String, t: &Theme, cx: &mut Context<Root>) -> Stateful<Div> {
    let tint = t.warntint;
    div()
        .id(element_id("theme-delete", &slug))
        .absolute()
        .top(px(-8.0))
        .right(px(-8.0))
        .size(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(t.warnline)
        .bg(t.bg)
        .cursor_pointer()
        .hover(move |s| s.bg(tint))
        .child(icon(Icon::Close, 11.0, t, t.warn))
        .on_click(cx.listener(move |root, _: &ClickEvent, window, cx| {
            // The swatch under it would choose the theme that is going away.
            cx.stop_propagation();
            root.delete_theme(slug.clone(), window, cx)
        }))
}

/// `.sw`: the card of one palette. The chosen one has an accent ring.
fn swatch_card(palette: Palette, on: bool, t: &Theme, cx: &mut Context<Root>) -> Stateful<Div> {
    let sel = t.sel;
    let p = palette.theme();
    // A custom palette has the delete mark. No tag takes room from the name.
    let name = div().px(px(4.0)).child(
        body(palette.name(), 13.5, t.fg)
            .font_weight(FontWeight::MEDIUM)
            .min_w_0()
            .truncate(),
    );
    let card = div()
        .id(element_id("palette", &palette.id()))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .px(px(6.0))
        .pt(px(6.0))
        .pb(px(10.0))
        .rounded(px(12.0))
        .border_1()
        .bg(t.bg)
        .cursor_pointer()
        .hover(move |s| s.bg(sel))
        .child(mini(&p))
        .child(name)
        .on_click(cx.listener(move |root, _: &ClickEvent, window, cx| {
            root.choose_palette(palette, window, cx)
        }));
    if on {
        card.border_color(t.acc).shadow(ring(t.acc))
    } else {
        card.border_color(t.line)
    }
}

/// One palette in the grid, with its mark and its delete.
fn swatch(palette: Palette, on: bool, t: &Theme, cx: &mut Context<Root>) -> Div {
    let id = palette.id();
    let mut cell = div()
        .relative()
        .w(px(CELL_W))
        .flex_none()
        .child(swatch_card(palette, on, t, cx));
    if on {
        cell = cell.child(badge(t));
    }
    match id.strip_prefix(CUSTOM_ID) {
        Some(slug) => cell.child(delete_button(slug.to_string(), t, cx)),
        None => cell,
    }
}

/// The palettes for light or for dark, as a grid.
fn palette_row(root: &Root, dark: bool, t: &Theme, cx: &mut Context<Root>) -> Div {
    let themes = &root.account.themes;
    let current = if dark { &themes.dark } else { &themes.light };
    let mut grid = div().flex().flex_wrap().gap(px(12.0)).w_full();
    for palette in palettes(&themes.custom, dark) {
        // A saved id names the palette. Two custom themes never share one.
        let on = palette.id() == current.id();
        grid = grid.child(swatch(palette, on, t, cx));
    }
    let (label, help) = if dark {
        ("Dark palette", "Used in dark mode.")
    } else {
        ("Light palette", "Used in light mode.")
    };
    panel::row(false, label, Some(help), panel::stack().child(grid), t)
}

fn store_row(t: &Theme, cx: &mut Context<Root>) -> Div {
    let help = "Search open-vsx.org and see a theme before you install it. \
                It appears in the picker of its mode.";
    let open = panel::button(
        "browse-themes",
        "Browse VS Code themes…",
        Kind::Plain,
        true,
        None,
        t,
    )
    .on_click(cx.listener(|root, _: &ClickEvent, window, cx| root.open_themes("", window, cx)));
    panel::row(
        false,
        "VS Code themes",
        Some(help),
        panel::stack().justify_center().child(open),
        t,
    )
}

/// The rows of the Appearance section of Settings, each with the words that
/// the search of Settings matches.
pub fn rows(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Vec<(&'static str, Div)> {
    vec![
        ("mode match system light dark theme", mode_row(root, t, cx)),
        (
            "light palette colours theme",
            palette_row(root, false, t, cx),
        ),
        ("dark palette colours theme", palette_row(root, true, t, cx)),
        (
            "vs code themes browse open-vsx store install",
            store_row(t, cx),
        ),
    ]
}
