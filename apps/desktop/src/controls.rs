//! Buttons for the account screens. From `.quiet` in `styles.css`.

use crate::scale::px;
use gpui::{div, prelude::*, relative, Div, ElementId, SharedString, Stateful};

use crate::theme::{Theme, FS_SMALL};
use crate::ui::SANS;

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
