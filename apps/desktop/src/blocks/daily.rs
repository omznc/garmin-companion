//! The parts that the Health, Sleep, Food and Weight blocks share: the block
//! wrapper, the line chart with a band, and the helpers for a daily series.

use crate::scale::px;
use chrono::{Datelike, Duration, NaiveDate};
use gpui::{canvas, div, point, prelude::*, relative, ColorExt as _, Div, Pixels, SharedString};

use super::{Block, Go};
use crate::charts::{area_wash, end_dot};
use crate::focus::Focus;
use crate::kit::{self, Cell};
use crate::theme::Theme;
use crate::ui::{stroke, MONO};

/// A block with a meta text, an optional link and a body.
pub fn block(
    meta: Option<String>,
    link: Option<(&'static str, Go)>,
    body: impl IntoElement,
) -> Option<Block> {
    Some(Block {
        meta: meta.map(Into::into),
        link: link.map(|(label, go)| (label.into(), go)),
        body: body.into_any_element(),
    })
}

/// One key figure of a stat grid.
pub fn cell(label: &str, value: String, unit: Option<&str>) -> Cell {
    Cell {
        label: label.to_string().into(),
        value: value.into(),
        unit: unit.map(|u| u.to_string().into()),
        sub: None,
        spark: None,
        big: false,
    }
}

/// A block of key figures in two columns. It hides with no figure.
pub fn key_figures(cells: Vec<Cell>, meta: &str, t: &Theme) -> Option<Block> {
    if cells.is_empty() {
        return None;
    }
    block(Some(meta.to_string()), None, kit::stat_grid(cells, 2, t))
}

/// The block that says why a screen has no data, from a note of the core.
pub fn empty_note(note: &garmin_api::body::EmptyNote, t: &Theme) -> Option<Block> {
    block(
        None,
        None,
        div()
            .p(px(12.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(t.fg)
                    .child(note.title.clone()),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(12.0))
                    .text_color(t.mutd)
                    .child(note.body.clone()),
            ),
    )
}

/// The padding that a body without a chart gets.
pub fn padded(body: impl IntoElement) -> Div {
    div().px(px(12.0)).py(px(12.0)).child(body)
}

/// "1:40" for 6000 seconds.
pub fn hm(secs: f64) -> String {
    let m = (secs / 60.0).round() as i64;
    format!("{}:{:02}", m / 60, m % 60)
}

/// "1,640" for 1640.
pub fn thousands(value: f64) -> String {
    let digits = (value.round() as i64).abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if value < -0.5 {
        out.insert(0, '-');
    }
    out
}

/// The first letter of the weekday of a date.
pub fn weekday_initial(date: NaiveDate) -> SharedString {
    date.weekday().to_string()[..1].to_string().into()
}

/// The meta text of a block with a series: the default text for one day, the
/// range for a longer focus.
pub fn window_meta(focus: Focus, default: &str) -> String {
    if focus.days() > 1 {
        focus.label()
    } else {
        default.to_string()
    }
}

/// A daily series that ends on the end of the focus, as dated points. A range
/// keeps only the days inside it. `values` is the series, oldest first.
pub fn dated(values: &[Option<f64>], focus: Focus) -> Vec<(NaiveDate, Option<f64>)> {
    let n = values.len() as i64;
    values
        .iter()
        .enumerate()
        .map(|(i, v)| (focus.end - Duration::days(n - 1 - i as i64), *v))
        .filter(|(d, _)| focus.days() <= 1 || focus.contains(*d))
        .collect()
}

/// The newest `count` days of a dated series.
pub fn newest(
    series: Vec<(NaiveDate, Option<f64>)>,
    count: usize,
) -> Vec<(NaiveDate, Option<f64>)> {
    let skip = series.len().saturating_sub(count);
    series.into_iter().skip(skip).collect()
}

/// The points of a dated series with a reading, as x between 0 and 1 by day.
pub fn day_points(series: &[(NaiveDate, Option<f64>)]) -> Vec<(f32, f32)> {
    let last = series.len().saturating_sub(1).max(1) as f32;
    series
        .iter()
        .enumerate()
        .filter_map(|(i, (_, v))| Some((i as f32 / last, (*v)? as f32)))
        .collect()
}

/// The first, middle and last date of a series as axis labels. The real
/// today reads "Today".
pub fn date_labels(series: &[(NaiveDate, Option<f64>)]) -> Vec<String> {
    let today = chrono::Local::now().date_naive();
    let text = |d: NaiveDate| {
        if d == today {
            "Today".to_string()
        } else {
            d.format("%-d %b").to_string()
        }
    };
    match series {
        [] => Vec::new(),
        [only] => vec![text(only.0)],
        [first, .., last] => vec![
            text(first.0),
            text(series[series.len() / 2].0),
            text(last.0),
        ],
    }
}

/// The lowest and highest value of a line, with 18 per cent of room on both
/// sides, as the design has it. The band counts.
fn line_range(pts: &[(f32, f32)], band: Option<(f32, f32)>) -> (f32, f32) {
    let mut lo = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let mut hi = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    if let Some((a, b)) = band {
        lo = lo.min(a);
        hi = hi.max(b);
    }
    let span = if hi > lo { hi - lo } else { 1.0 };
    (lo - span * 0.18, hi + span * 0.18)
}

/// A line chart as the design has it: a head, three ticks, an optional band,
/// the wash, the line, optional dots and a dot at the end.
pub struct Line {
    /// x between 0 and 1, and the value. In time order.
    pub pts: Vec<(f32, f32)>,
    pub band: Option<(f32, f32)>,
    pub dots: bool,
    pub height: f32,
    /// Digits after the point in a tick.
    pub decimals: usize,
    /// The figure, its unit and the words beside it.
    pub head: Option<(String, String, String)>,
    pub labels: Vec<String>,
}

pub fn head_row(value: String, unit: String, words: String, t: &Theme) -> Div {
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
                .text_color(t.fg)
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
}

fn ticks(lo: f32, hi: f32, decimals: usize, t: &Theme) -> Vec<Div> {
    [0.8_f32, 0.5, 0.2]
        .map(|f| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(relative(f))
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
                        .child(format!("{:.decimals$}", lo + (hi - lo) * (1.0 - f))),
                )
        })
        .into()
}

pub fn label_row(labels: Vec<String>, t: &Theme) -> Div {
    let last = labels.len().saturating_sub(1);
    labels.into_iter().enumerate().fold(
        div()
            .flex()
            .font_family(MONO)
            .text_size(px(10.5))
            .text_color(t.faint),
        |row, (i, text)| {
            let cell = div().flex_1().whitespace_nowrap().child(text);
            row.child(match i {
                0 => cell,
                i if i == last => cell.text_right(),
                _ => cell.text_center(),
            })
        },
    )
}

pub fn line_chart(c: Line, t: &Theme) -> Div {
    let (lo, hi) = line_range(&c.pts, c.band);
    let y = move |v: f32, h: f32| h - (v - lo) / (hi - lo) * h;
    let (acc, ring, bg) = (t.acc, t.acc.opacity(0.14), t.bg);
    let (pts, dots) = (c.pts.clone(), c.dots);
    let plot = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let h = crate::scale::dp(bounds.size.height);
            let w = crate::scale::dp(bounds.size.width);
            let at = |p: &(f32, f32)| -> gpui::Point<Pixels> {
                point(
                    bounds.origin.x + px(p.0 * w),
                    bounds.origin.y + px(y(p.1, h)),
                )
            };
            let solid: Vec<_> = pts.iter().map(at).collect();
            let Some(last) = solid.last() else { return };
            area_wash(
                window,
                &solid,
                bounds.origin.y + bounds.size.height,
                acc,
                0.34,
            );
            let line: Vec<_> = solid.iter().map(|p| Some(*p)).collect();
            stroke(&line, 2.0, acc, window);
            if dots {
                for p in &solid[..solid.len() - 1] {
                    for (side, colour) in [(7.0, acc), (4.0, bg)] {
                        let b = gpui::Bounds::centered_at(*p, gpui::size(px(side), px(side)));
                        window.paint_quad(
                            gpui::fill(b, colour).corner_radii(gpui::Corners::all(px(side / 2.0))),
                        );
                    }
                }
            }
            end_dot(window, *last, acc, ring);
        },
    )
    .absolute()
    .inset_0();
    let mut area = div()
        .relative()
        .my(px(4.0))
        .h(px(c.height))
        .children(ticks(lo, hi, c.decimals, t));
    if let Some((a, b)) = c.band {
        area = area.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(relative(y(b, 1.0)))
                .h(relative((b - a) / (hi - lo)))
                .bg(t.acctint.opacity(0.55)),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .px(px(12.0))
        .pt(px(12.0))
        .pb(px(10.0))
        .children(
            c.head
                .map(|(value, unit, words)| head_row(value, unit, words, t)),
        )
        .child(area.child(plot))
        .child(label_row(c.labels, t))
}

/// The change from the first to the last reading, as "-4 in 30 days", or
/// "steady" when it is under half a unit.
pub fn change_words(first: f32, last: f32, days: usize, decimals: usize) -> String {
    let d = last - first;
    if d.abs() * 10f32.powi(decimals as i32) < 0.5 {
        return format!("steady over {days} days");
    }
    format!("{d:+.decimals$} in {days} days")
}

/// True when the series has at least two readings: the least that makes a line.
pub fn has_line(series: &[(NaiveDate, Option<f64>)]) -> bool {
    series.iter().filter(|(_, v)| v.is_some()).count() >= 2
}

/// The first and last reading of a series.
pub fn ends(series: &[(NaiveDate, Option<f64>)]) -> Option<(f32, f32)> {
    let mut it = series.iter().filter_map(|(_, v)| *v);
    let first = it.next()?;
    Some((first as f32, it.next_back().unwrap_or(first) as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn a_series_clips_to_the_range() {
        let values: Vec<Option<f64>> = (0..30).map(|i| Some(f64::from(i))).collect();
        let day = Focus::day(d(20));
        let all = dated(&values, day);
        assert_eq!(all.len(), 30);
        assert_eq!(all.last().unwrap().0, d(20));
        let week = dated(&values, Focus::range(d(14), d(20)));
        assert_eq!(week.len(), 7);
        assert_eq!(week[0].1, Some(23.0));
        assert_eq!(newest(all, 14).len(), 14);
    }

    #[test]
    fn figures_read_as_text() {
        assert_eq!(hm(6000.0), "1:40");
        assert_eq!(thousands(1640.0), "1,640");
        assert_eq!(thousands(-2450.0), "-2,450");
        assert_eq!(change_words(52.0, 48.0, 30, 0), "-4 in 30 days");
        assert_eq!(change_words(73.0, 73.02, 90, 1), "steady over 90 days");
        assert_eq!(weekday_initial(d(19)), "M");
    }

    #[test]
    fn the_range_has_room_around_the_band() {
        let (lo, hi) = line_range(&[(0.0, 54.0), (1.0, 62.0)], Some((55.0, 66.0)));
        assert!(lo < 54.0 && hi > 66.0);
    }
}
