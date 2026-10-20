//! The shared pieces of the Today screen: type, metrics, bullets and charts.
//!
//! Each function matches a piece of `app/src/components/ui.tsx` and the
//! classes in `styles.css`. The sizes and the spacing are the same numbers.

use crate::scale::px;
use gpui::{
    div, point, prelude::*, relative, Bounds, Div, ElementId, FontWeight, Hsla, PathBuilder,
    Pixels, Point, SharedString, TextTransform,
};

use crate::theme::{Theme, FS_CAPTION, FS_H1, FS_MICRO};

pub const SANS: &str = "IBM Plex Sans";
pub const MONO: &str = "IBM Plex Mono";

/// The base line height of the design.
pub const LH_NORMAL: f32 = 1.45;

/// The width of a `ch` in IBM Plex Sans, as a share of the font size.
/// `max-width: 58ch` in the current app becomes `ch(58.0, size)`.
pub fn ch(count: f32, font_size: f32) -> Pixels {
    px(count * font_size * 0.6)
}

/// Uppercase and tracked out, as the current app writes its captions.
fn caps(text: impl Into<SharedString>, size: f32, tracking_em: f32, colour: Hsla) -> Div {
    div()
        .font_family(SANS)
        .text_size(px(size))
        .line_height(relative(1.0))
        .letter_spacing(px(size * tracking_em))
        .text_transform(TextTransform::Uppercase)
        .text_color(colour)
        .child(text.into())
}

/// `.eyebrow`: a section marker.
pub fn eyebrow(text: impl Into<SharedString>, t: &Theme) -> Div {
    caps(text, FS_MICRO, 0.12, t.faint)
}

/// `.eyebrow-lg`: the page header's marker.
pub fn eyebrow_lg(text: impl Into<SharedString>, t: &Theme) -> Div {
    caps(text, FS_CAPTION, 0.14, t.faint)
}

/// `.h1`: the page title.
pub fn h1(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(SANS)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(FS_H1))
        .letter_spacing(px(-0.01 * FS_H1))
        .line_height(relative(1.2))
        .child(text.into())
}

/// Body text at one of the six steps, with the current app's default line height.
pub fn body(text: impl Into<SharedString>, size: f32, colour: Hsla) -> Div {
    div()
        .font_family(SANS)
        .text_size(px(size))
        .line_height(relative(LH_NORMAL))
        .text_color(colour)
        .child(text.into())
}

/// `.underlined`: the quiet link.
pub fn underlined(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    size: f32,
    t: &Theme,
) -> gpui::Stateful<Div> {
    // A link with no target yet has no pointer and no hover style.
    let line = t.line;
    div()
        .id(id)
        .font_family(SANS)
        .text_size(px(size))
        .line_height(relative(LH_NORMAL))
        .text_color(t.mutd)
        .pb(px(2.0))
        .border_b_1()
        .border_color(line)
        .whitespace_nowrap()
        .child(text.into())
}

/// Map values to points inside a box, as `scaleFor` in `chart.ts` does. A
/// missing or non-finite value has no point, but it keeps its place: the points
/// sit by index, so a gap stays a gap.
pub(crate) fn scale(
    values: &[Option<f32>],
    bounds: Bounds<Pixels>,
    pad: f32,
) -> Vec<Option<Point<Pixels>>> {
    let present = || values.iter().flatten().filter(|v| v.is_finite());
    let (lo, hi) = present().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
    let range = if hi > lo { hi - lo } else { 1.0 };
    let last = values.len().saturating_sub(1).max(1) as f32;
    let (w, h) = (
        crate::scale::dp(bounds.size.width),
        crate::scale::dp(bounds.size.height),
    );
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let v = v.filter(|v| v.is_finite())?;
            let x = i as f32 / last * w;
            let y = h - pad - (v - lo) / range * (h - pad * 2.0);
            Some(point(bounds.origin.x + px(x), bounds.origin.y + px(y)))
        })
        .collect()
}

/// The unbroken runs of a series. Each one is drawn as its own stroke.
fn runs(points: &[Option<Point<Pixels>>]) -> Vec<Vec<Point<Pixels>>> {
    points
        .split(|p| p.is_none())
        .map(|run| run.iter().flatten().copied().collect::<Vec<_>>())
        .filter(|run| !run.is_empty())
        .collect()
}

pub(crate) fn stroke(
    points: &[Option<Point<Pixels>>],
    width: f32,
    colour: Hsla,
    window: &mut gpui::Window,
) {
    for run in runs(points) {
        let mut path = PathBuilder::stroke(px(width));
        let first = run[0];
        path.move_to(first);
        if run.len() == 1 {
            // A lone sample has no line, so it gets a hairline and the chart is not blank.
            path.line_to(point(first.x + px(2.0), first.y));
        }
        for p in &run[1..] {
            path.line_to(*p);
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, colour);
        }
    }
}
