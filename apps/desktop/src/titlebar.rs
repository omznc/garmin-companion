//! The title bar: the brand, the search box that opens the command palette,
//! the sync and coach buttons, and the window buttons. From the `header` of
//! the design. The whole bar drags the window.

use gpui::{div, prelude::*, px, ClickEvent, Context, Stateful, Window};

use crate::chrome;
use crate::icons::{icon, icon_spin, Icon};
use crate::kit::{icon_button, icon_button_shell, kbd, modifier};
use crate::shell::Root;
use crate::theme::Theme;
use crate::ui::SANS;

/// The width of the search box in the design.
const SEARCH_W: f32 = 420.0;
/// Room for the traffic lights that macOS draws over the left end.
const TRAFFIC_LIGHTS: f32 = 62.0;

fn brand(t: &Theme) -> impl IntoElement {
    // The Horizon mark: the sun in the accent, the horizon in the text colour,
    // the lower lines faint. The three layers stack like a duotone icon.
    let layer = |which, colour| icon(which, 20.0, t, colour).absolute().top_0().left_0();
    let mark = div()
        .flex_none()
        .relative()
        .size(px(20.0))
        .child(layer(Icon::LogoSun, t.acc))
        .child(layer(Icon::LogoHorizon, t.fg))
        .child(layer(Icon::LogoLines, t.faint));
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(8.0))
        .when(cfg!(target_os = "macos"), |d| d.pl(px(TRAFFIC_LIGHTS)))
        .child(mark)
        .child(
            div()
                .font_family(SANS)
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .whitespace_nowrap()
                .child("Garmin Companion"),
        )
}

fn search(t: &Theme, cx: &mut Context<Root>) -> impl IntoElement {
    let (line_strong, mutd) = (t.line_strong, t.mutd);
    div()
        .id("search")
        .group("search")
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(26.0))
        .w(px(SEARCH_W))
        .min_w_0()
        .pl(px(10.0))
        .pr(px(6.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(t.line)
        .bg(t.bg)
        .text_color(t.faint)
        .font_family(SANS)
        .text_size(px(12.5))
        .cursor_pointer()
        .hover(move |s| s.border_color(line_strong).text_color(mutd))
        // The bar under it starts a window move on the same event.
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(
            cx.listener(|root, _: &ClickEvent, window, cx| root.toggle_palette("", window, cx)),
        )
        .child(icon(Icon::Search, 13.0, t, t.faint))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child("Search or run a command"),
        )
        .child(kbd(format!("{} Shift P", modifier()), t))
}

fn sync_button(syncing: bool, t: &Theme, cx: &mut Context<Root>) -> Stateful<gpui::Div> {
    let button = icon_button_shell("tb-sync", false, syncing, t)
        .child(if syncing {
            icon_spin(Icon::Sync, 15.0, t.fg, "tb-sync-spin").into_any_element()
        } else {
            icon(Icon::Sync, 15.0, t, t.mutd).into_any_element()
        })
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
    if syncing {
        button
    } else {
        button.on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.start_sync(false, cx)))
    }
}

fn actions(root: &Root, window: &Window, t: &Theme, cx: &mut Context<Root>) -> impl IntoElement {
    let coach = icon_button("tb-coach", Icon::Panel, 15.0, false, root.coach_open, t)
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.toggle_coach(cx)));
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .justify_end()
        .gap(px(2.0))
        .child(sync_button(root.account.syncing(), t, cx))
        .child(coach)
        .children(chrome::controls(window, t).map(|controls| {
            div()
                .flex()
                .items_center()
                .child(div().w(px(1.0)).h(px(16.0)).mx(px(6.0)).bg(t.line))
                .child(controls)
        }))
}

pub fn render(
    root: &Root,
    window: &Window,
    t: &Theme,
    cx: &mut Context<Root>,
) -> Stateful<gpui::Div> {
    chrome::draggable(
        div()
            .id("titlebar")
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.0))
            .h(px(chrome::STRIP))
            .pl(px(10.0))
            .pr(px(6.0))
            .bg(t.bg3)
            .border_b_1()
            .border_color(t.line)
            .text_color(t.fg),
    )
    .child(brand(t))
    .child(search(t, cx))
    .child(actions(root, window, t, cx))
}
