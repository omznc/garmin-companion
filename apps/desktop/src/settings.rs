//! The Settings screen, from `app/src/screens/Settings.tsx` and the Settings design.
//!
//! The page is a header and four sections, each a panel of rows: Garmin account,
//! Coach, Appearance (`appearance.rs`) and About. A change applies at once.

use garmin_api::settings::SettingsView;
use gpui::{div, prelude::*, px, relative, AnyElement, ClickEvent, Context, Div, Window};

use crate::account::Sync;
use crate::icons::Icon;
use crate::panel::{self, Kind, Tone};
use crate::shell::{Root, SHELL_MAX};
use crate::sidebar::SIDEBAR_W;
use crate::theme::{Theme, FS_MD};
use crate::ui::{block_in, body, eyebrow_lg, MONO, SERIF};

/// The widest the Settings content gets. The reading column is narrower.
pub const CONTENT_MAX: f32 = 800.0;

/// At or below this content width the design's narrow rule applies: the title
/// shrinks and the rows get less padding. It is the 860px window of the design,
/// less the 2 x 56px of side air.
const NARROW_AT: f32 = 748.0;

/// The width of the content for a window `viewport` pixels wide.
fn content_width(viewport: f32) -> f32 {
    (viewport.min(SHELL_MAX) - SIDEBAR_W - 2.0 * crate::shell::COLUMN_SIDE).min(CONTENT_MAX)
}

fn sync_note(root: &Root, t: &Theme) -> Option<Div> {
    match &root.account.sync {
        Sync::Idle => None,
        Sync::Running(line) => Some(panel::note(Tone::Run, line.clone(), t)),
        Sync::Done(line) => Some(panel::note(Tone::Ok, line.clone(), t)),
        Sync::Failed(error) => Some(panel::note(Tone::Err, error.clone(), t)),
    }
}

fn connection_row(view: &SettingsView, t: &Theme) -> Div {
    let state = if view.connected {
        "Connected"
    } else {
        "Not connected"
    };
    let value = panel::stack()
        .child(panel::chip(state, view.connected, t))
        .child(body(view.account_line.clone(), 14.5, t.fg))
        .child(panel::help(view.cache_line.clone(), t));
    panel::row(true, "Connection", None, value, t)
}

fn folder_row(folder: &str, t: &Theme) -> Div {
    // A path has no break points. A long one ends in an ellipsis.
    let path = div()
        .w_full()
        .px(px(12.0))
        .py(px(9.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(t.line2)
        .bg(t.bg2)
        .font_family(MONO)
        .text_size(px(13.0))
        .text_color(t.fg)
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(folder.to_string());
    let value = panel::stack().items_stretch().child(path);
    panel::row(false, "Data folder", Some("The database and the cache."), value, t)
}

fn sync_row(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
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
    panel::row(false, "Sync", Some(help), value, t)
}

fn disconnect_row(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
    let busy = root.account.syncing();
    let value = panel::stack().justify_center().child(
        panel::button(
            "sign-out",
            "Disconnect from Garmin",
            Kind::Destroy,
            !busy,
            Some(Icon::SignOut),
            t,
        )
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.sign_out(cx))),
    );
    let help = "Signs this computer out of Garmin. To sync again, you must connect again.";
    panel::danger_row("Disconnect", Some(help), value, t)
}

fn garmin(root: &Root, view: &SettingsView, t: &Theme, cx: &mut Context<Root>) -> Div {
    let mut rows = panel::panel(t).child(connection_row(view, t));
    if let Some(folder) = &view.data_folder {
        rows = rows.child(folder_row(folder, t));
    }
    let rows = rows
        .child(sync_row(root, t, cx))
        .child(disconnect_row(root, t, cx));
    panel::section("Garmin account", None, t).child(rows)
}

fn about(root: &Root, view: &SettingsView, t: &Theme, cx: &mut Context<Root>) -> Div {
    let text = "Garmin Companion reads your Garmin data into a database on this computer. \
                You can delete that file at any time. The app is open source.";
    let prose = |text: String, colour| body(text, 14.5, colour).line_height(relative(1.6));
    // The version stands under the name, in mono.
    let version = div()
        .font_family(MONO)
        .text_size(px(12.5))
        .text_color(t.mutd)
        .child(format!("Version {}", view.version));
    let rows = panel::panel(t)
        .child(panel::row_with(
            true,
            "Garmin Companion",
            Some(version),
            panel::stack().child(prose(text.to_string(), t.mutd)),
            t,
        ))
        .child(crate::update_ui::settings_row(root, t, cx))
        .child(panel::row(
            false,
            "What leaves this machine",
            None,
            panel::stack().child(prose(view.privacy_line.clone(), t.fg)),
            t,
        ));
    panel::section("About", None, t).child(rows)
}

pub fn render(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> AnyElement {
    let Some(view) = root.account.settings.clone() else {
        return div().into_any_element();
    };
    let k = root.visits;
    let narrow = content_width(f32::from(window.viewport_size().width)) <= NARROW_AT;
    panel::set_narrow(narrow);
    let title = div()
        .font_family(SERIF)
        .text_size(px(if narrow { 42.0 } else { 52.0 }))
        .line_height(relative(1.04))
        .letter_spacing(px(if narrow { -0.42 } else { -0.52 }))
        .child("Settings");
    let lead = "Your Garmin history lives here, in a file you can delete. \
                A change applies at once. There is no save button.";
    let header = div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(block_in(k, 0, eyebrow_lg("This machine", t)))
        .child(block_in(k, 0, title))
        .child(block_in(
            k,
            1,
            body(lead, FS_MD, t.mutd).max_w(px(544.0)),
        ));
    div()
        .flex()
        .flex_col()
        .gap(px(60.0))
        .child(header)
        .child(block_in(k, 2, garmin(root, &view, t, cx)))
        .child(block_in(k, 3, crate::cloud::render(root, t, window, cx)))
        .child(block_in(k, 3, crate::appearance::render(root, t, cx)))
        .child(block_in(k, 4, about(root, &view, t, cx)))
        .into_any_element()
}
