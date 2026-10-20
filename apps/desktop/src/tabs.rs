//! The tab bar of the open screens and the breadcrumb bar under it. From the
//! `tablist` and the toolbar of the design.
//!
//! Each tab can close, Today too. The tab of a screen opens at the end when
//! the user picks the screen. At most eight tabs stay open: a new one closes
//! the oldest tab.

use gpui::{div, prelude::*, px, ClickEvent, Context, Div, MouseButton, SharedString};

use crate::icons::{icon, icon_hover, Icon};
use crate::screen::Screen;
use crate::shell::Root;
use crate::theme::Theme;
use crate::ui::SANS;

pub const MAX_TABS: usize = 8;

/// Open the tab of `screen` at the end, or find it when it is open. The
/// oldest tab makes room when the bar is full.
pub fn open(tabs: &mut Vec<Screen>, screen: Screen) {
    if tabs.contains(&screen) {
        return;
    }
    if tabs.len() >= MAX_TABS {
        tabs.remove(0);
    }
    tabs.push(screen);
}

/// Close the tab of `screen`. Returns the screen to show when the closed tab
/// was the one in view: its left neighbour. `None` when the view stays, or
/// when no tab is left.
pub fn close(tabs: &mut Vec<Screen>, screen: Screen, active: Screen) -> Option<Screen> {
    let at = tabs.iter().position(|s| *s == screen)?;
    tabs.remove(at);
    if screen != active {
        return None;
    }
    tabs.get(at.saturating_sub(1)).copied()
}

fn close_button(active: bool, t: &Theme) -> Div {
    let (hover, mutd, fg) = (t.hover, t.mutd, t.fg);
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(16.0))
        .rounded(px(4.0))
        .ml(px(2.0))
        .when(!active, |d| {
            d.opacity(0.0).group_hover("tab", |s| s.opacity(1.0))
        })
        .hover(move |s| s.bg(hover))
        .child(icon_hover(Icon::X, 10.0, t, mutd, Some(("tab-x", fg))))
        .group("tab-x")
}

fn tab(
    index: usize,
    screen: Screen,
    active: bool,
    t: &Theme,
    cx: &mut Context<Root>,
) -> impl IntoElement {
    let (fg, bg) = (t.fg, t.bg);
    let colour = if active { t.fg } else { t.mutd };
    let tab = div()
        .id(("tab", index))
        .group("tab")
        .flex_none()
        .flex()
        .items_center()
        .gap(px(7.0))
        .h_full()
        .px(px(12.0))
        .border_r_1()
        .border_color(t.line)
        .cursor_pointer()
        .font_family(SANS)
        .text_size(px(13.0))
        .whitespace_nowrap()
        .text_color(colour)
        // The lit tab joins the page: no line under it.
        .when(active, |d| d.bg(bg))
        .when(!active, |d| d.border_b_1().hover(move |s| s.text_color(fg)))
        .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.select_screen(screen, cx)))
        .on_mouse_down(
            MouseButton::Middle,
            cx.listener(move |root, _, _, cx| root.close_tab(screen, cx)),
        )
        .child(icon(screen.icon(), 13.0, t, colour))
        .child(SharedString::from(screen.label()));
    let x = close_button(active, t)
        .id(("tab-close", index))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            root.close_tab(screen, cx)
        }));
    tab.child(x)
}

/// The bar of tabs. The part right of the last tab is empty.
pub fn tab_bar(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
    let tabs = root.tabs().to_vec();
    let mut bar = div()
        .flex_none()
        .flex()
        .items_stretch()
        .h(px(34.0))
        .bg(t.bg2)
        .overflow_hidden();
    for (i, screen) in tabs.into_iter().enumerate() {
        bar = bar.child(tab(i, screen, screen == root.active, t, cx));
    }
    bar.child(div().flex_1().border_b_1().border_color(t.line))
}

/// The bar with the screen name, the page and the age of the last sync.
pub fn crumb_bar(root: &Root, narrow: bool, t: &Theme, cx: &mut Context<Root>) -> Div {
    if root.editing() {
        return crate::blocks::edit::bar(root, narrow, t, cx);
    }
    let (hover, fg) = (t.hover, t.fg);
    let screen = root.active;
    let home = div()
        .id("crumb-home")
        .flex_none()
        .cursor_pointer()
        .hover(move |s| s.text_color(fg).underline())
        .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.screen_home(screen, cx)))
        .child(screen.label());
    let pick =
        div()
            .id("crumb-pick")
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(4.0))
            .h(px(24.0))
            .px(px(6.0))
            .rounded(px(5.0))
            .cursor_pointer()
            .text_color(t.fg)
            .hover(move |s| s.bg(hover))
            .on_click(cx.listener(|root, _: &ClickEvent, window, cx| {
                root.toggle_calendar_popover(window, cx)
            }))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(root.crumb(t))),
            )
            .child(icon(Icon::ChevronDown, 10.0, t, t.faint));
    let pick = div()
        .min_w_0()
        .child(pick)
        .children(crate::calendar::popover(root, t, cx));
    let synced = root
        .account
        .synced_ago
        .as_ref()
        .filter(|_| !narrow)
        .map(|ago| {
            div()
                .flex_none()
                .text_color(t.faint)
                .child(format!("Synced {ago}"))
        });
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(34.0))
        .pl(px(16.0))
        .pr(px(12.0))
        .border_b_1()
        .border_color(t.line2)
        .font_family(SANS)
        .text_size(px(12.5))
        .text_color(t.mutd)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(5.0))
                .whitespace_nowrap()
                .child(home)
                .child(div().flex_none().text_color(t.faint).child("›"))
                .child(pick),
        )
        .children(synced)
        .children((screen == Screen::Today).then(|| crate::blocks::edit::start_button(t, cx)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_opens_one_tab() {
        let mut tabs = vec![Screen::Today];
        open(&mut tabs, Screen::Sleep);
        open(&mut tabs, Screen::Sleep);
        open(&mut tabs, Screen::Today);
        assert_eq!(tabs, [Screen::Today, Screen::Sleep]);
    }

    #[test]
    fn the_oldest_tab_but_today_makes_room() {
        let mut tabs = vec![Screen::Today];
        for screen in Screen::ALL.into_iter().skip(1).take(MAX_TABS - 1) {
            open(&mut tabs, screen);
        }
        assert_eq!(tabs.len(), MAX_TABS);
        open(&mut tabs, Screen::Settings);
        assert_eq!(tabs.len(), MAX_TABS);
        assert!(!tabs.contains(&Screen::Today));
        assert_eq!(tabs.last(), Some(&Screen::Settings));
    }

    #[test]
    fn a_closed_tab_gives_its_neighbour_and_the_last_gives_none() {
        let mut tabs = vec![Screen::Today, Screen::Sleep, Screen::Gear];
        // A tab out of view closes and the view stays.
        assert_eq!(close(&mut tabs, Screen::Gear, Screen::Sleep), None);
        assert_eq!(
            close(&mut tabs, Screen::Sleep, Screen::Sleep),
            Some(Screen::Today)
        );
        assert_eq!(close(&mut tabs, Screen::Today, Screen::Today), None);
        assert!(tabs.is_empty());
    }
}
