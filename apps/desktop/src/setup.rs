//! The Setup screen: connect Garmin. From `StepGarmin` in
//! `app/src/screens/Setup.tsx`.
//!
//! The old app had three steps (Garmin, a model, preferences). The model step
//! belongs to the Ask screen, which comes later. So this is one step.

use crate::scale::px;
use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, Window};

use crate::account::SignIn;
use crate::kit::{button, card, Look};
use crate::shell::Root;
use crate::theme::{Theme, FS_BASE, FS_CAPTION, FS_MD, FS_SMALL};
use crate::ui::{body, ch, eyebrow_lg, h1, underlined, MONO, SANS};

const COLUMN: f32 = 460.0;

fn status_line(root: &Root, t: &Theme) -> Option<AnyElement> {
    match &root.account.sign_in {
        SignIn::Idle => None,
        SignIn::Waiting(line) => Some(body(line.clone(), FS_BASE, t.mutd).into_any_element()),
        SignIn::Failed(error) => Some(body(error.clone(), FS_BASE, t.warn).into_any_element()),
    }
}

/// The note that offers an existing token file, when one is on the disk.
fn found_file(path: &str, t: &Theme, cx: &mut Context<Root>) -> impl IntoElement {
    let mut button = underlined("import-found", "Import those tokens", FS_SMALL, t);
    button = button
        .cursor_pointer()
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| {
            root.account.token_path.line.clear();
            root.import_tokens(cx);
        }));
    div()
        .pl(px(12.0))
        .border_l_1()
        .border_color(t.acc)
        .mb(px(14.0))
        .child(
            div()
                .font_family(SANS)
                .text_size(px(FS_BASE))
                .line_height(gpui::relative(1.65))
                .text_color(t.mutd)
                .child("Found an existing token file at ")
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(FS_CAPTION))
                        .child(path.to_string()),
                )
                .child("Importing it skips the sign-in."),
        )
        .child(div().mt(px(8.0)).child(button))
}

/// A debug screenshot run with `GC_SHOW_SETUP` shows the button as enabled.
/// A click stays a no-op, because a demo run blocks the sign-in.
fn preview_enabled() -> bool {
    cfg!(debug_assertions) && std::env::var("GC_SHOW_SETUP").is_ok()
}

/// The card with the one primary action.
fn sign_in_card(root: &Root, t: &Theme, cx: &mut Context<Root>) -> impl IntoElement {
    let waiting = matches!(root.account.sign_in, SignIn::Waiting(_));
    let label = if waiting {
        "Waiting for Garmin..."
    } else {
        "Sign in with Garmin"
    };
    let action = button(
        "sign-in",
        label,
        Look::Primary,
        !waiting && (!root.account.demo || preview_enabled()),
        t,
    )
    .h(px(30.0))
    .px(px(14.0))
    .text_size(px(FS_BASE))
    .font_weight(gpui::FontWeight::MEDIUM)
    .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.start_sign_in(cx)));
    let mut row = div().flex().items_center().gap(px(14.0)).child(action);
    if let Some(line) = status_line(root, t) {
        row = row.child(div().flex_1().min_w_0().child(line));
    }
    let content = div()
        .p(px(16.0))
        .child(
            body(garmin_api::session::sign_in_line(), FS_MD, t.mutd)
                .mb(px(16.0))
                .max_w(ch(52.0, FS_MD)),
        )
        .child(row);
    card("Garmin account", None, None, content, t)
}

/// The card that takes a token file from the garminconnect library.
fn token_card(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> impl IntoElement {
    let field = root.account.token_path.render(
        "token-path",
        t,
        cx,
        window,
        |root: &mut Root| &mut root.account.token_path,
        |root, _, cx| root.import_tokens(cx),
    );
    let mut content = div().p(px(16.0));
    if let Some(path) = &root.account.token_path_default {
        content = content.child(found_file(path, t, cx));
    }
    let content = content
        .child(
            body(
                "Have a token file from the garminconnect library?",
                FS_SMALL,
                t.mutd,
            )
            .mb(px(10.0)),
        )
        .child(field)
        .child(
            body(
                "Type the path and press Enter. Leave it empty to use the default path.",
                FS_CAPTION,
                t.faint,
            )
            .mt(px(8.0)),
        );
    card("Token file", None, None, content, t)
}

pub fn render(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> AnyElement {
    let column = div()
        .w_full()
        .max_w(px(COLUMN))
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(
            div()
                .child(div().mb(px(14.0)).child(eyebrow_lg("Welcome", t)))
                .child(h1("Connect Garmin.")),
        )
        .child(sign_in_card(root, t, cx))
        .child(token_card(root, t, window, cx));

    div()
        .id("setup")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .flex()
        .justify_center()
        .px(px(32.0))
        .pt(px(88.0))
        .child(column)
        .into_any_element()
}
