//! The blocks of the Activities screen, and the table that the detail page
//! and the page of all sessions share. The detail page of one session is in
//! `crate::activities`.

use crate::scale::px;
use chrono::{Datelike, Duration, NaiveDate};
use garmin_api::ActivityRow;
use gpui::{div, prelude::*, Div, SharedString};

use super::{Block, BlockDef, Ctx, Go, Width};
use crate::charts::{self, Bar, MeterRow, Tone};
use crate::focus::Focus;
use crate::icons::Icon;
use crate::kit::{self, Cell, Column, ListRow};
use crate::screen::Screen;
use crate::theme::Theme;

const DASH: &str = "—";
/// The rows of the table on the screen when the focus is one day.
const RECENT_ROWS: usize = 10;
/// The most rows when the focus is a range.
const RANGE_ROWS: usize = 50;

pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "activities_recent",
        title: "Recent activities",
        group: Screen::Activities,
        default_width: Width::Full,
        render: recent,
    },
    BlockDef {
        id: "activities_distance",
        title: "Distance by month",
        group: Screen::Activities,
        default_width: Width::TwoThirds,
        render: distance,
    },
    BlockDef {
        id: "activities_records",
        title: "Personal records",
        group: Screen::Activities,
        default_width: Width::Third,
        render: records,
    },
    BlockDef {
        id: "activities_sport",
        title: "Time by sport",
        group: Screen::Activities,
        default_width: Width::Third,
        render: sport,
    },
    BlockDef {
        id: "activities_weeks",
        title: "Weekly totals",
        group: Screen::Activities,
        default_width: Width::TwoThirds,
        render: weeks,
    },
    BlockDef {
        id: "activities_month",
        title: "This month and last month",
        group: Screen::Activities,
        default_width: Width::Third,
        render: month,
    },
    BlockDef {
        id: "activities_best",
        title: "Best sessions",
        group: Screen::Activities,
        default_width: Width::Third,
        render: bests,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("activities_recent", Width::Full),
    ("activities_distance", Width::TwoThirds),
    ("activities_records", Width::Third),
    ("activities_weeks", Width::TwoThirds),
    ("activities_sport", Width::Third),
    ("activities_month", Width::Third),
    ("activities_best", Width::Third),
];

pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

/// The count of sessions, and the distance of the last seven days or of the range.
pub fn lede(ctx: &Ctx) -> Option<String> {
    let view = ctx.activities.as_ref().ok().filter(|v| v.total > 0)?;
    let since = view
        .since_year
        .map_or(String::new(), |y| format!(" since {y}"));
    let mut text = format!("{} activities{since}.", group(view.total));
    let (from, to) = if ctx.focus.days() > 1 {
        (ctx.focus.start, ctx.focus.end)
    } else {
        (ctx.focus.end - Duration::days(6), ctx.focus.end)
    };
    let km = view
        .rows
        .iter()
        .filter(|r| parse_day(&r.date).is_some_and(|d| d >= from && d <= to))
        .filter_map(|r| r.distance_m)
        .sum::<f64>()
        / 1000.0;
    if km >= 0.05 {
        let when = if ctx.focus.days() > 1 {
            "in the range"
        } else {
            "in the last seven days"
        };
        text.push_str(&format!(" {km:.1} km {when}."));
    }
    Some(text)
}

pub fn crumb(ctx: &Ctx) -> Option<String> {
    Some(if ctx.focus.days() > 1 {
        ctx.focus.label()
    } else {
        "All sports".into()
    })
}

/* ------------------------------------------------------------------ shared --- */

pub(super) fn block(
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

pub(super) fn padded(body: impl IntoElement) -> Div {
    div().px(px(12.0)).py(px(12.0)).child(body)
}

pub(super) fn parse_day(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

/// The text for the meta of a block: the range when the focus is a range.
pub(super) fn meta_for(focus: &Focus, one_day: &str) -> String {
    if focus.days() > 1 {
        focus.label()
    } else {
        one_day.to_string()
    }
}

/// `1,284` from 1284.
pub(super) fn group(n: i64) -> String {
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// `5:18` under an hour, `1:02:10` from one hour.
pub(super) fn clock(secs: f64) -> String {
    let s = secs.round() as i64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// `18:40` from seconds: hours and minutes.
fn hours_minutes(secs: f64) -> String {
    let m = (secs / 60.0).round() as i64;
    format!("{}:{:02}", m / 60, m % 60)
}

/// The first letter of the month in `YYYY-MM`.
fn month_initial(month: &str) -> String {
    NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d")
        .map(|d| d.format("%b").to_string().chars().take(1).collect())
        .unwrap_or_default()
}

fn mean(values: impl Iterator<Item = f32>) -> Option<f32> {
    let (sum, n) = values.fold((0.0, 0), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f32)
}

/* ----------------------------------------------------------------- recent --- */

/// The rows in the focus. A day shows the newest rows up to that day. A range
/// shows the rows inside it.
fn in_focus<'a>(rows: &'a [ActivityRow], focus: &Focus) -> Vec<&'a ActivityRow> {
    let ranged = focus.days() > 1;
    let kept = rows.iter().filter(|r| {
        parse_day(&r.date).is_some_and(|d| {
            if ranged {
                focus.contains(d)
            } else {
                d <= focus.end
            }
        })
    });
    kept.take(if ranged { RANGE_ROWS } else { RECENT_ROWS })
        .collect()
}

/// The title of the rate column: pace on foot, speed on wheels, both, or none.
fn rate_title(rows: &[&ActivityRow]) -> Option<&'static str> {
    let pace = rows.iter().any(|r| r.pace.is_some());
    let speed = rows.iter().any(|r| r.speed_kmh.is_some());
    match (pace, speed) {
        (true, true) => Some("Pace / speed"),
        (true, false) => Some("Pace"),
        (false, true) => Some("Speed"),
        (false, false) => None,
    }
}

/// The pace of a session on foot, the speed of a session on wheels.
fn rate_text(r: &ActivityRow) -> Option<String> {
    r.pace
        .clone()
        .or_else(|| r.speed_kmh.map(|s| format!("{s:.1} km/h")))
}

/// The table of sessions. A click on a row opens the session.
pub fn table(rows: &[&ActivityRow], t: &Theme) -> Div {
    let numeric = |title: &str| Column {
        title: title.to_string().into(),
        numeric: true,
    };
    let rate = rate_title(rows);
    let has_distance = rows.iter().any(|r| r.distance_m.is_some());
    let mut columns = vec![
        Column {
            title: "Date".into(),
            numeric: false,
        },
        Column {
            title: "Activity".into(),
            numeric: false,
        },
    ];
    if has_distance {
        columns.push(numeric("Distance"));
    }
    columns.push(numeric("Time"));
    columns.extend(rate.map(numeric));
    columns.extend([numeric("Avg HR"), numeric("Load")]);
    let dash =
        |text: Option<String>| -> SharedString { text.unwrap_or_else(|| DASH.into()).into() };
    let lines = rows
        .iter()
        .map(|r| {
            let date = parse_day(&r.date).map(|d| d.format("%a %-d %b").to_string());
            let mut cells = vec![dash(date), r.name.clone().into()];
            if has_distance {
                cells.push(dash(r.distance_m.map(|m| format!("{:.2} km", m / 1000.0))));
            }
            cells.push(dash(r.duration_s.map(clock)));
            if rate.is_some() {
                cells.push(dash(rate_text(r)));
            }
            cells.extend([
                dash(r.avg_hr.map(|h| format!("{h:.0}"))),
                dash(r.load.map(|l| format!("{l:.0}"))),
            ]);
            (cells, Some(super::open_activity(r.activity_id)))
        })
        .collect();
    kit::table_rows(&columns, lines, t)
}

fn recent(ctx: &Ctx) -> Option<Block> {
    let view = ctx.activities.as_ref().ok()?;
    let rows = in_focus(&view.rows, &ctx.focus);
    if rows.is_empty() {
        return None;
    }
    let meta = if ctx.focus.days() > 1 {
        format!("{} in {}", group(rows.len() as i64), ctx.focus.label())
    } else {
        format!("{} in total", group(view.total))
    };
    block(
        Some(meta),
        Some(("All", Go::AllActivities)),
        table(&rows, ctx.t),
    )
}

/* ---------------------------------------------------------------- distance --- */

/// True when the month `YYYY-MM` has a day in the focus range.
fn month_in_range(month: &str, focus: &Focus) -> bool {
    let Ok(first) = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d") else {
        return false;
    };
    let last = NaiveDate::from_ymd_opt(
        first.year() + i32::from(first.month() == 12),
        first.month() % 12 + 1,
        1,
    )
    .map_or(first, |d| d - Duration::days(1));
    first <= focus.end && last >= focus.start
}

fn distance(ctx: &Ctx) -> Option<Block> {
    let months: Vec<_> = ctx
        .charts_view?
        .distance_by_month
        .iter()
        .filter(|m| ctx.focus.days() <= 1 || month_in_range(&m.month, &ctx.focus))
        .collect();
    if months.iter().all(|m| m.total_km <= 0.0) {
        return None;
    }
    let bars: Vec<Bar> = months
        .iter()
        .map(|m| Bar {
            value: m.total_km as f32,
            label: if m.total_km >= 0.5 {
                format!("{:.0}", m.total_km).into()
            } else {
                "".into()
            },
            day: month_initial(&m.month).into(),
        })
        .collect();
    let average = mean(bars.iter().map(|b| b.value))
        .filter(|a| *a > 0.0)
        .map(|a| (a, format!("avg {a:.0}")));
    let meta = if ctx.focus.days() > 1 {
        format!("{} · km", ctx.focus.label())
    } else {
        "last 12 months · km".into()
    };
    block(Some(meta), None, charts::bar_chart(&bars, average, ctx.t))
}

/* ----------------------------------------------------------------- records --- */

/// Metres of the distance in the label of a record, such as `Fastest 5 km`.
fn record_km(label: &str) -> Option<f64> {
    Some(match label.strip_prefix("Fastest ")? {
        "1 km" => 1.0,
        "1 mile" => 1.609_344,
        "5 km" => 5.0,
        "10 km" => 10.0,
        "half marathon" => 21.097_5,
        "marathon" => 42.195,
        _ => return None,
    })
}

fn parse_clock(text: &str) -> Option<f64> {
    text.split(':').try_fold(0.0, |acc, part| {
        Some(acc * 60.0 + part.parse::<f64>().ok()?)
    })
}

/// The pace of a record for a fixed distance, such as `4:22 /km`.
fn record_pace(label: &str, value: &str) -> Option<String> {
    let secs = parse_clock(value)? / record_km(label)?;
    let total = secs.round() as i64;
    Some(format!("{}:{:02} /km", total / 60, total % 60))
}

/// `5 km` from `Fastest 5 km`. Other labels stay as they are.
fn record_title(label: &str) -> String {
    let Some(rest) = label.strip_prefix("Fastest ") else {
        return label.to_string();
    };
    let mut chars = rest.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn records(ctx: &Ctx) -> Option<Block> {
    let list = &ctx.training?.fitness.records;
    if list.is_empty() {
        return None;
    }
    let rows = list
        .iter()
        .take(6)
        .map(|r| ListRow {
            glyph: Icon::Trophy,
            tone: Tone::Accent,
            title: record_title(&r.label).into(),
            sub: (!r.date.is_empty()).then(|| r.date.clone().into()),
            right: Some(r.value.clone().into()),
            right_sub: record_pace(&r.label, &r.value).map(Into::into),
        })
        .collect();
    block(None, None, kit::list_rows(rows, ctx.t))
}

/* ------------------------------------------------------------------- sport --- */

/// The seconds and share of each sport in a range, longest first.
fn sport_in_range(rows: &[ActivityRow], focus: &Focus) -> Vec<(String, f64, f64)> {
    let mut sums: Vec<(String, f64)> = Vec::new();
    for r in rows {
        let (Some(day), Some(secs)) = (parse_day(&r.date), r.duration_s) else {
            continue;
        };
        if !focus.contains(day) || secs <= 0.0 {
            continue;
        }
        match sums.iter_mut().find(|(label, _)| *label == r.sport_label) {
            Some(entry) => entry.1 += secs,
            None => sums.push((r.sport_label.clone(), secs)),
        }
    }
    sums.sort_by(|a, b| b.1.total_cmp(&a.1));
    let total: f64 = sums.iter().map(|s| s.1).sum();
    sums.into_iter()
        .map(|(label, secs)| (label, secs, secs / total * 100.0))
        .collect()
}

fn sport(ctx: &Ctx) -> Option<Block> {
    let list: Vec<(String, f64, f64)> = if ctx.focus.days() > 1 {
        let view = ctx.activities.as_ref().ok()?;
        sport_in_range(&view.rows, &ctx.focus)
    } else {
        let all = &ctx.charts_view?.time_by_sport;
        all.iter()
            .map(|s| (s.label.clone(), s.secs, s.share_pct))
            .collect()
    };
    let longest = list.iter().map(|s| s.1).fold(0.0, f64::max);
    if longest <= 0.0 {
        return None;
    }
    let rows = list
        .iter()
        .enumerate()
        .map(|(i, (label, secs, share))| MeterRow {
            name: label.clone().into(),
            fraction: (secs / longest) as f32,
            value: hours_minutes(*secs).into(),
            share: format!("{share:.0}%").into(),
            tone: if i == 0 { Tone::Accent } else { Tone::Muted },
        })
        .collect();
    block(
        Some(meta_for(&ctx.focus, "last 30 days")),
        None,
        padded(charts::meter_rows(rows, ctx.t)),
    )
}

/* ------------------------------------------------------------------- weeks --- */

pub(super) fn monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(i64::from(d.weekday().num_days_from_monday()))
}

/// The Monday of each of the last `count` weeks, oldest first. A range starts
/// at the week of its first day.
fn week_starts(focus: &Focus, count: i64) -> Vec<NaiveDate> {
    let last = monday(focus.end);
    let first = if focus.days() > 1 {
        monday(focus.start).max(last - Duration::weeks(count - 1))
    } else {
        last - Duration::weeks(count - 1)
    };
    let mut out = Vec::new();
    let mut day = first;
    while day <= last {
        out.push(day);
        day += Duration::weeks(1);
    }
    out
}

pub(super) fn week_rows(
    rows: &[ActivityRow],
    start: NaiveDate,
) -> impl Iterator<Item = &ActivityRow> {
    rows.iter().filter(move |r| {
        parse_day(&r.date).is_some_and(|d| d >= start && d < start + Duration::days(7))
    })
}

fn weeks(ctx: &Ctx) -> Option<Block> {
    let view = ctx.activities.as_ref().ok()?;
    let columns = [
        Column {
            title: "Week of".into(),
            numeric: false,
        },
        Column {
            title: "Sessions".into(),
            numeric: true,
        },
        Column {
            title: "Distance".into(),
            numeric: true,
        },
        Column {
            title: "Time".into(),
            numeric: true,
        },
        Column {
            title: "Load".into(),
            numeric: true,
        },
    ];
    let mut lines = Vec::new();
    for start in week_starts(&ctx.focus, 6).into_iter().rev() {
        let list: Vec<_> = week_rows(&view.rows, start).collect();
        let sum =
            |f: fn(&ActivityRow) -> Option<f64>| list.iter().filter_map(|r| f(r)).sum::<f64>();
        let (km, secs, load) = (
            sum(|r| r.distance_m) / 1000.0,
            sum(|r| r.duration_s),
            sum(|r| r.load),
        );
        let cell = |on: bool, text: String| -> SharedString {
            if on { text } else { DASH.to_string() }.into()
        };
        lines.push(vec![
            start.format("%-d %b").to_string().into(),
            list.len().to_string().into(),
            cell(km > 0.0, format!("{km:.1} km")),
            cell(secs > 0.0, hours_minutes(secs)),
            cell(load > 0.0, format!("{load:.0}")),
        ]);
    }
    if lines.iter().all(|l| l[1].as_ref() == "0") {
        return None;
    }
    block(
        Some(meta_for(&ctx.focus, "last 6 weeks")),
        None,
        kit::table(&columns, lines, ctx.t),
    )
}

/* ------------------------------------------------------------------- month --- */

fn month(ctx: &Ctx) -> Option<Block> {
    let c = &ctx.charts_view?.month_compare;
    if c.this_month.sessions == 0 && c.last_month.sessions == 0 {
        return None;
    }
    let cell = |label: &str, now: String, before: String| Cell {
        label: label.to_string().into(),
        value: now.into(),
        unit: None,
        sub: Some((format!("was {before}").into(), Tone::Muted)),
        spark: None,
        big: false,
    };
    let (a, b) = (&c.this_month, &c.last_month);
    let mut cells = vec![cell(
        "Sessions",
        a.sessions.to_string(),
        b.sessions.to_string(),
    )];
    if a.distance_km.abs() >= 0.05 || b.distance_km.abs() >= 0.05 {
        cells.push(cell(
            "Distance",
            format!("{:.1} km", a.distance_km.abs()),
            format!("{:.1} km", b.distance_km.abs()),
        ));
    }
    cells.push(cell("Time", hours_minutes(a.secs), hours_minutes(b.secs)));
    if let (Some(x), Some(y)) = (a.easy_avg_hr, b.easy_avg_hr) {
        cells.push(cell(
            "Easy heart rate",
            format!("{x:.0} bpm"),
            format!("{y:.0} bpm"),
        ));
    }
    block(
        Some(format!("first {} days", c.through_day)),
        None,
        kit::stat_grid(cells, 2, ctx.t),
    )
}

/* -------------------------------------------------------------------- best --- */

/// The row with the largest value of `f`.
fn best<'a>(
    rows: &[&'a ActivityRow],
    f: impl Fn(&ActivityRow) -> Option<f64>,
) -> Option<&'a ActivityRow> {
    rows.iter()
        .copied()
        .filter(|r| f(r).is_some())
        .max_by(|a, b| f(a).unwrap_or(0.0).total_cmp(&f(b).unwrap_or(0.0)))
}

fn best_cell(label: &str, row: &ActivityRow, value: String) -> Cell {
    let day = parse_day(&row.date).map(|d| d.format("%-d %b").to_string());
    let sub = match day {
        Some(d) => format!("{} · {d}", row.name),
        None => row.name.clone(),
    };
    Cell {
        label: label.to_string().into(),
        value: value.into(),
        unit: None,
        sub: Some((sub.into(), Tone::Muted)),
        spark: None,
        big: false,
    }
}

/// The pace in seconds per kilometre of a row on foot with a real distance.
fn pace_secs(r: &ActivityRow) -> Option<f64> {
    r.pace
        .as_deref()
        .and_then(parse_clock)
        .filter(|_| r.distance_m.is_some_and(|d| d >= 1000.0))
}

/// The fastest session of the sport that the rows hold most: pace on foot, speed on wheels.
/// Other sports have neither, so a gym or golf month shows no such cell.
fn fastest_cell(rows: &[&ActivityRow]) -> Option<Cell> {
    let foot = rows.iter().filter(|r| pace_secs(r).is_some()).count();
    let wheels = rows
        .iter()
        .filter(|r| r.speed_kmh.is_some() && r.distance_m.is_some_and(|d| d >= 1000.0))
        .count();
    if foot == 0 && wheels == 0 {
        return None;
    }
    if foot >= wheels {
        let r = best(rows, |r| pace_secs(r).map(|p| -p))?;
        return Some(best_cell(
            "Fastest pace",
            r,
            format!("{} /km", r.pace.clone()?),
        ));
    }
    let r = best(rows, |r| {
        r.distance_m.filter(|d| *d >= 1000.0).and(r.speed_kmh)
    })?;
    Some(best_cell(
        "Fastest speed",
        r,
        format!("{:.1} km/h", r.speed_kmh?),
    ))
}

fn bests(ctx: &Ctx) -> Option<Block> {
    let view = ctx.activities.as_ref().ok()?;
    let (from, to) = if ctx.focus.days() > 1 {
        (ctx.focus.start, ctx.focus.end)
    } else {
        (ctx.focus.end.with_day(1)?, ctx.focus.end)
    };
    let rows: Vec<&ActivityRow> = view
        .rows
        .iter()
        .filter(|r| parse_day(&r.date).is_some_and(|d| d >= from && d <= to))
        .collect();
    let mut cells = Vec::new();
    if let Some(r) = best(&rows, |r| r.distance_m) {
        cells.push(best_cell(
            "Longest",
            r,
            format!("{:.1} km", r.distance_m? / 1000.0),
        ));
    }
    if let Some(r) = best(&rows, |r| r.duration_s) {
        cells.push(best_cell("Most time", r, clock(r.duration_s?)));
    }
    cells.extend(fastest_cell(&rows));
    if let Some(r) = best(&rows, |r| r.load) {
        cells.push(best_cell("Highest load", r, format!("{:.0}", r.load?)));
    }
    if cells.is_empty() {
        return None;
    }
    let meta = if ctx.focus.days() > 1 {
        ctx.focus.label()
    } else {
        "this month".into()
    };
    block(Some(meta), None, kit::stat_grid(cells, 2, ctx.t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pace: Option<&str>, speed: Option<f64>) -> ActivityRow {
        ActivityRow {
            activity_id: 1,
            date: "2026-03-10".into(),
            name: "Session".into(),
            sport_label: "Sport".into(),
            distance_m: Some(10_000.0),
            duration_s: Some(3_600.0),
            pace: pace.map(Into::into),
            speed_kmh: speed,
            avg_hr: None,
            load: None,
        }
    }

    #[test]
    fn the_fastest_cell_follows_the_sport_of_the_rows() {
        let run = row(Some("5:00"), None);
        let ride = row(None, Some(30.0));
        let gym = row(None, None);
        let label = |rows: &[&ActivityRow]| fastest_cell(rows).map(|c| c.label.to_string());
        assert_eq!(label(&[&run, &gym]).as_deref(), Some("Fastest pace"));
        assert_eq!(label(&[&ride, &gym]).as_deref(), Some("Fastest speed"));
        assert_eq!(label(&[&gym]), None);
        assert_eq!(rate_title(&[&gym]), None);
        assert_eq!(rate_title(&[&run, &ride]), Some("Pace / speed"));
    }

    #[test]
    fn a_number_gets_a_comma_between_thousands() {
        assert_eq!(group(1284), "1,284");
        assert_eq!(group(95), "95");
        assert_eq!(group(1_000_000), "1,000,000");
    }

    #[test]
    fn a_record_has_a_pace_when_it_has_a_distance() {
        assert_eq!(
            record_pace("Fastest 5 km", "21:48").as_deref(),
            Some("4:22 /km")
        );
        assert_eq!(
            record_pace("Fastest half marathon", "1:39:12").as_deref(),
            Some("4:42 /km")
        );
        assert_eq!(record_pace("Longest run", "32.1 km"), None);
        assert_eq!(record_title("Fastest 5 km"), "5 km");
        assert_eq!(record_title("Fastest half marathon"), "Half marathon");
        assert_eq!(record_title("Longest run"), "Longest run");
    }

    #[test]
    fn a_month_is_in_the_range_when_one_day_is() {
        let day = |m, d| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        let range = Focus::range(day(9, 28), day(10, 4));
        assert!(month_in_range("2026-09", &range));
        assert!(month_in_range("2026-10", &range));
        assert!(!month_in_range("2026-08", &range));
        assert!(!month_in_range("2026-11", &range));
        assert!(month_in_range("2026-12", &Focus::day(day(12, 31))));
    }

    #[test]
    fn times_read_as_clock_and_hours() {
        assert_eq!(clock(2650.0), "44:10");
        assert_eq!(clock(7928.0), "2:12:08");
        assert_eq!(hours_minutes(18.0 * 3600.0 + 40.0 * 60.0), "18:40");
        assert_eq!(month_initial("2026-09"), "S");
    }
}
