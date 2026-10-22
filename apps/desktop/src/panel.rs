//! The building blocks of the Settings design: card, row, button, chip, note.
//!
//! Each function matches a rule in the design's stylesheet (`.card`, `.srow`,
//! `.btn`, `.chip`, `.seg`, `.note`). The numbers are the same.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::scale::px;
use gpui::{div, prelude::*, relative, Div, ElementId, FontWeight, Hsla, SharedString, Stateful};

use crate::icons::{icon, Icon};
use crate::theme::Theme;
use crate::ui::{body, SANS};

/// The content is narrow: rows get less padding. The Settings screen sets it
/// each frame from the width of the content.
static NARROW: AtomicBool = AtomicBool::new(false);

pub fn set_narrow(narrow: bool) {
    NARROW.store(narrow, Ordering::Relaxed);
}

pub fn narrow() -> bool {
    NARROW.load(Ordering::Relaxed)
}

/// The padding of a row and of a sub area: 9 px above and below, none at the
/// sides. The rows stand on the page, so a narrow page needs no other rule.
pub fn padded(d: Div) -> Div {
    d.py(px(if narrow() { 8.0 } else { 9.0 }))
}

/// A flex item as the stylesheet writes `flex: grow shrink basis`.
fn flex(d: Div, grow: f32, basis: f32) -> Div {
    d.flex_grow(grow)
        .flex_shrink(1.0)
        .flex_basis(px(basis))
        .min_w_0()
}

/// A section: a small muted label with an optional chip at the right, then
/// what the caller adds. The rows have no box.
pub fn section(title: &str, chip: Option<Div>, t: &Theme) -> Div {
    let head = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(26.0))
        .child(
            div()
                .flex_1()
                .font_family(SANS)
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .letter_spacing(px(11.5 * 0.02))
                .text_color(t.faint)
                .child(title.to_string()),
        )
        .children(chip);
    div().flex().flex_col().w_full().child(head)
}

/// `.help`: a small grey line.
pub fn help(text: impl Into<SharedString>, t: &Theme) -> Div {
    body(text, 12.0, t.mutd).line_height(relative(1.4))
}

/// The column of a row's right side.
pub fn stack() -> Div {
    div().flex().flex_col().items_end().gap(px(8.0))
}

fn label_column(label: &str, below: Option<Div>, colour: Hsla) -> Div {
    let mut col = flex(div(), 1.0, 260.0)
        .flex()
        .flex_col()
        .child(body(label.to_string(), 13.0, colour).font_weight(FontWeight::MEDIUM));
    if let Some(below) = below {
        col = col.child(below);
    }
    col
}

fn wrapped(label: Div, value: Div) -> Div {
    padded(div())
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(px(24.0))
        .gap_y(px(8.0))
        .child(label)
        .child(value.flex_none().max_w_full())
}

/// `.srow`: a label on the left, controls on the right. They wrap into one
/// column when the page is too narrow for both.
/// `below` is the line under the label: usually `help`.
pub fn row_with(_first: bool, label: &str, below: Option<Div>, value: Div, t: &Theme) -> Div {
    // Every row has a hairline above it. The first one sets off the section label.
    wrapped(label_column(label, below, t.fg), value)
        .border_t_1()
        .border_color(t.line2)
}

/// A row with a `note` on its own full-width line under the label and the
/// controls. A long note wraps there and cannot push out of the row.
pub fn row_note(
    label: &str,
    help_text: Option<&str>,
    note: Option<Div>,
    value: Div,
    t: &Theme,
) -> Div {
    let help_line = help_text.map(|h| help(h.to_string(), t));
    row_with(false, label, help_line, value, t).children(note.map(|n| n.w_full()))
}

pub fn row(first: bool, label: &str, help_text: Option<&str>, value: Div, t: &Theme) -> Div {
    row_with(
        first,
        label,
        help_text.map(|h| help(h.to_string(), t)),
        value,
        t,
    )
}

/// `.sub`: a block of forms and notes, set off by a hairline.
pub fn sub(t: &Theme) -> Div {
    padded(div())
        .flex()
        .flex_col()
        .gap(px(10.0))
        .border_t_1()
        .border_color(t.line2)
}

/// `.c`: controls in a wrapping line.
pub fn actions() -> Div {
    div().flex().flex_wrap().items_center().gap(px(6.0))
}

/// `.field`: an input and its button, in a wrapping line.
pub fn field_line() -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .w_full()
}

/// `.input`: the box that holds a `Field` in a `field_line`.
pub fn input_box(inner: impl IntoElement) -> Div {
    flex(div(), 1.0, 220.0).child(inner)
}

/* ---------------------------------------------------------------- buttons --- */

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Plain,
    /// The one main action: the accent as ground.
    Primary,
    /// A warning outline.
    Danger,
}

/// The ground, text and outline of a button kind.
fn kind_colours(kind: Kind, t: &Theme) -> (Hsla, Hsla, Hsla) {
    match kind {
        Kind::Plain => (t.bg2, t.fg, t.line),
        Kind::Primary => (t.acc, t.bg, t.acc),
        Kind::Danger => (gpui::transparent_black(), t.warn, t.warnline),
    }
}

/// What the pointer does to an enabled button.
fn hover_of(b: Stateful<Div>, kind: Kind, t: &Theme) -> Stateful<Div> {
    let hover = t.hover;
    match kind {
        Kind::Primary => b.hover(|s| s.opacity(0.9)),
        _ => b.hover(move |s| s.bg(hover)),
    }
}

/// `.btn`: 24px high. A disabled one is faint and has no pointer.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: Kind,
    enabled: bool,
    lead: Option<Icon>,
    t: &Theme,
) -> Stateful<Div> {
    let (bg, fg, line) = kind_colours(kind, t);
    let mut b = div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .h(px(24.0))
        .px(px(9.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(line)
        .bg(bg)
        .text_color(fg)
        .font_family(SANS)
        .text_size(px(12.0))
        .line_height(relative(1.0))
        .whitespace_nowrap();
    b = if enabled {
        hover_of(b.cursor_pointer(), kind, t)
    } else {
        b.opacity(0.45)
    };
    if let Some(which) = lead {
        b = b.child(icon(which, 13.0, t, fg));
    }
    b.child(label.into())
}

/* ------------------------------------------------------------ small pieces --- */

/// `.dot`: 7px. When on it is the accent with a ring of the accent wash.
pub fn dot(on: bool, t: &Theme) -> Div {
    let core = div()
        .size(px(7.0))
        .rounded_full()
        .bg(if on { t.acc } else { t.faint });
    let base = div().relative().flex_none().size(px(7.0));
    if !on {
        return base.child(core);
    }
    let ring = div()
        .absolute()
        .top(px(-3.0))
        .left(px(-3.0))
        .size(px(13.0))
        .rounded_full()
        .bg(t.acctint);
    base.child(ring).child(core)
}

/// `.chip`: a small label in a hairline box. `on` gives it the accent.
pub fn chip(label: impl Into<SharedString>, on: bool, t: &Theme) -> Div {
    crate::kit::chip(label, on, false, t)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    /// Work in progress: the accent dot.
    Run,
    Ok,
    Err,
    Info,
}

/// The mark at the start of a note: an icon, or the accent dot while it runs.
fn note_mark(tone: Tone, t: &Theme) -> Div {
    let (which, colour) = match tone {
        Tone::Run => return div().mt(px(5.0)).child(dot(true, t)),
        Tone::Ok => (Icon::Check, t.acc),
        Tone::Err => (Icon::Warning, t.warn),
        Tone::Info => (Icon::Info, t.mutd),
    };
    div().mt(px(1.0)).child(icon(which, 16.0, t, colour))
}

/// `.note`: a line of status text with a mark. An error has its own box.
pub fn note(tone: Tone, text: impl Into<SharedString>, t: &Theme) -> Div {
    let colour = match tone {
        Tone::Ok => t.fg,
        Tone::Err => t.warn,
        _ => t.mutd,
    };
    let mut d = div().flex().items_start().gap(px(8.0)).min_w_0();
    d = match tone {
        Tone::Err => d
            .px(px(12.0))
            .py(px(9.0))
            .rounded(px(9.0))
            .border_1()
            .border_color(t.warnline)
            .bg(t.warntint),
        Tone::Info => d
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(9.0))
            .border_1()
            .border_dashed()
            .border_color(t.line),
        _ => d,
    };
    d.child(note_mark(tone, t).flex_none()).child(
        body(text, 13.5, colour)
            .line_height(relative(1.45))
            .flex_1()
            .min_w_0(),
    )
}

/// `.seg`: a segmented control. The caller adds `seg_item`s.
pub fn seg(t: &Theme) -> Div {
    div()
        .flex()
        .flex_wrap()
        .p(px(2.0))
        .gap(px(2.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(t.line)
        .bg(t.bg)
}

/// One button of a `seg`. The chosen one has the `active` ground. The design
/// has no icon in a segment, so `_lead` is not drawn.
pub fn seg_item(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on: bool,
    _lead: Option<Icon>,
    t: &Theme,
) -> Stateful<Div> {
    let (hover, fg) = (t.hover, t.fg);
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(22.0))
        .px(px(9.0))
        .rounded(px(4.0))
        .cursor_pointer()
        .font_family(SANS)
        .text_size(px(12.0))
        .line_height(relative(1.0))
        .whitespace_nowrap()
        .text_color(if on { t.fg } else { t.mutd })
        .when(on, |d| d.bg(t.active))
        .when(!on, |d| d.hover(move |s| s.bg(hover).text_color(fg)))
        .child(label.into())
}
