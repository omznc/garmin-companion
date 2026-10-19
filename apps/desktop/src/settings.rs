//! The Settings screen, from `app/src/screens/Settings.tsx`.
//!
//! Only the essential sections are here: Garmin (account, sync), Appearance
//! (light, dark or system), what leaves this machine, and About. The rest of
//! the old screen comes with the screens it belongs to.

use garmin_api::settings::{SettingsView, ThemeMode};
use gpui::{
    div, prelude::*, px, relative, AnyElement, ClickEvent, Context, Div, FontWeight, Window,
};

use crate::account::Sync;
use crate::controls::quiet;
use crate::shell::Root;
use crate::theme::{Theme, FS_CAPTION, FS_MD, FS_SMALL};
use crate::ui::{block_in, body, ch, eyebrow, eyebrow_lg, h1, MONO, SANS};

/// A section: its marker, then the content, with the same air above each.
pub(crate) fn section(first: bool, title: &str, t: &Theme) -> Div {
    div()
        .mt(px(if first { 0.0 } else { 62.0 }))
        .child(div().mb(px(20.0)).child(eyebrow(title.to_string(), t)))
}

/// The dot before the account line: the accent when connected.
fn dot(on: bool, t: &Theme) -> Div {
    div()
        .flex_none()
        .mt(px(10.5))
        .size(px(5.0))
        .rounded_full()
        .bg(if on { t.acc } else { t.line })
}

fn sync_note(root: &Root, t: &Theme) -> Option<AnyElement> {
    match &root.account.sync {
        Sync::Idle => None,
        Sync::Running(line) | Sync::Done(line) => {
            Some(body(line.clone(), FS_SMALL, t.mutd).into_any_element())
        }
        Sync::Failed(error) => Some(body(error.clone(), FS_SMALL, t.warn).into_any_element()),
    }
}

fn garmin(root: &Root, view: &SettingsView, t: &Theme, cx: &mut Context<Root>) -> Div {
    let busy = root.account.syncing();
    let label = if busy { "Syncing..." } else { "Sync recent" };
    let actions = div()
        .flex()
        .items_baseline()
        .gap(px(26.0))
        .mt(px(24.0))
        .child(
            quiet("sync-recent", label, !busy, false, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.start_sync(false, cx))),
        )
        .child(
            quiet("sync-full", "Full re-sync", !busy, false, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.start_sync(true, cx))),
        )
        .child(
            // Pushed to the far end: it undoes the others.
            quiet("sign-out", "Disconnect", !busy, true, t)
                .ml_auto()
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.sign_out(cx))),
        );

    let mut block = section(true, "Garmin", t).child(
        div()
            .flex()
            .items_start()
            .gap(px(12.0))
            .child(dot(view.connected, t))
            .child(
                body(
                    format!("{} {}", view.account_line, view.cache_line),
                    FS_MD,
                    t.fg,
                )
                .line_height(relative(1.6)),
            ),
    );
    if let Some(folder) = &view.data_folder {
        block = block.child(
            div()
                .mt(px(10.0))
                .font_family(MONO)
                .text_size(px(FS_CAPTION))
                .text_color(t.faint)
                .child(folder.clone()),
        );
    }
    block = block.child(actions);
    if let Some(note) = sync_note(root, t) {
        block = block.child(div().mt(px(14.0)).child(note));
    }
    block
}

fn theme_choice(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
    let choices = [
        ("theme-system", "Match system", ThemeMode::System),
        ("theme-light", "Light", ThemeMode::Light),
        ("theme-dark", "Dark", ThemeMode::Dark),
    ];
    let mut row = div().flex().items_baseline().gap(px(26.0));
    for (id, label, mode) in choices {
        let on = root.account.mode == mode;
        let item = quiet(id, label, true, false, t)
            .text_color(if on { t.fg } else { t.faint })
            .when(on, |d| d.font_weight(FontWeight::MEDIUM))
            .on_click(cx.listener(move |root, _: &ClickEvent, window, cx| {
                root.set_theme_mode(mode, window, cx)
            }));
        row = row.child(item);
    }
    row
}

fn appearance(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
    section(false, "Appearance", t)
        .child(div().font_family(SANS).text_size(px(FS_MD)).child("Theme"))
        .child(
            body(
                "Match system follows the setting of your desktop.",
                FS_SMALL,
                t.faint,
            )
            .mt(px(5.0))
            .mb(px(14.0)),
        )
        .child(theme_choice(root, t, cx))
}

pub(crate) fn prose(text: &str, t: &Theme) -> Div {
    body(text.to_string(), FS_MD, t.mutd)
        .line_height(relative(1.7))
        .max_w(ch(62.0, FS_MD))
}

pub fn render(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> AnyElement {
    let Some(view) = root.account.settings.clone() else {
        return div().into_any_element();
    };
    let k = root.visits;
    let about = "Garmin Companion reads your Garmin data into a database on this computer. \
                 You can delete that file at any time. The app is open source.";
    div()
        .child(block_in(k, 0, eyebrow_lg("This machine", t)))
        .child(block_in(k, 0, h1("Settings").mt(px(18.0))))
        .child(block_in(
            k,
            1,
            body(
                "Your Garmin history lives here, in a file you can delete.",
                FS_MD,
                t.mutd,
            )
            .mt(px(18.0))
            .mb(px(52.0))
            .max_w(ch(62.0, FS_MD)),
        ))
        .child(block_in(k, 2, garmin(root, &view, t, cx)))
        .child(block_in(k, 3, crate::cloud::render(root, t, window, cx)))
        .child(block_in(k, 3, appearance(root, t, cx)))
        .child(block_in(
            k,
            4,
            section(false, "What leaves this machine", t).child(prose(&view.privacy_line, t)),
        ))
        .child(block_in(
            k,
            4,
            section(false, "About", t)
                .child(prose(about, t))
                .child(prose(&format!("Version {}.", view.version), t).mt(px(12.0)))
                .child(crate::update_ui::settings_row(root, t, cx)),
        ))
        .into_any_element()
}
