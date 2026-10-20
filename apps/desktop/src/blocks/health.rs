//! The blocks of the Health screen. Each one hides when its data is missing.

use chrono::NaiveDate;

use super::daily::{
    block, change_words, date_labels, dated, day_points, ends, has_line, line_chart, newest,
    thousands, weekday_initial, window_meta, Line,
};
use super::{Block, BlockDef, Ctx, Width};
use crate::charts::{self, Bar};
use crate::screen::Screen;

pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "health_resting_hr",
        title: "Resting heart rate",
        group: Screen::Health,
        default_width: Width::Half,
        render: resting_hr,
    },
    BlockDef {
        id: "health_hrv",
        title: "HRV status",
        group: Screen::Health,
        default_width: Width::Half,
        render: hrv,
    },
    BlockDef {
        id: "health_battery",
        title: "Body Battery today",
        group: Screen::Health,
        default_width: Width::Half,
        render: battery,
    },
    BlockDef {
        id: "health_stress",
        title: "Stress",
        group: Screen::Health,
        default_width: Width::Half,
        render: stress,
    },
    BlockDef {
        id: "health_steps",
        title: "Steps",
        group: Screen::Health,
        default_width: Width::Half,
        render: steps,
    },
    BlockDef {
        id: "health_spo2",
        title: "Blood oxygen",
        group: Screen::Health,
        default_width: Width::Half,
        render: spo2,
    },
    BlockDef {
        id: "health_respiration",
        title: "Breathing rate",
        group: Screen::Health,
        default_width: Width::Half,
        render: respiration,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("health_resting_hr", Width::Half),
    ("health_hrv", Width::Half),
    ("health_battery", Width::Half),
    ("health_stress", Width::Half),
    ("health_steps", Width::Half),
    ("health_spo2", Width::Half),
    ("health_respiration", Width::Half),
];

pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

/// The change of the resting heart rate, and the HRV status, in two short sentences.
pub fn lede(ctx: &Ctx) -> Option<String> {
    let mut parts = Vec::new();
    if let Some((first, last)) = ends(&series(ctx, "resting_hr")) {
        let words = match last - first {
            d if d <= -1.0 => format!("Your resting heart rate fell by {:.0} bpm.", -d),
            d if d >= 1.0 => format!("Your resting heart rate rose by {d:.0} bpm."),
            _ => "Your resting heart rate is steady.".to_string(),
        };
        parts.push(words);
    }
    if let Some(status) = hrv_status(ctx) {
        parts.push(format!("HRV is {}.", status.to_lowercase()));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

pub fn crumb(_ctx: &Ctx) -> Option<String> {
    None
}

/// The 30 day series of a health metric, clipped to the focus.
fn series(ctx: &Ctx, key: &str) -> Vec<(NaiveDate, Option<f64>)> {
    ctx.wellness
        .and_then(|w| w.health.metrics.iter().find(|m| m.key == key))
        .map(|m| dated(&m.points, ctx.focus))
        .unwrap_or_default()
}

/// The word of the HRV status: "Balanced".
fn hrv_status(ctx: &Ctx) -> Option<String> {
    let tile = ctx.tiles_view?.tiles.iter().find(|t| t.key == "hrv")?;
    let word = tile.status.as_deref()?.replace('_', " ").to_lowercase();
    let mut chars = word.chars();
    let first = chars.next()?.to_uppercase().collect::<String>();
    Some(first + chars.as_str())
}

/// A line block for a daily series: the latest figure, the words, the line.
fn daily_line(
    series: Vec<(NaiveDate, Option<f64>)>,
    unit: &str,
    decimals: usize,
    words: impl FnOnce(usize) -> String,
) -> Option<(Line, usize)> {
    if !has_line(&series) {
        return None;
    }
    let latest = series.iter().rev().find_map(|(_, v)| *v)?;
    let days = series.len();
    Some((
        Line {
            pts: day_points(&series),
            band: None,
            dots: false,
            height: 130.0,
            decimals,
            head: Some((
                format!("{latest:.decimals$}"),
                unit.to_string(),
                words(days),
            )),
            labels: date_labels(&series),
        },
        days,
    ))
}

fn trend_block(ctx: &Ctx, key: &str, unit: &str, decimals: usize) -> Option<Block> {
    let s = series(ctx, key);
    let change = ends(&s);
    let (line, days) = daily_line(s, unit, decimals, |days| {
        change.map_or(String::new(), |(a, b)| change_words(a, b, days, decimals))
    })?;
    block(
        Some(window_meta(ctx.focus, &format!("{days} days · {unit}"))),
        None,
        line_chart(line, ctx.t),
    )
}

fn resting_hr(ctx: &Ctx) -> Option<Block> {
    trend_block(ctx, "resting_hr", "bpm", 0)
}

fn spo2(ctx: &Ctx) -> Option<Block> {
    trend_block(ctx, "spo2", "%", 0)
}

fn respiration(ctx: &Ctx) -> Option<Block> {
    trend_block(ctx, "respiration", "br/min", 0)
}

/* -------------------------------------------------------------------- hrv --- */

/// The overnight readings of the focused night, when the day series is too short.
fn night_line(ctx: &Ctx, band: Option<(f32, f32)>) -> Option<Line> {
    let readings = &ctx.extras_view?.hrv_readings;
    if readings.len() < 2 {
        return None;
    }
    let last = (readings.len() - 1) as f32;
    let clock = |i: usize| readings[i].time.get(11..16).unwrap_or("").to_string();
    Some(Line {
        pts: readings
            .iter()
            .enumerate()
            .map(|(i, r)| (i as f32 / last, r.value as f32))
            .collect(),
        band,
        dots: false,
        height: 130.0,
        decimals: 0,
        head: Some((
            format!("{:.0}", readings[readings.len() - 1].value),
            "ms".into(),
            "last reading".into(),
        )),
        labels: vec![clock(0), clock(readings.len() - 1)],
    })
}

fn hrv(ctx: &Ctx) -> Option<Block> {
    let tile = ctx.tiles_view?.tiles.iter().find(|t| t.key == "hrv");
    let band = tile
        .and_then(|t| t.band_low.zip(t.band_high))
        .map(|(a, b)| (a as f32, b as f32));
    let s = series(ctx, "hrv");
    let words = [
        hrv_status(ctx),
        band.map(|(a, b)| format!("baseline {a:.0}-{b:.0}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let line = if has_line(&s) {
        let latest = s.iter().rev().find_map(|(_, v)| *v)?;
        Line {
            pts: day_points(&s),
            band,
            dots: false,
            height: 130.0,
            decimals: 0,
            head: Some((format!("{latest:.0}"), "ms".into(), words)),
            labels: date_labels(&s),
        }
    } else {
        night_line(ctx, band)?
    };
    block(
        Some(window_meta(ctx.focus, "overnight · ms")),
        None,
        line_chart(line, ctx.t),
    )
}

/* ---------------------------------------------------------------- battery --- */

/// "06:30" from minutes after midnight.
fn clock(minute: u32) -> String {
    format!("{:02}:{:02}", minute / 60 % 24, minute % 60)
}

/// The hours on the axis: every second hour from the first to the last point.
fn hour_labels(first: u32, last: u32) -> Vec<String> {
    let step = if last - first > 16 * 60 { 4 } else { 2 };
    (first / 60..=last / 60)
        .step_by(step)
        .map(|h| format!("{h:02}"))
        .collect()
}

fn battery(ctx: &Ctx) -> Option<Block> {
    let curve = &ctx.extras_view?.body_battery;
    let (first, last) = (curve.first()?, curve.last()?);
    if curve.len() < 2 || last.minute <= first.minute {
        return None;
    }
    let span = (last.minute - first.minute) as f32;
    let peak = curve
        .iter()
        .max_by(|a, b| a.level.total_cmp(&b.level))
        .unwrap_or(last);
    let line = Line {
        pts: curve
            .iter()
            .map(|p| ((p.minute - first.minute) as f32 / span, p.level as f32))
            .collect(),
        band: None,
        dots: false,
        height: 130.0,
        decimals: 0,
        head: Some((
            format!("{:.0}", last.level),
            String::new(),
            format!("highest {:.0} at {}", peak.level, clock(peak.minute)),
        )),
        labels: hour_labels(first.minute, last.minute),
    };
    let meta = if ctx.focus.is_live() {
        "since midnight".to_string()
    } else {
        ctx.focus.end.format("%-d %b").to_string()
    };
    block(Some(meta), None, line_chart(line, ctx.t))
}

/* ----------------------------------------------------------------- stress --- */

/// Bars of a daily series: the value above, the weekday under.
fn day_bars(series: &[(NaiveDate, Option<f64>)]) -> Vec<Bar> {
    series
        .iter()
        .map(|(date, v)| Bar {
            value: v.unwrap_or(0.0) as f32,
            // A long figure, such as steps, shows in thousands to fit the bar.
            label: v
                .map_or(String::new(), |v| {
                    if v >= 1000.0 {
                        format!("{:.0}k", v / 1000.0)
                    } else {
                        thousands(v)
                    }
                })
                .into(),
            day: weekday_initial(*date),
        })
        .collect()
}

fn average(series: &[(NaiveDate, Option<f64>)]) -> Option<f32> {
    let values: Vec<f64> = series.iter().filter_map(|(_, v)| *v).collect();
    (!values.is_empty()).then(|| (values.iter().sum::<f64>() / values.len() as f64) as f32)
}

fn bars_block(ctx: &Ctx, key: &str, count: usize, default_meta: &str, word: &str) -> Option<Block> {
    let s = if ctx.focus.days() > 1 {
        series(ctx, key)
    } else {
        newest(series(ctx, key), count)
    };
    let avg = average(&s)?;
    block(
        Some(window_meta(ctx.focus, default_meta)),
        None,
        charts::bar_chart(
            &day_bars(&s),
            Some((avg, format!("{word} {avg:.0}"))),
            ctx.t,
        ),
    )
}

fn stress(ctx: &Ctx) -> Option<Block> {
    bars_block(ctx, "stress", 14, "14 days · daily average", "avg")
}

fn steps(ctx: &Ctx) -> Option<Block> {
    bars_block(ctx, "steps", 14, "14 days", "avg")
}
