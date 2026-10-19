//! The shared pieces of the Today screen: type, metrics, bullets and charts.
//!
//! Each function matches a piece of `app/src/components/ui.tsx` and the
//! classes in `styles.css`. The sizes and the spacing are the same numbers.

use std::time::Duration;

use gpui::{
    canvas, div, linear_color_stop, linear_gradient, point, prelude::*, px, relative, Animation,
    AnimationExt, Bounds, ColorExt as _, Div, ElementId, Hsla, PathBuilder, Pixels, Point,
    SharedString, TextTransform,
};

use crate::data::{Attention, Metric};
use crate::spring::ease_out;
use crate::theme::{Theme, DUR_SLOW, FS_CAPTION, FS_MD, FS_MICRO, FS_SMALL};

pub const SERIF: &str = "Instrument Serif";
pub const SANS: &str = "Instrument Sans";
pub const MONO: &str = "Geist Mono";

/// What a browser calls `line-height: normal` for Instrument Sans.
pub const LH_NORMAL: f32 = 1.26;

/// The width of a `ch` in Instrument Sans, as a share of the font size.
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

/// `.h1`: the page title in the display face.
pub fn h1(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(SERIF)
        .text_size(px(38.0))
        .line_height(relative(1.1))
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

/// The smaller unit that trails a figure, such as the "h" in "7h 02m".
fn unit_size(text: &str) -> f32 {
    if text.trim().chars().count() == 1 {
        26.0
    } else {
        21.0
    }
}

/// A large mono figure over a small uppercase caption.
pub fn metric(m: &Metric, t: &Theme) -> Div {
    let colour = if m.accent { t.acc } else { t.fg };
    let value = m.value.iter().fold(
        div()
            .flex()
            .items_baseline()
            .font_family(MONO)
            .line_height(relative(1.0))
            .letter_spacing(px(-0.04 * 38.0))
            .text_color(colour),
        |row, part| {
            let size = if part.unit {
                unit_size(&part.text)
            } else {
                38.0
            };
            row.child(
                div()
                    .text_size(px(size))
                    .child(part.text.clone())
                    .whitespace_nowrap(),
            )
        },
    );
    let label = div()
        .mt(px(9.0))
        .font_family(SANS)
        .text_size(px(FS_MICRO))
        .line_height(relative(1.35))
        .letter_spacing(px(FS_MICRO * 0.11))
        .text_transform(TextTransform::Uppercase)
        .text_color(t.mutd)
        .child(m.label.clone());
    div().flex_none().child(value).child(label)
}

/// A figure from the strip under the last session: 20px mono over a caption.
pub fn small_stat(label: &str, value: &str, t: &Theme) -> Div {
    div()
        .flex_none()
        .child(
            div()
                .font_family(MONO)
                .text_size(px(20.0))
                .line_height(relative(LH_NORMAL))
                .letter_spacing(px(-0.03 * 20.0))
                .text_color(t.fg)
                .child(value.to_string()),
        )
        .child(caps(label.to_string(), FS_MICRO, 0.11, t.faint).mt(px(8.0)))
}

/// The dotted list used for Attention.
pub fn bullet(index: usize, item: &Attention, t: &Theme) -> Div {
    // Level with the first line. Text and dot do not share a baseline here.
    let dot = div()
        .flex_none()
        .mt(px(10.0))
        .size(px(4.0))
        .rounded_full()
        .bg(if item.accent { t.acc } else { t.faint });
    let text = div()
        .flex_1()
        .min_w_0()
        .font_family(SANS)
        .text_size(px(FS_MD))
        .line_height(relative(1.55))
        .child(item.text.clone());
    let mut row = div()
        .flex()
        .gap(px(14.0))
        .items_start()
        .child(dot)
        .child(text);
    let has_spark = item.spark.iter().any(Option::is_some);
    if has_spark || item.link.is_some() {
        // At the end of the last line, where the current app sets it inline.
        let mut tail = div()
            .flex_none()
            .self_end()
            .flex()
            .items_center()
            .gap(px(6.0));
        if has_spark {
            tail = tail.child(spark(&item.spark, t));
        }
        if let Some(link) = &item.link {
            tail = tail.child(underlined(
                ("attention-link", index),
                link.label.clone(),
                FS_SMALL,
                t,
            ));
        }
        row = row.child(tail);
    }
    row
}

/// Map values to points inside a box, as `scaleFor` in `chart.ts` does. A
/// missing or non-finite value has no point, but it keeps its place: the points
/// sit by index, so a gap stays a gap.
fn scale(values: &[Option<f32>], bounds: Bounds<Pixels>, pad: f32) -> Vec<Option<Point<Pixels>>> {
    let present = || values.iter().flatten().filter(|v| v.is_finite());
    let (lo, hi) = present().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
    let range = if hi > lo { hi - lo } else { 1.0 };
    let last = values.len().saturating_sub(1).max(1) as f32;
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
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

fn stroke(points: &[Option<Point<Pixels>>], width: f32, colour: Hsla, window: &mut gpui::Window) {
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

fn spark_canvas(values: &[Option<f32>], t: &Theme) -> gpui::Canvas<()> {
    let (values, acc) = (values.to_vec(), t.acc);
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| stroke(&scale(&values, bounds, 3.0), 1.25, acc, window),
    )
}

/// An inline sparkline: 90 by 20, accent stroke.
pub fn spark(values: &[Option<f32>], t: &Theme) -> impl IntoElement {
    spark_canvas(values, t).w(px(90.0)).h(px(20.0)).flex_none()
}

/// A sparkline that fills the width of its row.
pub fn spark_wide(values: &[Option<f32>], t: &Theme) -> impl IntoElement {
    spark_canvas(values, t).w_full().h(px(30.0))
}

/// The page's one chart: a baseline, a wash under the line, and the line.
pub fn line_chart(values: &[f32], height: f32, pad: f32, t: &Theme) -> impl IntoElement {
    let (values, acc, line2) = (
        values.iter().map(|v| Some(*v)).collect::<Vec<_>>(),
        t.acc,
        t.line2,
    );
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let base = bounds.origin.y + bounds.size.height - px(1.0);
            let rule = Bounds::new(
                point(bounds.origin.x, base),
                gpui::size(bounds.size.width, px(1.0)),
            );
            window.paint_quad(gpui::fill(rule, line2));

            let points = scale(&values, bounds, pad);
            // This series has no gaps, so the area under it is one shape.
            let solid: Vec<_> = points.iter().flatten().copied().collect();
            let (Some(first), Some(last)) = (solid.first(), solid.last()) else {
                return;
            };
            let mut area = PathBuilder::fill();
            area.move_to(point(first.x, bounds.origin.y + bounds.size.height));
            for p in &solid {
                area.line_to(*p);
            }
            area.line_to(point(last.x, bounds.origin.y + bounds.size.height));
            area.close();
            if let Ok(area) = area.build() {
                let wash = linear_gradient(
                    180.0,
                    linear_color_stop(acc.opacity(0.2), 0.0),
                    linear_color_stop(acc.opacity(0.0), 1.0),
                );
                window.paint_path(area, wash);
            }
            stroke(&points, 1.3, acc, window);
        },
    )
    .w_full()
    .h(px(height))
}

/// The labels under a chart, spread across its width.
pub fn axis_labels(labels: &[String], t: &Theme) -> Div {
    labels.iter().fold(
        div()
            .flex()
            .justify_between()
            .mt(px(9.0))
            .font_family(SANS)
            .text_size(px(FS_MICRO))
            .line_height(relative(1.0))
            .text_color(t.faint),
        |row, label| row.child(label.clone()),
    )
}

/// `block-in`: a block that rises 6px and fades in, after a delay.
///
/// `key` changes on each navigation, so the animation runs again. The cascade
/// stops at the fifth block, as in the stylesheet.
pub fn block_in(key: usize, index: usize, child: impl IntoElement) -> impl IntoElement {
    // Reduced motion shows the block at once: one millisecond, with no cascade.
    let reduced = crate::motion::reduced();
    let delay = if reduced {
        0.0
    } else {
        (index.min(4) as f32) * 28.0
    };
    let total = if reduced {
        1.0
    } else {
        DUR_SLOW as f32 + delay
    };
    let slow = if reduced { 1.0 } else { DUR_SLOW as f32 };
    div().relative().child(child).with_animation(
        ElementId::NamedInteger("block".into(), (key * 64 + index) as u64),
        Animation::new(Duration::from_millis(total as u64))
            .with_easing(move |x| ease_out(((x * total - delay) / slow).clamp(0.0, 1.0))),
        |el, t| el.opacity(t).top(px(6.0 * (1.0 - t))),
    )
}
