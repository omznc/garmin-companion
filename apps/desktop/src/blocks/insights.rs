//! The blocks of the Insights screen: the findings of the coach, and the
//! month so far against the month before.

use chrono::{Datelike, NaiveDate};
use garmin_api::charts::{MonthCompare, MonthStats};
use garmin_api::insights::InsightItem;
use garmin_core::findings::Tone as FindingTone;
use gpui::{div, prelude::*, Div};

use super::{Block, BlockDef, Ctx, Width};
use crate::charts::Tone;
use crate::icons::Icon;
use crate::insight::open_insight;
use crate::kit::{self, Cell, ListRow};
use crate::screen::Screen;

pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "insights_list",
        title: "Insights",
        group: Screen::Insights,
        default_width: Width::TwoThirds,
        render: list,
    },
    BlockDef {
        id: "insights_month",
        title: "Month so far",
        group: Screen::Insights,
        default_width: Width::Third,
        render: month,
    },
];

pub const LAYOUT: &[(&str, Width)] = &[
    ("insights_list", Width::TwoThirds),
    ("insights_month", Width::Third),
];

pub fn title(_ctx: &Ctx) -> Option<String> {
    Some("Insights".into())
}

pub fn lede(_ctx: &Ctx) -> Option<String> {
    Some("Patterns that the coach found in your data.".into())
}

/// Insights keep the text of the screen.
pub fn crumb(_ctx: &Ctx) -> Option<String> {
    None
}

/// Findings that first showed in the last seven days are new.
const NEW_DAYS: i64 = 7;

/// How long ago a finding first showed, in words.
pub(crate) fn age(first_seen: &str, today: NaiveDate) -> Option<String> {
    let seen = NaiveDate::parse_from_str(first_seen, "%Y-%m-%d").ok()?;
    let days = (today - seen).num_days();
    Some(match days {
        ..=0 => "today".into(),
        1 => "yesterday".into(),
        2..=6 => format!("{days} days ago"),
        7..=13 => "last week".into(),
        _ => format!("{} weeks ago", days / 7),
    })
}

fn is_new(item: &InsightItem, today: NaiveDate) -> bool {
    item.first_seen
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .is_some_and(|seen| (today - seen).num_days() < NEW_DAYS)
}

fn list(ctx: &Ctx) -> Option<Block> {
    let view = &ctx.training?.insights;
    let today = ctx.focus.end;
    let mut rows = Vec::new();
    let mut fresh = 0;
    for section in &view.sections {
        for item in &section.items {
            fresh += usize::from(is_new(item, today));
            let sub = match item.first_seen.as_deref().and_then(|s| age(s, today)) {
                Some(age) => format!("{} · {age}", section.title),
                None => section.title.clone(),
            };
            let tone = if item.tone == FindingTone::Watch {
                Tone::Warn
            } else {
                Tone::Accent
            };
            let row = ListRow {
                glyph: Icon::NavInsights,
                tone,
                title: item.claim.clone().into(),
                sub: Some(sub.into()),
                right: None,
                right_sub: None,
            };
            rows.push((row, Some(open_insight(item.id.clone()))));
        }
    }
    if rows.is_empty() {
        return None;
    }
    Some(Block {
        meta: (fresh > 0).then(|| format!("{fresh} new").into()),
        link: None,
        body: kit::list_rows_click(rows, ctx.t).into_any_element(),
    })
}

/// "1:14 h" style time: hours and minutes of `secs`.
fn hours(secs: f64) -> String {
    let m = (secs / 60.0).round() as i64;
    format!("{}:{:02}", m / 60, m % 60)
}

/// A signed difference. The tone is accent for a change in the good direction.
fn delta(diff: f64, unit: &str, decimals: usize, up_is_good: bool) -> Option<(String, Tone)> {
    let shown = format!("{diff:+.decimals$}");
    if shown
        .trim_start_matches(['+', '-'])
        .trim_matches(['0', '.'])
        .is_empty()
    {
        return None;
    }
    let good = (diff > 0.0) == up_is_good;
    let tone = if good { Tone::Accent } else { Tone::Muted };
    Some((format!("{} {unit}", shown.replace('-', "\u{2212}")), tone))
}

fn cells(m: &MonthCompare) -> Vec<Cell> {
    let (a, b): (&MonthStats, &MonthStats) = (&m.this_month, &m.last_month);
    let cell = |label: &str, value: String, unit: Option<&str>, sub: Option<(String, Tone)>| Cell {
        label: label.to_string().into(),
        value: value.into(),
        unit: unit.map(|u| u.to_string().into()),
        sub: sub.map(|(s, t)| (s.into(), t)),
        spark: None,
        big: false,
    };
    let mut out = vec![cell(
        "Sessions",
        a.sessions.to_string(),
        None,
        delta(a.sessions as f64 - b.sessions as f64, "", 0, true),
    )];
    if a.distance_km > 0.0 || b.distance_km > 0.0 {
        out.push(cell(
            "Distance",
            crate::ui::fixed(a.distance_km, 0),
            Some("km"),
            delta(a.distance_km - b.distance_km, "km", 0, true),
        ));
    }
    out.push(cell("Time", hours(a.secs), Some("h"), None));
    if let (Some(x), Some(y)) = (a.easy_avg_hr, b.easy_avg_hr) {
        out.push(cell(
            "Easy HR",
            format!("{x:.0}"),
            Some("bpm"),
            delta(x - y, "bpm", 0, false),
        ));
    }
    out
}

fn month_name(day: NaiveDate, back: u32) -> String {
    let first = day.with_day(1).unwrap_or(day);
    let month = first
        .checked_sub_months(chrono::Months::new(back))
        .unwrap_or(first);
    month.format("%B").to_string()
}

fn month(ctx: &Ctx) -> Option<Block> {
    let m = &ctx.charts_view?.month_compare;
    if m.this_month.sessions == 0 && m.last_month.sessions == 0 {
        return None;
    }
    let cells = cells(m);
    let columns = 2;
    let grid: Div = kit::stat_grid(cells, columns, ctx.t);
    Some(Block {
        meta: Some(format!("vs {}", month_name(ctx.focus.end, 1)).into()),
        link: None,
        body: div().child(grid).into_any_element(),
    })
}
