//! The Setup screen: connect Garmin. From `StepGarmin` in
//! `app/src/screens/Setup.tsx`.
//!
//! The old app had three steps (Garmin, a model, preferences). The model step
//! belongs to the Ask screen, which comes later. So this is one step.

use crate::scale::px;
use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, Window};

use crate::account::SignIn;
use crate::controls::cta;
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
        .pl(px(16.0))
        .border_l_1()
        .border_color(t.acc)
        .mb(px(36.0))
        .max_w(ch(52.0, FS_BASE))
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
        .child(div().mt(px(12.0)).child(button))
}

pub fn render(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> AnyElement {
    let waiting = matches!(root.account.sign_in, SignIn::Waiting(_));
    let lede = garmin_api::session::sign_in_line();

    let mut column = div()
        .w_full()
        .max_w(px(COLUMN))
        .pt(px(120.0))
        .child(div().mb(px(52.0)).child(eyebrow_lg("Welcome", t)))
        .child(h1("Connect Garmin.").mb(px(12.0)))
        .child(
            body(lede, FS_MD, t.mutd)
                .mb(px(36.0))
                .max_w(ch(52.0, FS_MD)),
        );
    if let Some(path) = &root.account.token_path_default {
        column = column.child(found_file(path, t, cx));
    }
    if let Some(line) = status_line(root, t) {
        column = column.child(div().mb(px(24.0)).child(line));
    }

    let label = if waiting {
        "Waiting for Garmin..."
    } else {
        "Sign in with Garmin"
    };
    let button = cta("sign-in", label, !waiting && !root.account.demo, t)
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.start_sign_in(cx)));

    let field = root.account.token_path.render(
        "token-path",
        t,
        cx,
        window,
        |root: &mut Root| &mut root.account.token_path,
        |root, _, cx| root.import_tokens(cx),
    );
    let import = div()
        .mt(px(64.0))
        .child(
            body(
                "Have a token file from the garminconnect library?",
                FS_SMALL,
                t.faint,
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

    div()
        .size_full()
        .flex()
        .justify_center()
        .px(px(32.0))
        .child(column.child(div().mt(px(40.0)).child(button)).child(import))
        .into_any_element()
}
