//! The status bar at the bottom of the window. From the `footer` of the design.

use gpui::{div, prelude::*, px, ClickEvent, Context, Div, SharedString};

use crate::focus::Focus;
use crate::icons::{icon, Icon};
use crate::shell::Root;
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

/// One item: a 12 px label with room around it. A dot can come first.
fn item(text: impl Into<SharedString>, mono: bool, dot: bool, t: &Theme) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.0))
        .h_full()
        .px(px(8.0))
        .rounded(px(4.0))
        .text_color(t.mutd)
        .text_size(px(12.0))
        .whitespace_nowrap()
        .when(mono, |d| d.font_family(MONO))
        .when(dot, |d| {
            d.child(div().size(px(7.0)).rounded_full().bg(t.acc))
        })
        .child(text.into())
}

/// "Focus: <label>" with an x. A click goes back to the live day.
fn focus_chip(root: &Root, t: &Theme, cx: &mut Context<Root>) -> impl IntoElement {
    let tint = t.acctint;
    item(format!("Focus: {}", root.focus.label()), false, false, t)
        .id("focus-chip")
        .bg(tint)
        .text_color(t.acc)
        .cursor_pointer()
        .child(icon(Icon::X, 10.0, t, t.acc))
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.set_focus(Focus::today(), cx)))
}

/// The bar. `coach` names the provider. `narrow` hides what the design hides
/// in a small window.
pub fn render(
    root: &Root,
    coach: Option<&str>,
    narrow: bool,
    t: &Theme,
    cx: &mut Context<Root>,
) -> Div {
    let focus = (!root.focus.is_live()).then(|| focus_chip(root, t, cx));
    let synced = root
        .account
        .synced_ago
        .as_ref()
        .map(|ago| item(format!("Synced {ago}"), false, true, t));
    let count = root.activity_count().filter(|_| !narrow).map(|n| {
        let text = format!("{} activities", crate::map::num(n as f64));
        item(text, true, false, t)
    });
    let coach = coach
        .filter(|_| !narrow)
        .map(|name| item(format!("Coach: {name}"), false, false, t));
    let version = format!("v{}", crate::updater::VERSION);
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(2.0))
        .h(px(26.0))
        .px(px(6.0))
        .bg(t.bg3)
        .border_t_1()
        .border_color(t.line)
        .overflow_hidden()
        .font_family(SANS)
        .children(synced)
        .children(count)
        .children(focus)
        .child(div().flex_1())
        .children(coach)
        .child(item(root.palette.name(), false, false, t))
        .children((!narrow).then(|| item(version, true, false, t)))
}
