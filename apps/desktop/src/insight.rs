//! The detail page of one Insights item: what was found, the data behind it,
//! what to do, and the sessions that it counted. The text and the figures come
//! from `garmin-api` and `garmin-core`. This file lays them out with the kit.

use chrono::NaiveDate;
use garmin_api::insights::{InsightItem, InsightRow, InsightSection, InsightSeries};
use garmin_core::findings::{Tone as FindingTone, Unit};
use gpui::{
    canvas, div, prelude::*, relative, AnyElement, ClickEvent, Context, Div, Hsla, SharedString,
};

use crate::blocks::{self, insights::age};
use crate::charts::{self, Bar};
use crate::kit::{self, ListRow, Look, RowClick};
use crate::scale::px;
use crate::shell::{page_heading, Root};
use crate::theme::Theme;
use crate::training::Training;
use crate::ui::{scale, stroke, MONO};

/// The longest crumb, in characters.
const CRUMB_MAX: usize = 60;
/// A chart of one series with this many points or fewer is a bar chart.
const BAR_MAX: usize = 7;
const CHART_H: f32 = 150.0;

/// The item with this id and the section that holds it.
pub fn find<'a>(
    training: Option<&'a Training>,
    id: &str,
) -> Option<(&'a InsightSection, &'a InsightItem)> {
    training?
        .insights
        .sections
        .iter()
        .find_map(|s| s.items.iter().find(|i| i.id == id).map(|i| (s, i)))
}

/// The claim cut short, with no full stop at the end.
fn short(claim: &str) -> String {
    let claim = claim.trim_end_matches('.');
    if claim.chars().count() <= CRUMB_MAX {
        return claim.to_string();
    }
    let cut: String = claim.chars().take(CRUMB_MAX).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}...")
}

/// The text under the screen name in the crumb bar.
pub fn crumb(training: Option<&Training>, id: &str) -> String {
    find(training, id).map_or_else(|| "Insight".into(), |(_, item)| short(&item.claim))
}

/// A click on a row that opens the detail page of an item.
pub fn open_insight(id: String) -> RowClick {
    Box::new(move |_, cx| {
        let Some(root) = cx.try_global::<blocks::RootHandle>().map(|h| h.0.clone()) else {
            return;
        };
        let id = id.clone();
        let _ = root.update(cx, |root, cx| root.open_insight(id, cx));
    })
}

/// The rows of a list, and a click on a row that has one. The kit list has no click.
pub fn clickable_rows(rows: Vec<(ListRow, Option<RowClick>)>, t: &Theme) -> Div {
    let mut col = div().flex().flex_col();
    for (i, (row, click)) in rows.into_iter().enumerate() {
        let mut line = div()
            .id(("insight-row", i))
            .when(i > 0, |d| d.border_t_1().border_color(t.line2))
            .child(kit::list_rows(vec![row], t));
        if let Some(click) = click {
            line = line
                .cursor_pointer()
                .on_click(move |_, window, cx| click(window, cx));
        }
        col = col.child(line);
    }
    col
}

/* ----------------------------------------------------------------- figures --- */

fn pace(minutes: f64) -> String {
    let total = (minutes * 60.0).round() as i64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// A value of a series, written as the series writes it.
fn format_value(unit: Option<Unit>, v: f64) -> String {
    match unit {
        Some(Unit::Pace) => pace(v),
        Some(Unit::Pct) => format!("{v:.0}%"),
        Some(Unit::PerBeat) => format!("{v:.2}"),
        _ if v.abs() >= 100.0 => format!("{v:.0}"),
        _ => format!("{v:.1}"),
    }
}

/// A date label as "4 Mar". Other labels stay as they are.
fn short_label(label: &str) -> String {
    NaiveDate::parse_from_str(label, "%Y-%m-%d")
        .map(|d| d.format("%-d %b").to_string())
        .unwrap_or_else(|_| label.to_string())
}

/// The estimate in a sentence. The unit depends on what the finding measures.
fn estimate_text(item: &InsightItem) -> Option<String> {
    let e = item.estimate?;
    let (scale, unit) = match item.id.as_str() {
        "fitness-at-fixed-hr" | "cadence-lever" => (60.0, " s per km"),
        "easy-share-trend" => (1.0, " points a month"),
        _ => (1.0, ""),
    };
    let number = |v: f64| format!("{:.2}", v * scale);
    Some(format!(
        "Estimate: {}{unit}. The 90% range is from {} to {}. It rests on {} observations.",
        number(e.value),
        number(e.low),
        number(e.high),
        e.n
    ))
}

fn tone_words(tone: FindingTone) -> &'static str {
    match tone {
        FindingTone::Good => "Good news",
        FindingTone::Note => "Worth noting",
        FindingTone::Watch => "Worth watching",
    }
}

/* ------------------------------------------------------------------- cards --- */

fn padded(body: impl IntoElement) -> Div {
    div().px(px(12.0)).py(px(12.0)).child(body)
}

fn paragraph(text: String, colour: Hsla, size: f32) -> Div {
    div().text_size(px(size)).text_color(colour).child(text)
}

fn found_card(item: &InsightItem, t: &Theme) -> Div {
    let mut body =
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(paragraph(item.detail.clone(), t.fg, 13.0));
    if let Some(basis) = &item.basis {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .child(paragraph("What was counted".into(), t.faint, 11.5))
                .child(paragraph(basis.clone(), t.mutd, 12.5)),
        );
    }
    if let Some(text) = estimate_text(item) {
        body = body.child(paragraph(text, t.mutd, 12.5));
    }
    kit::card("What we found", None, None, padded(body), t)
}

fn action_card(item: &InsightItem, t: &Theme) -> Option<Div> {
    let action = item.action.clone()?;
    let body = padded(paragraph(action, t.fg, 13.0));
    Some(kit::card("What to do", None, None, body, t))
}

/// One line of a chart: the values, the colour and the width of the stroke.
struct Line {
    values: Vec<Option<f32>>,
    colour: Hsla,
    width: f32,
}

/// The lines of the series. The comparison line is first, so the subject sits on top.
/// An inverted series is negated, so low values are drawn at the top.
fn lines(series: &[InsightSeries], t: &Theme) -> Vec<Line> {
    let mut out: Vec<Line> = series
        .iter()
        .filter(|s| s.values.iter().flatten().count() >= 2)
        .map(|s| Line {
            values: s
                .values
                .iter()
                .map(|v| v.map(|v| if s.invert { -v } else { v } as f32))
                .collect(),
            colour: if s.muted { t.faint } else { t.acc },
            width: if s.muted { 1.4 } else { 2.0 },
        })
        .collect();
    out.sort_by_key(|l| l.colour == t.acc);
    out
}

/// The grid lines at 20, 50 and 80 per cent, with the value of the subject at each.
fn grid_lines(subject: &InsightSeries, t: &Theme) -> Vec<Div> {
    let present: Vec<f64> = subject.values.iter().flatten().copied().collect();
    let lo = present.iter().copied().fold(f64::MAX, f64::min);
    let hi = present.iter().copied().fold(f64::MIN, f64::max);
    let (top, bottom) = if subject.invert { (lo, hi) } else { (hi, lo) };
    [(0.2, top), (0.5, (top + bottom) / 2.0), (0.8, bottom)]
        .map(|(at, value)| {
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
                        .child(format_value(subject.format, value)),
                )
        })
        .into()
}

fn legend(series: &[InsightSeries], t: &Theme) -> Div {
    let mut row = div()
        .flex()
        .gap(px(14.0))
        .font_family(MONO)
        .text_size(px(11.0));
    for s in series {
        let colour = if s.muted { t.faint } else { t.acc };
        row = row.child(
            div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .text_color(t.mutd)
                .child(div().w(px(8.0)).h(px(8.0)).rounded(px(4.0)).bg(colour))
                .child(s.name.clone()),
        );
    }
    row
}

/// The first and the last label under the chart.
fn x_labels(labels: &[String], t: &Theme) -> Div {
    let (first, last) = (labels.first(), labels.last());
    div()
        .flex()
        .justify_between()
        .font_family(MONO)
        .text_size(px(10.5))
        .text_color(t.faint)
        .child(first.map(|l| short_label(l)).unwrap_or_default())
        .child(last.map(|l| short_label(l)).unwrap_or_default())
}

/// A bar chart of one short series, such as the days of a week.
fn bars(item: &InsightItem, series: &InsightSeries, t: &Theme) -> Div {
    let bars: Vec<Bar> = series
        .values
        .iter()
        .enumerate()
        .map(|(i, v)| Bar {
            value: v.unwrap_or(0.0) as f32,
            label: v
                .map(|v| format_value(series.format, v))
                .unwrap_or_default()
                .into(),
            day: item
                .labels
                .get(i)
                .map(|l| short_label(l))
                .unwrap_or_default()
                .into(),
        })
        .collect();
    charts::bar_chart(&bars, None, t)
}

/// A line chart of the series. `None` when no series has two points.
fn line_chart(item: &InsightItem, t: &Theme) -> Option<Div> {
    let lines = lines(&item.series, t);
    let subject = item.series.iter().find(|s| !s.muted)?;
    if lines.is_empty() {
        return None;
    }
    let plot = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            for line in &lines {
                stroke(
                    &scale(&line.values, bounds, 12.0),
                    line.width,
                    line.colour,
                    window,
                );
            }
        },
    )
    .absolute()
    .inset_0();
    let area = div()
        .relative()
        .my(px(4.0))
        .h(px(CHART_H))
        .children(grid_lines(subject, t))
        .child(plot);
    Some(
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(area)
            .child(x_labels(
                if item.labels.is_empty() {
                    &item.ends
                } else {
                    &item.labels
                },
                t,
            ))
            .child(legend(&item.series, t)),
    )
}

fn chart(item: &InsightItem, t: &Theme) -> Option<Div> {
    let drawn = item.series.iter().filter(|s| !s.muted).count() == 1 && item.series.len() == 1;
    match item.series.first() {
        Some(s) if drawn && s.values.len() <= BAR_MAX && !s.values.is_empty() => {
            Some(div().child(bars(item, s, t)).child(legend(&item.series, t)))
        }
        _ => line_chart(item, t),
    }
}

fn data_row(i: usize, row: &InsightRow, t: &Theme) -> Div {
    let value = if row.accent { t.acc } else { t.fg };
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(8.0))
        .text_size(px(13.0))
        .when(i > 0, |d| d.border_t_1().border_color(t.line2))
        .when(row.accent, |d| d.bg(t.acctint))
        .child(
            div().flex_1().min_w_0().child(row.label.clone()).children(
                row.note
                    .clone()
                    .map(|n| div().text_size(px(12.0)).text_color(t.faint).child(n)),
            ),
        )
        .child(
            div()
                .flex_none()
                .font_family(MONO)
                .text_color(value)
                .child(row.value.clone()),
        )
}

fn data_card(item: &InsightItem, t: &Theme) -> Option<Div> {
    let chart = chart(item, t);
    if chart.is_none() && item.rows.is_empty() {
        return None;
    }
    let mut body = div().flex().flex_col();
    if let Some(chart) = chart {
        body = body.child(padded(chart));
    }
    if !item.rows.is_empty() {
        let table = div().children(item.rows.iter().enumerate().map(|(i, r)| data_row(i, r, t)));
        body = body.child(div().border_t_1().border_color(t.line2).child(table));
    }
    Some(kit::card("The data", None, None, body, t))
}

fn sessions_card(root: &Root, item: &InsightItem, t: &Theme) -> Option<Div> {
    if item.sessions.is_empty() {
        return None;
    }
    let view = root.list.as_ref().ok()?;
    let rows: Vec<_> = view
        .rows
        .iter()
        .filter(|r| item.sessions.contains(&r.activity_id))
        .collect();
    if rows.is_empty() {
        return None;
    }
    let meta: SharedString = format!("{} counted", rows.len()).into();
    let table = blocks::activities::table(&rows, t);
    Some(kit::card("Sessions", Some(meta), None, table, t))
}

/// The cards, with the width of each in columns.
fn cards(root: &Root, item: &InsightItem, t: &Theme) -> Vec<(u8, Div)> {
    let mut out = vec![(4, found_card(item, t))];
    out.extend(data_card(item, t).map(|c| (8, c)));
    out.extend(action_card(item, t).map(|c| (4, c)));
    out.extend(sessions_card(root, item, t).map(|c| (8, c)));
    out
}

/// What the coach hears when the user asks about an item.
fn question(item: &InsightItem) -> String {
    format!(
        "Tell me more about this insight ({}): {} What does it mean for me, and what should I do?",
        item.id, item.claim
    )
}

fn lede(section: &InsightSection, item: &InsightItem, today: NaiveDate) -> String {
    let mut parts = vec![section.title.clone(), tone_words(item.tone).to_string()];
    if let Some(age) = item.first_seen.as_deref().and_then(|s| age(s, today)) {
        parts.push(format!("first seen {age}"));
    }
    parts.join(" · ")
}

/// The page of one item. It says so when the item is gone, for example after a sync.
pub fn page(
    root: &Root,
    id: &str,
    available: f32,
    narrow: bool,
    t: &Theme,
    cx: &mut Context<Root>,
) -> AnyElement {
    let Some((section, item)) = find(root.training.as_ref(), id) else {
        return padded("This insight is not in the data any more.")
            .text_color(t.mutd)
            .into_any_element();
    };
    let ask = question(item);
    let button = kit::button(
        "insight-ask",
        "Ask the coach about this",
        Look::Primary,
        true,
        t,
    )
    .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.ask_coach(ask.clone(), cx)));
    let heading = page_heading(
        item.claim.clone(),
        Some(lede(section, item, root.focus.end)),
        t,
    );
    div()
        .child(heading)
        .child(div().flex().mb(px(16.0)).child(button))
        .child(blocks::lay_out(cards(root, item, t), available, narrow))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_claim_is_cut_at_a_word() {
        let claim =
            "At the same heart rate you now run about 40 seconds per kilometre faster than before.";
        let s = short(claim);
        assert!(s.ends_with("..."));
        assert!(s.chars().count() <= CRUMB_MAX + 3);
        assert_eq!(short("A short claim."), "A short claim");
    }

    #[test]
    fn a_pace_reads_as_minutes_and_seconds() {
        assert_eq!(pace(5.5), "5:30");
        assert_eq!(format_value(Some(Unit::Pct), 61.4), "61%");
    }

    #[test]
    fn a_date_label_is_short_and_other_labels_stay() {
        assert_eq!(short_label("2026-03-04"), "4 Mar");
        assert_eq!(short_label("Mon"), "Mon");
    }
}
