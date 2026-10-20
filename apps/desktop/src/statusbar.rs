//! The status bar at the bottom of the window. From the `footer` of the design.
//! Each item opens a menu (`statusmenu.rs`) or a screen.

use crate::scale::px;
use gpui::{div, prelude::*, ClickEvent, Context, Div, ElementId, SharedString, Stateful};

use crate::account::Sync;
use crate::focus::Focus;
use crate::icons::{icon, icon_spin, Icon};
use crate::screen::Screen;
use crate::shell::Root;
use crate::statusmenu::{self, MenuKind};
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

/// What comes before the label.
#[derive(Clone, Copy)]
enum Lead {
    None,
    Dot(gpui::Hsla),
    Spinner,
}

/// One item: a 12 px label with room around it. It has a hover state and
/// a pointer. `open` keeps the hover colour while its menu is open.
fn item(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    mono: bool,
    lead: Lead,
    open: bool,
    t: &Theme,
) -> Stateful<Div> {
    let (hover, fg) = (t.hover, t.fg);
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.0))
        .h_full()
        .px(px(8.0))
        .rounded(px(4.0))
        .cursor_pointer()
        .text_color(if open { t.fg } else { t.mutd })
        .text_size(px(12.0))
        .whitespace_nowrap()
        .when(open, |d| d.bg(t.active))
        .when(!open, |d| d.hover(move |s| s.bg(hover).text_color(fg)))
        .when(mono, |d| d.font_family(MONO))
        .map(|d| match lead {
            Lead::None => d,
            Lead::Dot(colour) => d.child(div().size(px(7.0)).rounded_full().bg(colour)),
            Lead::Spinner => d.child(icon_spin(Icon::Sync, 12.0, t.acc, "status-sync-spin")),
        })
        .child(text.into())
}

/// An item that opens the menu of `kind`, and the menu above it.
#[allow(clippy::too_many_arguments)]
fn with_menu(
    kind: MenuKind,
    id: &'static str,
    text: impl Into<SharedString>,
    mono: bool,
    lead: Lead,
    root: &Root,
    t: &Theme,
    cx: &mut Context<Root>,
) -> Div {
    let open = root
        .status_menu
        .open
        .as_ref()
        .is_some_and(|o| o.kind == kind);
    let button = item(id, text, mono, lead, open, t).on_click(cx.listener(
        move |root, _: &ClickEvent, window, cx| root.toggle_status_menu(kind, window, cx),
    ));
    div()
        .flex_none()
        .h_full()
        .child(button)
        .children(statusmenu::menu(root, kind, t, cx))
}

/// "Focus: <label>" with an x. The label opens the calendar. The x goes back
/// to the live day.
fn focus_chip(root: &Root, t: &Theme, cx: &mut Context<Root>) -> impl IntoElement {
    let (tint, hover) = (t.acctint, t.hover);
    let clear = div()
        .id("focus-clear")
        .flex()
        .items_center()
        .justify_center()
        .size(px(16.0))
        .rounded(px(4.0))
        .hover(move |s| s.bg(hover))
        .child(icon(Icon::X, 10.0, t, t.acc))
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            root.set_focus(Focus::today(), cx)
        }));
    item(
        "focus-chip",
        format!("Focus: {}", root.focus.label()),
        false,
        Lead::None,
        false,
        t,
    )
    .bg(tint)
    .text_color(t.acc)
    .child(clear)
    .on_click(
        cx.listener(|root, _: &ClickEvent, window, cx| root.toggle_calendar_popover(window, cx)),
    )
}

/// The label of the sync item, and what comes before it.
fn sync_label(root: &Root, t: &Theme) -> (String, Lead) {
    let account = &root.account;
    match &account.sync {
        Sync::Running(step) => {
            // A long step would push the other items. The menu shows it in full.
            let short: String = step.chars().take(48).collect();
            (format!("Syncing... {short}"), Lead::Spinner)
        }
        Sync::Failed(_) => ("Sync failed".into(), Lead::Dot(t.warn)),
        _ => match &account.synced_ago {
            Some(ago) => (format!("Synced {ago}"), Lead::Dot(t.acc)),
            None => ("Not synced yet".into(), Lead::None),
        },
    }
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
    let (label, lead) = sync_label(root, t);
    let synced = with_menu(
        MenuKind::Sync,
        "status-sync",
        label,
        false,
        lead,
        root,
        t,
        cx,
    );
    let count = root.activity_count().filter(|_| !narrow).map(|n| {
        let text = format!("{} activities", crate::map::num(n as f64));
        item("status-count", text, true, Lead::None, false, t).on_click(
            cx.listener(|root, _: &ClickEvent, _, cx| root.select_screen(Screen::Activities, cx)),
        )
    });
    let coach = coach.filter(|_| !narrow).map(|name| {
        let text = format!("Coach: {name}");
        with_menu(
            MenuKind::Coach,
            "status-coach",
            text,
            false,
            Lead::None,
            root,
            t,
            cx,
        )
    });
    let palette = with_menu(
        MenuKind::Palette,
        "status-palette",
        root.palette.name(),
        false,
        Lead::None,
        root,
        t,
        cx,
    );
    let version = (!narrow).then(|| {
        let text = format!("v{}", crate::updater::VERSION);
        with_menu(
            MenuKind::Version,
            "status-version",
            text,
            true,
            Lead::None,
            root,
            t,
            cx,
        )
    });
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
        .font_family(SANS)
        .child(synced)
        .children(count)
        .children(focus)
        .child(div().flex_1())
        .children(coach)
        .child(palette)
        .children(version)
}
