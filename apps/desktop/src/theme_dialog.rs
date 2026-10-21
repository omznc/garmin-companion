//! The theme search dialog: "VS Code themes" over the Settings page.
//!
//! The markup follows the theme search modal of the Settings design. The state
//! and the network jobs live in `themes.rs`. The dialog covers the window like
//! the command palette does, and it takes the keys that the field leaves.

use crate::scale::px;
use garmin_core::theme as folder;
use gpui::{
    div, point, prelude::*, relative, AnyElement, BoxShadow, ClickEvent, Context, Div, FontWeight,
    Hsla, KeyDownEvent, Stateful, TextTransform, Window,
};

use crate::icons::{icon, Icon};
use crate::panel::{self, Kind, Tone};
use crate::shell::Root;
use crate::theme::{Palette, Theme};
use crate::themes::{downloads, goes_to, key_of, mode_tag, Fetch, Phase, Store};
use crate::ui::{body, eyebrow, MONO, SANS};

/// At or below this window width the design's narrow rule applies.
const NARROW_AT: f32 = 860.0;
const DIALOG_MAX: f32 = 880.0;
/// A dialog narrower than this stacks the two columns.
const STACK_BELOW: f32 = 680.0;
const PREVIEW_MIN: f32 = 250.0;

/// The colours of a theme, as the app would show them.
fn derived(theme: &folder::Theme) -> Option<Theme> {
    Palette::custom(theme).map(Palette::theme)
}

/// The sizes that depend on the window.
struct Fit {
    /// The side padding of the dialog parts.
    pad: f32,
    /// The air between the window and the dialog: across and down.
    air: (f32, f32),
    stacked: bool,
    /// The most that the dialog may be high.
    height: f32,
}

fn fit(window: &Window) -> Fit {
    let size = window.viewport_size();
    let (width, height) = (crate::scale::dp(size.width), crate::scale::dp(size.height));
    let narrow = width <= NARROW_AT;
    let air = if narrow { (12.0, 12.0) } else { (24.0, 48.0) };
    Fit {
        pad: if narrow { 12.0 } else { 16.0 },
        air,
        stacked: (width - 2.0 * air.0).min(DIALOG_MAX) < STACK_BELOW,
        height: height - 2.0 * air.1,
    }
}

fn header(fit: &Fit, t: &Theme, cx: &mut Context<Root>) -> Div {
    let hover = t.sel;
    let titles = div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_baseline()
        .gap(px(10.0))
        .child(
            body("VS Code themes", 14.0, t.fg)
                .flex_none()
                .font_weight(FontWeight::MEDIUM),
        )
        .child(
            body(
                "From open-vsx.org. A theme is saved on this computer as seven colours.",
                12.0,
                t.faint,
            )
            .truncate(),
        );
    let close = div()
        .id("themes-close")
        .flex_none()
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(Icon::Close, 13.0, t, t.mutd))
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.close_themes(cx)));
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(fit.pad))
        .py(px(8.0))
        .border_b_1()
        .border_color(t.line2)
        .child(titles)
        .child(close)
}

/// The flat search row at the top of the list, with a hairline below it.
fn search_row(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let field = root.account.themes.search.render(
        "theme-search",
        t,
        cx,
        window,
        |root| &mut root.account.themes.search,
        |root, _, cx| root.search_themes(cx),
    );
    let mark =
        div()
            .absolute()
            .left(px(14.0))
            .top(px(11.0))
            .child(icon(Icon::Search, 14.0, t, t.mutd));
    div()
        .relative()
        .flex_none()
        .border_b_1()
        .border_color(t.line2)
        .child(field)
        .child(mark)
}

/// A strip of the page, the text and the accent of a theme.
fn strip(colours: Option<&Theme>, t: &Theme) -> Div {
    let [bg, fg, acc] = match colours {
        Some(p) => [p.bg, p.fg, p.acc],
        None => [t.line2; 3],
    };
    let span = |colour: Hsla| div().w(px(5.0)).h(px(14.0)).bg(colour);
    div()
        .flex()
        .flex_none()
        .rounded(px(3.0))
        .overflow_hidden()
        .border_1()
        .border_color(t.line)
        .child(span(bg))
        .child(span(fg))
        .child(span(acc))
}

fn result_row(index: usize, store: &Store, t: &Theme, cx: &mut Context<Root>) -> Stateful<Div> {
    let found = &store.found[index];
    let on = index == store.selected;
    let sel = t.sel;
    let themes = match store.fetched.get(&key_of(found)) {
        Some(Fetch::Ready(list)) => Some(list.as_slice()),
        _ => None,
    };
    let colours = themes.and_then(<[_]>::first).and_then(derived);
    let main = div()
        .flex_1()
        .min_w_0()
        .child(
            body(found.display.clone(), 13.0, t.fg)
                .line_height(relative(1.3))
                .truncate(),
        )
        .child(
            body(
                format!("{} · {}", found.namespace, downloads(found.downloads)),
                11.5,
                t.mutd,
            )
            .line_height(relative(1.3))
            .truncate(),
        );
    let mut row = div()
        .id(("theme-result", index))
        .flex()
        .items_center()
        .gap(px(10.0))
        .mx(px(6.0))
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .child(strip(colours.as_ref(), t))
        .child(main)
        .on_click(cx.listener(move |root, _: &ClickEvent, window, cx| {
            root.select_result(index, cx);
            // The arrow keys reach the dialog only from the field.
            root.account.themes.search.focus.focus(window, cx);
        }));
    if let Some(list) = themes {
        row = row.child(mode_label(mode_tag(list), t));
    }
    if on {
        row.bg(sel)
    } else {
        row.hover(move |s| s.bg(sel))
    }
}

/// The small muted mode label at the end of a row.
fn mode_label(text: impl Into<gpui::SharedString>, t: &Theme) -> Div {
    div()
        .flex_none()
        .font_family(MONO)
        .text_size(px(10.0))
        .letter_spacing(px(10.0 * 0.04))
        .text_color(t.faint)
        .text_transform(TextTransform::Uppercase)
        .whitespace_nowrap()
        .child(text.into())
}

/// The left column: the state of the search, then the results.
fn results(
    root: &Root,
    fit: &Fit,
    t: &Theme,
    window: &Window,
    cx: &mut Context<Root>,
) -> Stateful<Div> {
    let store = &root.account.themes.store;
    let air = |d: Div| d.px(px(16.0)).py(px(10.0));
    let mut list = div()
        .id("theme-list")
        .flex()
        .flex_col()
        .py(px(6.0))
        .min_w_0();
    list = if fit.stacked {
        list.flex_none()
    } else {
        list.flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&store.scroll)
    };
    list = match &store.phase {
        Phase::Idle => list.child(air(panel::help("Type a name and press Enter.", t))),
        Phase::Searching(text) => list.child(air(panel::note(
            Tone::Run,
            format!("Searching open-vsx.org for \u{201c}{text}\u{201d}."),
            t,
        ))),
        Phase::Empty => list.child(air(panel::note(
            Tone::Info,
            "No theme matches that search.",
            t,
        ))),
        Phase::Failed(e) => list.child(air(panel::note(Tone::Err, e.clone(), t))),
        Phase::List => {
            list = list.child(
                eyebrow(format!("{} themes", store.found.len()), t)
                    .px(px(16.0))
                    .pt(px(4.0))
                    .pb(px(6.0)),
            );
            for index in 0..store.found.len() {
                list = list.child(result_row(index, store, t, cx));
            }
            list
        }
    };
    let col = div()
        .id("theme-results")
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .flex()
        .flex_col()
        .min_w_0()
        .child(search_row(root, t, window, cx))
        .child(list);
    if fit.stacked {
        col.flex_none()
    } else {
        col.flex_basis(px(320.0)).min_h_0()
    }
}

/// The page that a theme would paint: the nav and a short Today.
fn preview(p: &Theme, line: Hsla) -> Div {
    let label =
        |text: &str, colour: Hsla| body(text.to_string(), 12.5, colour).px(px(6.0)).py(px(5.0));
    let current = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .rounded(px(6.0))
        .bg(p.sel)
        .child(div().ml(px(6.0)).size(px(6.0)).rounded_full().bg(p.acc))
        .child(label("Today", p.fg).pl(px(0.0)));
    let nav = div()
        .w(relative(0.3))
        .flex_none()
        .px(px(12.0))
        .py(px(16.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .bg(p.bg2)
        .child(
            div()
                .font_family(SANS)
                .text_size(px(16.0))
                .text_color(p.fg)
                .px(px(6.0))
                .pb(px(10.0))
                .child("Companion"),
        )
        .child(current)
        .children(["Health", "Sleep", "Training"].map(|name| label(name, p.mutd).pl(px(18.0))));
    let buttons = div()
        .mt_auto()
        .flex()
        .flex_wrap()
        .gap(px(8.0))
        .child(
            body("Ask the coach", 12.0, p.bg)
                .font_weight(FontWeight::MEDIUM)
                .px(px(10.0))
                .py(px(5.0))
                .rounded(px(6.0))
                .bg(p.acc),
        )
        .child(
            body("Load is high", 12.0, p.warn)
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(p.warn),
        );
    let page = div()
        .flex_1()
        .min_w_0()
        .px(px(20.0))
        .py(px(18.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            body("Tuesday", 10.5, p.faint)
                .letter_spacing(px(10.5 * 0.12))
                .text_transform(TextTransform::Uppercase),
        )
        .child(
            div()
                .font_family(SANS)
                .text_size(px(30.0))
                .line_height(relative(1.05))
                .text_color(p.fg)
                .child("Today"),
        )
        .child(
            div()
                .mt(px(6.0))
                .font_family(MONO)
                .text_size(px(22.0))
                .text_color(p.fg)
                .child("7.4 km"),
        )
        .child(body("Easy run, 42 minutes", 12.5, p.mutd))
        .child(div().h(px(1.0)).my(px(6.0)).bg(p.line))
        .child(buttons);
    div()
        .flex()
        .min_h(px(PREVIEW_MIN))
        .rounded(px(6.0))
        .overflow_hidden()
        .border_1()
        .border_color(line)
        .bg(p.bg)
        .child(nav)
        .child(page)
}

/// A dashed box where the preview goes.
fn placeholder(inner: impl IntoElement, t: &Theme) -> Div {
    div()
        .flex_1()
        .min_h(px(PREVIEW_MIN))
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .rounded(px(6.0))
        .border_1()
        .border_dashed()
        .border_color(t.line)
        .child(inner)
}

/// What stands beside the Install button: the button, or the chip.
fn install_control(store: &Store, key: &str, t: &Theme, cx: &mut Context<Root>) -> AnyElement {
    if store.installed.contains_key(key) {
        return panel::chip("Installed", true, t).into_any_element();
    }
    if store.installing.as_deref() == Some(key) {
        return panel::button(
            "theme-install",
            "Installing",
            Kind::Primary,
            false,
            Some(Icon::Spinner),
            t,
        )
        .into_any_element();
    }
    let free = store.installing.is_none();
    let mut install = panel::button("theme-install", "Install", Kind::Primary, free, None, t);
    if free {
        install =
            install.on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.install_selected(cx)));
    }
    install.into_any_element()
}

/// The name, the facts and the install control under the preview.
fn facts(
    store: &Store,
    i: usize,
    themes: &[folder::Theme],
    t: &Theme,
    cx: &mut Context<Root>,
) -> Div {
    let found = &store.found[i];
    let mut meta = format!("{} · {}", found.namespace, downloads(found.downloads));
    if themes.len() > 1 {
        meta.push_str(&format!(" · {} themes", themes.len()));
    }
    meta.push_str(&format!(" · goes to the {}", goes_to(themes)));
    let text = div()
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .flex_basis(px(200.0))
        .min_w_0()
        .child(
            body(found.display.clone(), 14.0, t.fg)
                .font_weight(FontWeight::MEDIUM)
                .truncate(),
        )
        .child(panel::help(meta, t));
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(px(16.0))
        .gap_y(px(12.0))
        .child(text)
        .child(install_control(store, &key_of(found), t, cx))
}

/// The notes under the facts: working, done or failed.
fn notes(store: &Store, i: usize, themes: &[folder::Theme], t: &Theme) -> Vec<Div> {
    let found = &store.found[i];
    let key = key_of(found);
    let mut out = Vec::new();
    if store.installing.as_deref() == Some(key.as_str()) {
        out.push(panel::note(
            Tone::Run,
            format!("Installing {}.", found.display),
            t,
        ));
    }
    if let Some(names) = store.installed.get(&key) {
        let text = format!(
            "Installed {}. Pick it in the {}.",
            names.join(", "),
            goes_to(themes)
        );
        out.push(panel::note(Tone::Ok, text, t));
    }
    if let Some(e) = &store.failure {
        out.push(panel::note(Tone::Err, e.clone(), t));
    }
    out
}

/// The right column: the preview of the chosen result.
fn detail(root: &Root, fit: &Fit, t: &Theme, cx: &mut Context<Root>) -> Stateful<Div> {
    let store = &root.account.themes.store;
    let mut col = div()
        .id("theme-detail")
        .flex_grow(1.5)
        .flex_shrink(1.0)
        .border_l_1()
        .border_color(t.line2)
        .px(px(16.0))
        .py(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .min_w_0();
    col = if fit.stacked {
        col.flex_none()
    } else {
        col.flex_basis(px(360.0)).overflow_y_scroll()
    };
    let i = store.selected;
    let chosen = matches!(store.phase, Phase::List)
        .then(|| store.found.get(i))
        .flatten();
    let Some(found) = chosen else {
        let hint = panel::help("A preview shows here when you choose a result.", t).text_center();
        return col.child(placeholder(hint, t));
    };
    match store.fetched.get(&key_of(found)) {
        Some(Fetch::Ready(themes)) => {
            let shown = themes.first().and_then(derived);
            let picture = match shown {
                Some(p) => preview(&p, t.line),
                None => placeholder(
                    panel::note(Tone::Err, "The colours of this theme cannot be read.", t),
                    t,
                ),
            };
            col.child(picture)
                .child(facts(store, i, themes, t, cx))
                .children(notes(store, i, themes, t))
        }
        Some(Fetch::Failed(e)) => col.child(placeholder(panel::note(Tone::Err, e.clone(), t), t)),
        _ => col.child(placeholder(
            panel::note(
                Tone::Run,
                format!("Getting the colours of {}.", found.display),
                t,
            ),
            t,
        )),
    }
}

fn kbd(label: &str, t: &Theme) -> Div {
    div()
        .px(px(4.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(t.line)
        .font_family(MONO)
        .text_size(px(10.5))
        .text_color(t.mutd)
        .child(label.to_string())
}

fn footer(fit: &Fit, t: &Theme) -> Div {
    let hint = |keys: &[&str], text: &str| {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .children(keys.iter().map(|k| kbd(k, t)))
            .child(text.to_string())
    };
    div()
        .flex()
        .flex_wrap()
        .gap_x(px(16.0))
        .gap_y(px(6.0))
        .px(px(fit.pad))
        .py(px(7.0))
        .border_t_1()
        .border_color(t.line2)
        .font_family(SANS)
        .text_size(px(11.5))
        .text_color(t.faint)
        .child(hint(&["Enter"], "searches"))
        .child(hint(&["\u{2191}", "\u{2193}"], "choose a result"))
        .child(hint(&["Esc"], "closes"))
}

/// The dialog, or nothing while it is closed.
pub fn render(
    root: &Root,
    t: &Theme,
    window: &Window,
    cx: &mut Context<Root>,
) -> Option<AnyElement> {
    if !root.account.themes.store.open {
        return None;
    }
    let fit = fit(window);
    // The header and the footer take about 80px. The body gets the rest at
    // most, so the dialog fits the window and the columns scroll.
    let body_max = (fit.height - 80.0).max(160.0);
    let columns = div()
        .id("themes-body")
        .flex()
        .flex_shrink(1.0)
        .min_h(px(body_max.min(440.0)))
        .max_h(px(body_max))
        .child(results(root, &fit, t, window, cx))
        .child(detail(root, &fit, t, cx));
    let columns = if fit.stacked {
        columns.flex_col().overflow_y_scroll()
    } else {
        columns
    };
    let shadow = BoxShadow {
        color: t.dshadow,
        offset: point(px(0.0), px(24.0)),
        blur_radius: px(64.0),
        spread_radius: px(0.0),
        inset: false,
    };
    let dialog = div()
        .id("themes-dialog")
        .w_full()
        .max_w(px(DIALOG_MAX))
        .max_h(px(fit.height))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(10.0))
        .border_1()
        .border_color(t.line)
        .bg(t.bg2)
        .shadow(vec![shadow])
        // A click on the dialog must not reach the scrim.
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(header(&fit, t, cx))
        .child(columns)
        .child(footer(&fit, t));
    // The scrim takes every click and key that the page under it would take.
    let scrim = div()
        .id("themes-scrim")
        .absolute()
        .size_full()
        .top_0()
        .left_0()
        .occlude()
        .flex()
        .justify_center()
        .items_start()
        .px(px(fit.air.0))
        .py(px(fit.air.1))
        .bg(t.scrim)
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.close_themes(cx)))
        .on_key_down(cx.listener(|root, event: &KeyDownEvent, _, cx| {
            match event.keystroke.key.as_str() {
                "escape" => root.close_themes(cx),
                "down" => root.step_result(true, cx),
                "up" => root.step_result(false, cx),
                _ => {}
            }
        }))
        .child(dialog);
    Some(scrim.into_any_element())
}
