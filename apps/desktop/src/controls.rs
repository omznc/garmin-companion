//! Buttons for the account screens. From `.cta` and `.quiet` in `styles.css`.

use gpui::{div, prelude::*, px, relative, Div, ElementId, SharedString, Stateful};

use crate::icons::{icon, Icon};
use crate::theme::{Theme, FS_SMALL};
use crate::ui::SANS;

/// `.cta`: the one big action of a screen. 24px serif in the accent, with an
/// arrow. A disabled one is faint and has no pointer.
pub fn cta(
    id: &'static str,
    label: impl Into<SharedString>,
    enabled: bool,
    t: &Theme,
) -> Stateful<Div> {
    let colour = if enabled { t.acc } else { t.faint };
    let fg = t.fg;
    let row = div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(10.0))
        .font_family(SANS)
        .text_size(px(24.0))
        .line_height(relative(1.2))
        .text_color(colour)
        .child(label.into())
        .child(icon(Icon::ArrowRight, 18.0, t, colour));
    if enabled {
        row.cursor_pointer().hover(move |s| s.text_color(fg))
    } else {
        row
    }
}

/// `.quiet`: a small text action. It darkens under the pointer, or takes the
/// accent when it is the one action that cannot be undone.
pub fn quiet(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    enabled: bool,
    danger: bool,
    t: &Theme,
) -> Stateful<Div> {
    let on = if danger { t.acc } else { t.fg };
    let row = div()
        .id(id)
        .font_family(SANS)
        .text_size(px(FS_SMALL))
        .line_height(relative(1.26))
        .text_color(if enabled { t.faint } else { t.line })
        .child(label.into());
    if enabled {
        row.cursor_pointer().hover(move |s| s.text_color(on))
    } else {
        row
    }
}
