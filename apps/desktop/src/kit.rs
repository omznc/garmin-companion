//! The parts that every screen is built from: the buttons, the chips, the key
//! hints and the charts. The sizes come from the CSS of the design (`.ib`,
//! `.btn`, `.chip`, `.kbd`).

use gpui::{div, prelude::*, px, relative, Div, ElementId, Hsla, SharedString, Stateful};

use crate::icons::{icon_hover, Icon};
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

/// The key that the shortcuts use: Cmd on macOS, Ctrl elsewhere.
pub fn modifier() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

/// `.kbd`: a key hint in a hairline box.
pub fn kbd(text: impl Into<SharedString>, t: &Theme) -> Div {
    div()
        .flex_none()
        .px(px(4.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(t.line)
        .font_family(MONO)
        .text_size(px(11.0))
        .line_height(px(16.0))
        .text_color(t.faint)
        .whitespace_nowrap()
        .child(text.into())
}

/// `.ib`: an icon button, 28 px or 22 px. `on` marks a toggle that is set.
pub fn icon_button(
    id: &'static str,
    glyph: Icon,
    glyph_size: f32,
    small: bool,
    on: bool,
    t: &Theme,
) -> Stateful<Div> {
    let colour = if on { t.fg } else { t.mutd };
    icon_button_shell(id, small, on, t).child(icon_hover(
        glyph,
        glyph_size,
        t,
        colour,
        Some((id, t.fg)),
    ))
}

/// The box of an icon button, for a button whose icon is not a plain one.
pub fn icon_button_shell(id: &'static str, small: bool, on: bool, t: &Theme) -> Stateful<Div> {
    let side = if small { 22.0 } else { 28.0 };
    let hover = t.hover;
    div()
        .id(id)
        .group(id)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(side))
        .rounded(px(if small { 4.0 } else { 6.0 }))
        .cursor_pointer()
        .when(on, |d| d.bg(t.active))
        .when(!on, |d| d.hover(move |s| s.bg(hover)))
}

/// `.chip`: a small label in a hairline box. A dot marks a live state.
pub fn chip(label: impl Into<SharedString>, accent: bool, dot: bool, t: &Theme) -> Div {
    let (fg, border, bg) = if accent {
        (t.acc, t.accline, t.acctint)
    } else {
        (t.mutd, t.line, t.bg2)
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.0))
        .h(px(20.0))
        .px(px(7.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(border)
        .bg(bg)
        .font_family(SANS)
        .text_size(px(11.5))
        .text_color(fg)
        .whitespace_nowrap()
        .when(dot, |d| {
            d.child(div().size(px(6.0)).rounded_full().bg(t.acc))
        })
        .child(label.into())
}

/// How a button looks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Default,
    /// The filled accent button.
    Primary,
    /// The outlined warning button. The screens of Track 3 use it.
    #[allow(dead_code)]
    Danger,
    /// No ground: for a button inside a box.
    Ghost,
}

/// `.btn`: a 24 px button with a label. A disabled button has no pointer.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    look: Look,
    enabled: bool,
    t: &Theme,
) -> Stateful<Div> {
    let (fg, border, bg): (Hsla, Hsla, Hsla) = match look {
        Look::Default => (t.fg, t.line, t.bg2),
        Look::Primary => (t.bg, t.acc, t.acc),
        Look::Danger => (t.warn, t.warnline, t.warntint),
        Look::Ghost => (t.fg, t.line, gpui::transparent_black()),
    };
    let hover = t.hover;
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.0))
        .h(px(24.0))
        .px(px(9.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(border)
        .bg(bg)
        .font_family(SANS)
        .text_size(px(12.0))
        .line_height(relative(1.0))
        .text_color(fg)
        .whitespace_nowrap()
        .when(enabled, |d| {
            d.cursor_pointer().hover(move |s| {
                if look == Look::Primary {
                    s.opacity(0.9)
                } else {
                    s.bg(hover)
                }
            })
        })
        .when(!enabled, |d| d.opacity(0.45))
        .child(label.into())
}
