//! The charts of the block kit: the sparkline, the meter rows, the bar chart
//! and the area chart. The sizes and the colours come from the CSS of the
//! design (`.spark`, `.mrow`, `bars`, `line`).

use gpui::{
    canvas, div, fill, linear_color_stop, linear_gradient, point, prelude::*, px, relative, size,
    Bounds, ColorExt as _, Corners, Div, Hsla, PathBuilder, Pixels, Point, SharedString,
};

use crate::theme::Theme;
use crate::ui::{scale, stroke, MONO, SANS};

/// What a figure or a bar says: nothing special, a good change, or a warning.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Muted,
    Accent,
    Warn,
}

impl Tone {
    pub fn colour(self, t: &Theme) -> Hsla {
        match self {
            Tone::Muted => t.mutd,
            Tone::Accent => t.acc,
            Tone::Warn => t.warn,
        }
    }
}

fn some(values: &[f32]) -> Vec<Option<f32>> {
    values.iter().map(|v| Some(*v)).collect()
}

/// A filled round dot with a soft ring, at the end of a line.
pub(crate) fn end_dot(window: &mut gpui::Window, at: Point<Pixels>, colour: Hsla, ring: Hsla) {
    for (side, tint) in [(13.0, ring), (7.0, colour)] {
        let bounds = Bounds::centered_at(at, size(px(side), px(side)));
        window.paint_quad(fill(bounds, tint).corner_radii(Corners::all(px(side / 2.0))));
    }
}

/// The wash under a line, from the colour at the top to nothing at the bottom.
pub(crate) fn area_wash(
    window: &mut gpui::Window,
    points: &[Point<Pixels>],
    floor: Pixels,
    colour: Hsla,
    top_alpha: f32,
) {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return;
    };
    let mut area = PathBuilder::fill();
    area.move_to(point(first.x, floor));
    for p in points {
        area.line_to(*p);
    }
    area.line_to(point(last.x, floor));
    area.close();
    if let Ok(area) = area.build() {
        let wash = linear_gradient(
            180.0,
            linear_color_stop(colour.opacity(top_alpha), 0.0),
            linear_color_stop(colour.opacity(0.0), 1.0),
        );
        window.paint_path(area, wash);
    }
}

/// `.spark`: 34 px high, a dashed baseline at the mean, the wash, the line
/// and a dot at the end.
pub fn spark(values: &[f32], t: &Theme) -> impl IntoElement {
    let (values, acc, tint, faint) = (some(values), t.acc, t.acctint, t.faint.opacity(0.5));
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let points = scale(&values, bounds, 3.0);
            let solid: Vec<_> = points.iter().flatten().copied().collect();
            if solid.is_empty() {
                return;
            }
            let present: Vec<f32> = values.iter().flatten().copied().collect();
            let mean = present.iter().sum::<f32>() / present.len() as f32;
            let base = scale(
                &[
                    Some(mean),
                    Some(present.iter().copied().fold(f32::MAX, f32::min)),
                    Some(present.iter().copied().fold(f32::MIN, f32::max)),
                ],
                bounds,
                3.0,
            )[0];
            if let Some(base) = base {
                let mut x = f32::from(bounds.origin.x);
                let end = x + f32::from(bounds.size.width);
                while x < end {
                    let dash = Bounds::new(point(px(x), base.y), size(px(2.0), px(1.0)));
                    window.paint_quad(fill(dash, faint));
                    x += 5.0;
                }
            }
            area_wash(
                window,
                &solid,
                bounds.origin.y + bounds.size.height,
                acc,
                0.32,
            );
            stroke(&points, 1.6, acc, window);
            if let Some(last) = solid.last() {
                end_dot(window, *last, acc, tint);
            }
        },
    )
    .w_full()
    .h(px(34.0))
}

/// One row of a meter: a name, a bar, a value and a share.
pub struct MeterRow {
    pub name: SharedString,
    /// The length of the bar, 0 to 1.
    pub fraction: f32,
    pub value: SharedString,
    pub share: SharedString,
    /// Muted for the easy end, warn for the hard end, accent otherwise.
    pub tone: Tone,
}

fn meter_fill(tone: Tone, t: &Theme) -> (Hsla, Hsla) {
    match tone {
        Tone::Accent => (t.acc.opacity(0.5), t.acc),
        Tone::Warn => (t.warn.opacity(0.6), t.warn),
        Tone::Muted => (t.fg.opacity(0.18), t.fg.opacity(0.3)),
    }
}

/// `.mrow`: the rows of a meter, with the room that the design gives them.
pub fn meter_rows(rows: Vec<MeterRow>, t: &Theme) -> Div {
    let mut col = div().flex().flex_col().gap(px(7.0));
    // Every row takes the width of the longest name, value and share, so the
    // bars start and end at the same place. A character is about 6.6 px wide
    // in the name font and 7 px in the mono font.
    let widest = |f: fn(&MeterRow) -> usize| rows.iter().map(f).max().unwrap_or(0) as f32;
    let name_w = (widest(|r| r.name.chars().count()) * 6.6).max(52.0);
    let value_w = widest(|r| r.value.chars().count()) * 7.0;
    let share_w = (widest(|r| r.share.chars().count()) * 7.0).max(30.0);
    for row in rows {
        let (from, to) = meter_fill(row.tone, t);
        let fraction = row.fraction.clamp(0.0, 1.0);
        let fill_bar = div()
            .h_full()
            .w(relative(fraction))
            .rounded(px(3.0))
            .bg(linear_gradient(
                90.0,
                linear_color_stop(from, 0.0),
                linear_color_stop(to, 1.0),
            ));
        col = col.child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .font_family(MONO)
                .text_size(px(11.5))
                .child(
                    div()
                        .flex_none()
                        .w(px(name_w))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .font_family(SANS)
                        .text_size(px(12.5))
                        .text_color(t.mutd)
                        .child(row.name),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h(px(10.0))
                        .rounded(px(3.0))
                        .bg(t.line2)
                        .overflow_hidden()
                        .child(fill_bar),
                )
                .child(
                    div()
                        .flex_none()
                        .w(px(value_w))
                        .text_color(t.fg)
                        .text_right()
                        .child(row.value),
                )
                .child(
                    div()
                        .flex_none()
                        .w(px(share_w))
                        .text_color(t.faint)
                        .text_right()
                        .child(row.share),
                ),
        );
    }
    col
}

/// One bar of a bar chart.
pub struct Bar {
    pub value: f32,
    /// The text above the bar. Empty for a day with nothing.
    pub label: SharedString,
    /// The text under the bar.
    pub day: SharedString,
}

const BAR_CHART_H: f32 = 132.0;
/// The room above the tallest bar for its value label.
const BAR_LABEL_ROOM: f32 = 18.0;

/// The alpha of the bar fill. It reads well on all palettes.
const BAR_ALPHA: f32 = 0.6;

/// `bars`: a chart 132 px high. All bars have one solid fill. The bold axis
/// label marks the last day. A dashed line marks the average. The average
/// label sits above the plot, so no bar or label covers it.
pub fn bar_chart(bars: &[Bar], average: Option<(f32, String)>, t: &Theme) -> Div {
    bar_chart_sized(bars, average, BAR_CHART_H, None, t)
}

/// `bars` with a set height. `full` is the value of a full bar: `None` takes
/// the tallest bar or the line, whichever is higher.
pub fn bar_chart_sized(
    bars: &[Bar],
    average: Option<(f32, String)>,
    chart_h: f32,
    full: Option<f32>,
    t: &Theme,
) -> Div {
    let tallest = bars.iter().map(|b| b.value).fold(0.0_f32, f32::max);
    let line = average.as_ref().map_or(0.0, |a| a.0);
    let max = full.unwrap_or_else(|| tallest.max(line)).max(1.0);
    let room = chart_h - BAR_LABEL_ROOM;
    let gap = if bars.len() > 9 { 5.0 } else { 10.0 };
    let last = bars.len().saturating_sub(1);

    let mut columns = div()
        .absolute()
        .inset_0()
        .px(px(4.0))
        .flex()
        .items_end()
        .gap(px(gap));
    for bar in bars {
        // A bar of zero keeps a stub of 1.5 per cent, so the day is not blank.
        let height = (bar.value / max * room).max(chart_h * 0.015);
        let fill = if bar.value <= 0.0 {
            t.fg.opacity(0.1)
        } else {
            t.acc.opacity(BAR_ALPHA)
        };
        let body = div().w_full().h(px(height)).rounded_t(px(3.0)).bg(fill);
        columns = columns.child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .justify_end()
                .gap(px(4.0))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(10.0))
                        .text_color(t.mutd)
                        .text_center()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(bar.label.clone()),
                )
                .child(body),
        );
    }

    let mut plot = div()
        .relative()
        .h(px(chart_h))
        .border_b_1()
        .border_color(t.line)
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(chart_h / 3.0))
                .h(px(1.0))
                .bg(t.line2),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(chart_h * 2.0 / 3.0))
                .h(px(1.0))
                .bg(t.line2),
        );
    let mut key = None;
    if let Some((value, label)) = average {
        let bottom = value / max * room;
        plot = plot.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(bottom))
                .border_t_1()
                .border_dashed()
                .border_color(t.faint.opacity(0.7)),
        );
        key = Some(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(4.0))
                .font_family(MONO)
                .text_size(px(10.0))
                .text_color(t.faint)
                .child(div().w(px(14.0)).h(px(1.0)).bg(t.faint.opacity(0.7)))
                .child(label),
        );
    }
    plot = plot.child(columns);

    let mut days = div().flex().gap(px(gap)).px(px(4.0));
    for (i, bar) in bars.iter().enumerate() {
        let today = i == last;
        days = days.child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(MONO)
                .text_size(px(10.5))
                .text_color(if today { t.fg } else { t.faint })
                .when(today, |d| d.font_weight(gpui::FontWeight::BOLD))
                .text_center()
                .whitespace_nowrap()
                .child(bar.day.clone()),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .px(px(12.0))
        .pt(px(14.0))
        .pb(px(10.0))
        .children(key)
        .child(plot)
        .child(days)
}

const AREA_H: f32 = 130.0;

/// `line`: a head with a figure, an area chart with the lowest, middle and
/// highest value on the right edge, and the first and last label under it.
pub struct AreaChart<'a> {
    pub values: &'a [f32],
    pub colour: Hsla,
    pub height: Option<f32>,
    /// The figure and its unit, and the words beside it.
    pub head: Option<(SharedString, SharedString, SharedString)>,
    pub head_colour: Option<Hsla>,
    pub x_first: SharedString,
    pub x_last: SharedString,
}

pub fn area_chart(chart: AreaChart, t: &Theme) -> Div {
    let height = chart.height.unwrap_or(AREA_H);
    let present = some(chart.values);
    let (lo, hi) = chart
        .values
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
    let pad = 12.0;
    let (colour, ring) = (chart.colour, chart.colour.opacity(0.14));
    let plot = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let points = scale(&present, bounds, pad);
            let solid: Vec<_> = points.iter().flatten().copied().collect();
            area_wash(
                window,
                &solid,
                bounds.origin.y + bounds.size.height,
                colour,
                0.34,
            );
            stroke(&points, 2.0, colour, window);
            if let Some(last) = solid.last() {
                end_dot(window, *last, colour, ring);
            }
        },
    )
    .absolute()
    .inset_0();
    // The ticks sit at 20, 50 and 80 per cent of the height, as in the design.
    let ticks = [(0.2, hi), (0.5, (hi + lo) / 2.0), (0.8, lo)].map(|(at, value)| {
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(relative(at))
            .border_t_1()
            .border_color(t.line2)
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(-8.0))
                    .pl(px(4.0))
                    .bg(t.bg)
                    .font_family(MONO)
                    .text_size(px(10.0))
                    .text_color(t.faint)
                    .child(format!("{value:.0}")),
            )
    });
    let head = chart.head.map(|(value, unit, words)| {
        div()
            .flex()
            .items_baseline()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .font_family(MONO)
                    .text_size(px(20.0))
                    .text_color(chart.head_colour.unwrap_or(t.fg))
                    .child(value)
                    .child(
                        div()
                            .ml(px(3.0))
                            .text_size(px(11.5))
                            .text_color(t.mutd)
                            .child(unit),
                    ),
            )
            .child(div().text_size(px(12.0)).text_color(t.mutd).child(words))
    });
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .px(px(12.0))
        .pt(px(12.0))
        .pb(px(10.0))
        .children(head)
        .child(
            div()
                .relative()
                .my(px(4.0))
                .h(px(height))
                .children(ticks)
                .child(plot),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .font_family(MONO)
                .text_size(px(10.5))
                .text_color(t.faint)
                .child(chart.x_first)
                .child(chart.x_last),
        )
}
