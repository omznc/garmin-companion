//! The blocks of the Weight screen. Each one hides when its data is missing.

use chrono::{Duration, NaiveDate};
use garmin_api::body::{WeightPointView, WeightView};

use super::daily::{block, cell, change_words, date_labels, key_figures, line_chart, Line};
use super::{Block, BlockDef, Ctx, Width};
use crate::charts::Tone;
use crate::icons::Icon;
use crate::kit::{self, Column, ListRow};
use crate::screen::Screen;

pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "weight_line",
        title: "Weight",
        group: Screen::Weight,
        default_width: Width::TwoThirds,
        render: line,
    },
    BlockDef {
        id: "weight_body",
        title: "Body composition",
        group: Screen::Weight,
        default_width: Width::Third,
        render: body,
    },
    BlockDef {
        id: "weight_windows",
        title: "Change by period",
        group: Screen::Weight,
        default_width: Width::Third,
        render: windows,
    },
    BlockDef {
        id: "weight_empty",
        title: "No weight",
        group: Screen::Weight,
        default_width: Width::Full,
        render: empty,
    },
    BlockDef {
        id: "weight_weighins",
        title: "Weigh-ins",
        group: Screen::Weight,
        default_width: Width::TwoThirds,
        render: weighins,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("weight_empty", Width::Full),
    ("weight_line", Width::TwoThirds),
    ("weight_body", Width::Third),
    ("weight_weighins", Width::TwoThirds),
    ("weight_windows", Width::Third),
];

/// Why the screen is empty, when the core says so.
fn empty(ctx: &Ctx) -> Option<Block> {
    super::daily::empty_note(ctx.body.0.weight.empty.as_ref()?, ctx.t)
}

pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

/// The weight sentence of the core.
pub fn lede(ctx: &Ctx) -> Option<String> {
    view(ctx)?.sentence.clone()
}

pub fn crumb(_ctx: &Ctx) -> Option<String> {
    None
}

/// The weight view, when it holds weigh-ins.
fn view<'a>(ctx: &Ctx<'a>) -> Option<&'a WeightView> {
    let v = &ctx.body.0.weight;
    (v.empty.is_none() && !v.points.is_empty()).then_some(v)
}

fn date_of(p: &WeightPointView) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&p.date, "%Y-%m-%d").ok()
}

/// The weigh-ins of the window that ends on the focus, oldest first: 90 days
/// for one day, the range for a longer focus. An outlier does not count.
fn window<'a>(ctx: &Ctx<'a>, v: &'a WeightView) -> Vec<(NaiveDate, &'a WeightPointView)> {
    let from = if ctx.focus.days() > 1 {
        ctx.focus.start
    } else {
        ctx.focus.end - Duration::days(89)
    };
    v.points
        .iter()
        .filter(|p| !p.outlier)
        .filter_map(|p| Some((date_of(p)?, p)))
        .filter(|(d, _)| *d >= from && *d <= ctx.focus.end)
        .collect()
}

fn line(ctx: &Ctx) -> Option<Block> {
    let v = view(ctx)?;
    let points = window(ctx, v);
    if points.len() < 2 {
        return None;
    }
    let (first, last) = (points.first()?, points.last()?);
    let span = ((last.0 - first.0).num_days().max(1)) as f32;
    let days = if ctx.focus.days() > 1 {
        ctx.focus.days()
    } else {
        90
    };
    let ago = (ctx.focus.end - last.0).num_days();
    let mut words = change_words(first.1.kg as f32, last.1.kg as f32, days as usize, 1);
    if ago > 0 {
        words.push_str(&format!(" · last logged {ago} days ago"));
    }
    let series: Vec<(NaiveDate, Option<f64>)> = points.iter().map(|(d, _)| (*d, None)).collect();
    let chart = Line {
        pts: points
            .iter()
            .map(|(d, p)| ((*d - first.0).num_days() as f32 / span, p.kg as f32))
            .collect(),
        band: None,
        dots: true,
        height: 130.0,
        decimals: 1,
        head: Some((format!("{:.1}", last.1.kg), "kg".into(), words)),
        labels: date_labels(&series),
    };
    let meta = if ctx.focus.days() > 1 {
        ctx.focus.label()
    } else {
        "90 days · kg".to_string()
    };
    block(Some(meta), None, line_chart(chart, ctx.t))
}

fn body(ctx: &Ctx) -> Option<Block> {
    let v = view(ctx)?;
    let c = v.composition.as_ref();
    let one = |label: &str, value: Option<f64>, unit: &str| {
        value.map(|v| cell(label, format!("{v:.1}"), Some(unit)))
    };
    let cells = [
        one("Weight", v.latest_kg, "kg"),
        one("Body fat", c.and_then(|c| c.body_fat_pct), "%"),
        one("Muscle", c.and_then(|c| c.muscle_kg), "kg"),
        one("BMI", v.bmi, ""),
        one("Body water", c.and_then(|c| c.body_water_pct), "%"),
        one("Bone", c.and_then(|c| c.bone_kg), "kg"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let meta = c
        .and_then(|c| NaiveDate::parse_from_str(&c.date, "%Y-%m-%d").ok())
        .map_or("latest".to_string(), |d| d.format("%-d %b").to_string());
    key_figures(cells, &meta, ctx.t)
}

fn windows(ctx: &Ctx) -> Option<Block> {
    let v = view(ctx)?;
    if v.windows.is_empty() {
        return None;
    }
    let rows = v
        .windows
        .iter()
        .map(|w| ListRow {
            glyph: Icon::Info,
            tone: Tone::Accent,
            title: w.sentence.clone().into(),
            sub: Some(format!("{} days", w.days).into()),
            right: None,
            right_sub: None,
        })
        .collect();
    block(None, None, kit::list_rows(rows, ctx.t))
}

fn weighins(ctx: &Ctx) -> Option<Block> {
    let v = view(ctx)?;
    let mut rows: Vec<Vec<gpui::SharedString>> = v
        .points
        .iter()
        .filter_map(|p| Some((date_of(p)?, p)))
        .filter(|(d, _)| *d <= ctx.focus.end && (ctx.focus.days() <= 1 || *d >= ctx.focus.start))
        .map(|(d, p)| {
            vec![
                d.format("%a %-d %b").to_string().into(),
                format!("{:.1}", p.kg).into(),
                p.trend_kg
                    .map_or("-".to_string(), |t| format!("{t:.1}"))
                    .into(),
                if p.outlier { "check this" } else { "" }.into(),
            ]
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    rows.reverse();
    rows.truncate(60);
    let columns = [
        ("Date", false),
        ("Weight (kg)", true),
        ("Trend (kg)", true),
        ("Note", false),
    ]
    .map(|(title, numeric)| Column {
        title: title.into(),
        numeric,
    });
    block(
        Some(format!("{} in total", v.count)),
        None,
        kit::table(&columns, rows, ctx.t),
    )
}
