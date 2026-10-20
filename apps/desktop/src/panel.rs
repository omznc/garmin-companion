//! The building blocks of the Settings design: panel, row, button, chip, note.
//!
//! Each function matches a rule in the design's stylesheet (`.panel`, `.row`,
//! `.btn`, `.chip`, `.note`, `.opt`). The numbers are the same. The `opt` card
//! keeps its size with a padding that compensates for the thicker border. A
//! swatch and a result row use an inset shadow (`appearance::ring`) in place.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{
    div, prelude::*, px, relative, Div, ElementId, FontWeight, Hsla, SharedString, Stateful,
    TextTransform,
};

use crate::icons::{icon, Icon};
use crate::theme::{Theme, FS_BASE, FS_CAPTION, FS_SMALL};
use crate::ui::{body, SANS, SERIF};

/// The content is narrower than the design's 860px rule allows: rows get less
/// padding. The Settings screen sets it each frame from the width of the content.
static NARROW: AtomicBool = AtomicBool::new(false);

pub fn set_narrow(narrow: bool) {
    NARROW.store(narrow, Ordering::Relaxed);
}

pub fn narrow() -> bool {
    NARROW.load(Ordering::Relaxed)
}

/// The padding of a row and of a sub area: 20 by 24, or 18 by 16 when narrow.
pub fn padded(d: Div) -> Div {
    if narrow() {
        d.px(px(16.0)).py(px(18.0))
    } else {
        d.px(px(24.0)).py(px(20.0))
    }
}

/// A flex item as the stylesheet writes `flex: grow shrink basis`.
fn flex(d: Div, grow: f32, basis: f32) -> Div {
    d.flex_grow(grow)
        .flex_shrink(1.0)
        .flex_basis(px(basis))
        .min_w_0()
}

/// `.sec`: a title, an optional lead line, then what the caller adds.
pub fn section(title: &str, lead: Option<&str>, t: &Theme) -> Div {
    let head = div()
        .font_family(SERIF)
        .text_size(px(30.0))
        .line_height(relative(1.1))
        .text_color(t.fg)
        .child(title.to_string());
    let mut sec = div().flex().flex_col().gap(px(16.0)).child(head);
    if let Some(lead) = lead {
        sec = sec.child(body(lead.to_string(), FS_BASE, t.mutd).mt(px(-6.0)));
    }
    sec
}

/// `.panel`: a bordered box that clips its rows to the rounded corners.
pub fn panel(t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .border_1()
        .border_color(t.line)
        .rounded(px(14.0))
        .bg(t.bg)
        .overflow_hidden()
}

/// `.help`: a small grey line.
pub fn help(text: impl Into<SharedString>, t: &Theme) -> Div {
    body(text, 13.5, t.mutd).line_height(relative(1.5))
}

/// The column of a row's right side: a stack that starts at the left edge.
pub fn stack() -> Div {
    div().flex().flex_col().items_start().gap(px(12.0))
}

fn label_column(label: &str, below: Option<Div>, colour: Hsla) -> Div {
    let mut col = flex(div(), 1.0, 180.0)
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(body(label.to_string(), FS_BASE, colour).font_weight(FontWeight::MEDIUM));
    if let Some(below) = below {
        col = col.child(below);
    }
    col
}

fn wrapped(label: Div, value: Div) -> Div {
    padded(div())
        .flex()
        .flex_wrap()
        .gap_x(px(32.0))
        .gap_y(px(12.0))
        .child(label)
        .child(flex(value, 2.4, 320.0))
}

/// `.row`: a label on the left, a value on the right. They wrap into one
/// column when the panel is too narrow for both. The first row has no top line.
/// `below` is the line under the label: usually `help`.
pub fn row_with(first: bool, label: &str, below: Option<Div>, value: Div, t: &Theme) -> Div {
    let r = wrapped(label_column(label, below, t.fg), value);
    if first {
        r
    } else {
        r.border_t_1().border_color(t.line2)
    }
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

/// `.sub`: the grey area under the rows of a panel.
pub fn sub(t: &Theme) -> Div {
    padded(div())
        .flex()
        .flex_col()
        .gap(px(14.0))
        .bg(t.bg2)
        .border_t_1()
        .border_color(t.line2)
}

/// `.actions`: buttons in a wrapping line.
pub fn actions() -> Div {
    div().flex().flex_wrap().items_center().gap(px(10.0))
}

/// `.field`: an input and its button, in a wrapping line.
pub fn field_line() -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(10.0))
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
    /// The one main action: the text colour as ground.
    Primary,
    /// A warning outline.
    Danger,
}

/// The ground, text and outline of a button kind.
fn kind_colours(kind: Kind, t: &Theme) -> (Hsla, Hsla, Hsla) {
    match kind {
        Kind::Plain => (t.bg, t.fg, t.line),
        Kind::Primary => (t.fg, t.bg, t.fg),
        Kind::Danger => (gpui::transparent_black(), t.warn, t.warnline),
    }
}

/// What the pointer does to an enabled button.
fn hover_of(b: Stateful<Div>, kind: Kind, t: &Theme) -> Stateful<Div> {
    let (sel, tint) = (t.sel, t.warntint);
    match kind {
        Kind::Plain => b.hover(move |s| s.bg(sel)),
        Kind::Danger => b.hover(move |s| s.bg(tint)),
        Kind::Primary => b.hover(|s| s.opacity(0.88)),
    }
}

/// `.btn`: 40px high. A disabled one is faint, dashed and has no pointer.
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
        .gap(px(8.0))
        .h(px(40.0))
        .px(px(16.0))
        .rounded(px(9.0))
        .border_1()
        .font_family(SANS)
        .text_size(px(FS_SMALL))
        .line_height(relative(1.0))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap();
    if !enabled {
        b = b
            .opacity(0.4)
            .border_dashed()
            .border_color(t.line)
            .text_color(t.mutd);
    } else {
        b = hover_of(
            b.bg(bg).border_color(line).text_color(fg).cursor_pointer(),
            kind,
            t,
        );
    }
    let colour = if enabled { fg } else { t.mutd };
    if let Some(which) = lead {
        b = b.child(icon(which, 15.0, t, colour));
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

/// `.chip`: a pill with a dot and a short state word.
pub fn chip(label: impl Into<SharedString>, on: bool, t: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.0))
        .h(px(26.0))
        .px(px(10.0))
        .rounded_full()
        .border_1()
        .border_color(t.line)
        .font_family(SANS)
        .text_size(px(FS_CAPTION))
        .line_height(relative(1.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if on { t.fg } else { t.mutd })
        .whitespace_nowrap()
        .child(dot(on, t))
        .child(label.into())
}

/// `.tag`: a small uppercase label. The accent form is for a beta mark.
pub fn tag(label: impl Into<SharedString>, accent: bool, t: &Theme) -> Div {
    let d = div()
        .flex_none()
        .px(px(6.0))
        .py(px(1.0))
        .rounded(px(5.0))
        .border_1()
        .font_family(SANS)
        .text_size(px(11.0))
        .line_height(relative(1.5))
        .font_weight(FontWeight::MEDIUM)
        .letter_spacing(px(11.0 * 0.06))
        .text_transform(TextTransform::Uppercase)
        .whitespace_nowrap();
    let d = if accent {
        d.text_color(t.acc).border_color(t.accline).bg(t.acctint)
    } else {
        d.text_color(t.mutd).border_color(t.line)
    };
    d.child(label.into())
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
        .p(px(3.0))
        .gap(px(3.0))
        .rounded(px(11.0))
        .border_1()
        .border_color(t.line)
        .bg(t.bg2)
}

/// One button of a `seg`. The chosen one lifts onto the page colour.
pub fn seg_item(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on: bool,
    lead: Option<Icon>,
    t: &Theme,
) -> Stateful<Div> {
    let fg = t.fg;
    let colour = if on { t.fg } else { t.mutd };
    let mut b = div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(7.0))
        .h(px(34.0))
        .px(px(13.0))
        .rounded(px(8.0))
        .border_1()
        .font_family(SANS)
        .text_size(px(FS_SMALL))
        .line_height(relative(1.0))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .text_color(colour)
        .cursor_pointer();
    b = if on {
        b.bg(t.bg).border_color(t.line)
    } else {
        b.border_color(gpui::transparent_black())
            .hover(move |s| s.text_color(fg))
    };
    if let Some(which) = lead {
        b = b.child(icon(which, 15.0, t, colour));
    }
    b.child(label.into())
}

/// `.opts`: a wrapping row for `opt` cards.
pub fn opts() -> Div {
    div().flex().flex_wrap().gap(px(10.0))
}

/// `.radio`: a 16px circle, with the accent core when on.
fn radio(on: bool, t: &Theme) -> Div {
    let ring = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(16.0))
        .rounded_full()
        .border_2()
        .border_color(if on { t.acc } else { t.faint });
    if on {
        ring.child(div().size(px(8.0)).rounded_full().bg(t.acc))
    } else {
        ring
    }
}

/// `.opt`: a choice card with a radio, a name, a tag and a description. The
/// chosen card has a 2px accent border. It gives back 1px of padding, so the
/// card keeps its size.
pub fn opt(
    id: impl Into<ElementId>,
    name: &str,
    mark: Option<&str>,
    desc: &str,
    on: bool,
    t: &Theme,
) -> Stateful<Div> {
    let sel = t.sel;
    let give = if on { 1.0 } else { 0.0 };
    let top = div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .min_w_0()
        .child(radio(on, t))
        .child(
            body(name.to_string(), FS_BASE, t.fg)
                .font_weight(FontWeight::MEDIUM)
                .flex_1()
                .min_w_0(),
        );
    let top = match mark {
        Some(text) => top.child(tag(text.to_string(), false, t)),
        None => top,
    };
    let card = div()
        .id(id)
        .flex()
        .flex_col()
        .gap(px(6.0))
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .flex_basis(px(190.0))
        .min_w_0()
        .pt(px(14.0 - give))
        .pb(px(16.0 - give))
        .px(px(16.0 - give))
        .rounded(px(12.0))
        .bg(t.bg)
        .cursor_pointer()
        .hover(move |s| s.bg(sel));
    let card = if on {
        card.border_2().border_color(t.acc)
    } else {
        card.border_1().border_color(t.line)
    };
    card.child(top).child(
        body(desc.to_string(), FS_CAPTION, t.mutd)
            .line_height(relative(1.45))
            .pl(px(26.0)),
    )
}
