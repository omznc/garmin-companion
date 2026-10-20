//! The blocks of the Sleep screen. Each one hides when its data is missing.

use crate::scale::px;
use chrono::NaiveDate;
use garmin_api::sleep_view::{NightView, SleepWindow, StageSpan};
use gpui::{div, prelude::*, relative, ColorExt as _, Div, Hsla};

use super::daily::{
    block, cell, dated, head_row, hm, key_figures as grid, label_row, padded, weekday_initial,
    window_meta,
};
use super::{Block, BlockDef, Ctx, Go, Width};
use crate::charts::{self, Bar, MeterRow, Tone};
use crate::icons::Icon;
use crate::kit::{self, ListRow};
use crate::screen::Screen;
use crate::theme::Theme;
use crate::ui::MONO;

pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "sleep_night",
        title: "Last night",
        group: Screen::Sleep,
        default_width: Width::TwoThirds,
        render: night,
    },
    BlockDef {
        id: "sleep_stages",
        title: "Stages",
        group: Screen::Sleep,
        default_width: Width::Third,
        render: stages,
    },
    BlockDef {
        id: "sleep_score",
        title: "Sleep score",
        group: Screen::Sleep,
        default_width: Width::TwoThirds,
        render: score,
    },
    BlockDef {
        id: "sleep_times",
        title: "Bed and wake times",
        group: Screen::Sleep,
        default_width: Width::Third,
        render: times,
    },
    BlockDef {
        id: "sleep_insights",
        title: "What your nights say",
        group: Screen::Sleep,
        default_width: Width::TwoThirds,
        render: insights,
    },
    BlockDef {
        id: "sleep_details",
        title: "Night details",
        group: Screen::Sleep,
        default_width: Width::Third,
        render: details,
    },
    BlockDef {
        id: "sleep_week",
        title: "Hours, last 7 nights",
        group: Screen::Sleep,
        default_width: Width::Half,
        render: week,
    },
    BlockDef {
        id: "sleep_month",
        title: "Hours, last 30 nights",
        group: Screen::Sleep,
        default_width: Width::Full,
        render: month,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("sleep_night", Width::TwoThirds),
    ("sleep_stages", Width::Third),
    ("sleep_score", Width::TwoThirds),
    ("sleep_times", Width::Third),
    ("sleep_insights", Width::TwoThirds),
    ("sleep_details", Width::Third),
    ("sleep_week", Width::Half),
    ("sleep_month", Width::Full),
];

pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

/// The sentence of the night, and the sleep debt.
pub fn lede(ctx: &Ctx) -> Option<String> {
    let view = ctx.wellness.map(|w| &w.sleep)?;
    let debt = ctx
        .tiles_view
        .and_then(|t| t.sleep_debt.as_ref())
        .filter(|d| d.debt_secs >= 60.0)
        .map(|d| {
            let m = (d.debt_secs / 60.0).round() as i64;
            format!(
                " You are {} h {:02} m short over the last {} nights.",
                m / 60,
                m % 60,
                d.nights
            )
        });
    Some(format!("{}{}", view.lede, debt.unwrap_or_default()))
}

/// "Night of 19 October".
pub fn crumb(ctx: &Ctx) -> Option<String> {
    let date = night_view(ctx)
        .and_then(|n| NaiveDate::parse_from_str(&n.date, "%Y-%m-%d").ok())
        .unwrap_or(ctx.focus.end);
    Some(format!("Night of {}", date.format("%-d %B")))
}

fn night_view<'a>(ctx: &Ctx<'a>) -> Option<&'a NightView> {
    ctx.wellness?.sleep.last_night.as_ref()
}

/* ------------------------------------------------------------- hypnogram --- */

fn stage_colour(key: &str, t: &Theme) -> Hsla {
    match key {
        "deep" => t.acc,
        "rem" => t.acc.opacity(0.7),
        "light" => t.acc.opacity(0.35),
        _ => t.warn,
    }
}

/// The row of a stage in the hypnogram, awake at the top.
fn lane(key: &str) -> f32 {
    match key {
        "awake" => 0.0,
        "rem" => 1.0,
        "light" => 2.0,
        _ => 3.0,
    }
}

const HYPNO_H: f32 = 120.0;

fn plot(spans: &[StageSpan], total: f64, t: &Theme) -> Div {
    let mut area = div()
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(HYPNO_H))
        .rounded(px(4.0));
    for i in 0..4 {
        area = area.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(relative(i as f32 / 4.0))
                .h(px(1.0))
                .bg(t.line2),
        );
    }
    for s in spans {
        area = area.child(
            div()
                .absolute()
                .left(relative((s.from_mins / total) as f32))
                .w(relative((s.mins / total) as f32))
                .top(px(lane(&s.key) * 30.0 + 4.0))
                .h(px(22.0))
                .pr(px(1.0))
                .child(
                    div()
                        .size_full()
                        .rounded(px(2.0))
                        .bg(stage_colour(&s.key, t)),
                ),
        );
    }
    area
}

fn lane_names(t: &Theme) -> Div {
    div()
        .flex_none()
        .w(px(44.0))
        .h(px(HYPNO_H))
        .flex()
        .flex_col()
        .font_family(MONO)
        .text_size(px(10.5))
        .text_color(t.faint)
        .children(
            ["Awake", "REM", "Light", "Deep"]
                .map(|name| div().h(relative(0.25)).flex().items_center().child(name)),
        )
}

/// Five clock labels over the night, from the start time `HH:MM`.
fn clock_labels(start: &str, total_mins: f64) -> Vec<String> {
    let (Some(h), Some(m)) = (
        start.get(0..2).and_then(|s| s.parse::<f64>().ok()),
        start.get(3..5).and_then(|s| s.parse::<f64>().ok()),
    ) else {
        return Vec::new();
    };
    (0..5)
        .map(|i| {
            let at = (h * 60.0 + m + total_mins * f64::from(i) / 4.0).rem_euclid(1440.0);
            format!("{:02}:{:02}", (at / 60.0) as i64, (at % 60.0) as i64)
        })
        .collect()
}

fn legend(n: &NightView, t: &Theme) -> Div {
    let mut row = div()
        .flex()
        .flex_wrap()
        .gap_x(px(14.0))
        .gap_y(px(6.0))
        .text_size(px(12.0))
        .text_color(t.mutd);
    for key in ["deep", "light", "rem", "awake"] {
        let Some(s) = n.stages.iter().find(|s| s.key == key) else {
            continue;
        };
        row = row.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .size(px(9.0))
                        .rounded(px(2.0))
                        .bg(stage_colour(key, t)),
                )
                .child(s.label.clone())
                .child(div().font_family(MONO).text_color(t.fg).child(hm(s.secs))),
        );
    }
    row
}

fn night(ctx: &Ctx) -> Option<Block> {
    let n = night_view(ctx)?;
    let total = n.timeline.last().map(|s| s.from_mins + s.mins)?;
    if total <= 0.0 {
        return None;
    }
    let t = ctx.t;
    let words = [
        n.score.map(|s| format!("score {s:.0}")),
        n.qualifier.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let labels = n
        .start
        .as_deref()
        .map(|s| clock_labels(s, total))
        .unwrap_or_default();
    let body =
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .child(head_row(n.duration_text.clone(), String::new(), words, t))
            .child(div().flex().gap(px(8.0)).child(lane_names(t)).child(plot(
                &n.timeline,
                total,
                t,
            )))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().flex_none().w(px(44.0)))
                    .child(div().flex_1().child(label_row(labels, t))),
            )
            .child(legend(n, t));
    let meta = n
        .start
        .clone()
        .zip(n.end.clone())
        .map(|(a, b)| format!("{a} – {b}"));
    block(meta, Some(("Sleep", Go::Screen(Screen::Sleep))), body)
}

/* ---------------------------------------------------------------- stages --- */

fn stages(ctx: &Ctx) -> Option<Block> {
    let n = night_view(ctx)?;
    if n.stages.is_empty() {
        return None;
    }
    let longest = n.stages.iter().map(|s| s.secs).fold(0.0_f64, f64::max);
    let rows = ["deep", "light", "rem", "awake"]
        .iter()
        .filter_map(|key| n.stages.iter().find(|s| s.key == *key))
        .map(|s| MeterRow {
            name: s.label.clone().into(),
            fraction: (s.secs / longest) as f32,
            value: hm(s.secs).into(),
            share: format!("{:.0}%", s.pct).into(),
            tone: if s.key == "awake" {
                Tone::Warn
            } else {
                Tone::Accent
            },
        })
        .collect();
    block(
        Some("last night".into()),
        None,
        padded(charts::meter_rows(rows, ctx.t)),
    )
}

/* ----------------------------------------------------------------- score --- */

fn score(ctx: &Ctx) -> Option<Block> {
    let scores = &ctx.tiles_view?.sleep_scores;
    let series = dated(&scores.points, ctx.focus);
    let present = series.iter().filter_map(|(_, v)| *v).count();
    if present == 0 {
        return None;
    }
    let bars: Vec<Bar> = series
        .iter()
        .map(|(date, v)| Bar {
            value: v.unwrap_or(0.0) as f32,
            label: v.map_or(String::new(), |v| format!("{v:.0}")).into(),
            day: weekday_initial(*date),
        })
        .collect();
    let avg = (series.iter().filter_map(|(_, v)| *v).sum::<f64>() / present as f64) as f32;
    block(
        Some(window_meta(ctx.focus, "14 nights")),
        None,
        charts::bar_chart_sized(
            &bars,
            Some((avg, format!("avg {avg:.0}"))),
            132.0,
            Some(100.0),
            ctx.t,
        ),
    )
}

/* ----------------------------------------------------------------- times --- */

fn clock_text(minute: f64) -> String {
    format!(
        "{:02}:{:02}",
        (minute / 60.0) as i64 % 24,
        (minute % 60.0).round() as i64 % 60
    )
}

fn times(ctx: &Ctx) -> Option<Block> {
    let tiles = ctx.tiles_view?;
    let mut cells = Vec::new();
    if let Some(b) = &tiles.bed_wake {
        cells.push(cell("Bedtime", clock_text(b.bed_minute), None));
        cells.push(cell("Wake time", clock_text(b.wake_minute), None));
        cells.push(cell(
            "Consistency",
            format!("±{:.0}", b.consistency_mins),
            Some("min"),
        ));
    }
    if let Some(d) = &tiles.sleep_debt {
        cells.push(cell("Sleep debt", hm(d.debt_secs), Some("h")));
    }
    grid(cells, "14-night average", ctx.t)
}

/* --------------------------------------------------------------- details --- */

fn details(ctx: &Ctx) -> Option<Block> {
    let n = night_view(ctx)?;
    let whole = |v: Option<f64>| v.map(|v| format!("{v:.0}"));
    let items = [
        ("Awake periods", whole(n.awake_count), None),
        ("Restless moments", whole(n.restless_count), None),
        ("Breathing", whole(n.avg_respiration), Some("br/min")),
        ("Blood oxygen", whole(n.avg_spo2), Some("%")),
        ("Lowest oxygen", whole(n.lowest_spo2), Some("%")),
        ("Naps", n.nap_secs.map(hm), Some("h")),
    ];
    let cells = items
        .into_iter()
        .filter_map(|(label, v, unit)| Some(cell(label, v?, unit)))
        .collect();
    grid(cells, "last night", ctx.t)
}

/* -------------------------------------------------------------- insights --- */

fn insights(ctx: &Ctx) -> Option<Block> {
    let list = &ctx.wellness?.sleep.insights;
    if list.is_empty() {
        return None;
    }
    let rows = list
        .iter()
        .map(|i| {
            use garmin_core::sleep::Tone as T;
            let (glyph, tone) = match i.tone {
                T::Good => (Icon::Check, Tone::Accent),
                T::Note => (Icon::Info, Tone::Accent),
                T::Watch => (Icon::Warning, Tone::Warn),
            };
            ListRow {
                glyph,
                tone,
                title: i.claim.clone().into(),
                sub: Some(i.detail.clone().into()),
                right: None,
                right_sub: None,
            }
        })
        .collect();
    block(None, None, kit::list_rows(rows, ctx.t))
}

/* --------------------------------------------------------------- windows --- */

fn window_bars(w: &SleepWindow, focus: crate::focus::Focus) -> Vec<Bar> {
    let kept: Vec<_> = w
        .bars
        .iter()
        .filter_map(|b| {
            let date = NaiveDate::parse_from_str(&b.date, "%Y-%m-%d").ok()?;
            (focus.days() <= 1 || focus.contains(date)).then_some((date, b.hours))
        })
        .collect();
    kept.into_iter()
        .map(|(date, hours)| Bar {
            value: hours.unwrap_or(0.0) as f32,
            label: hours.map_or(String::new(), |h| format!("{h:.1}")).into(),
            day: if w.days > 9 {
                date.format("%-d").to_string().into()
            } else {
                weekday_initial(date)
            },
        })
        .collect()
}

fn window_block(ctx: &Ctx, w: &SleepWindow, meta: &str) -> Option<Block> {
    let bars = window_bars(w, ctx.focus);
    let hours: Vec<f32> = bars.iter().map(|b| b.value).filter(|v| *v > 0.0).collect();
    if hours.is_empty() {
        return None;
    }
    let avg = hours.iter().sum::<f32>() / hours.len() as f32;
    let body = div()
        .child(charts::bar_chart(
            &bars,
            Some((avg, format!("avg {avg:.1}"))),
            ctx.t,
        ))
        .child(
            padded(w.summary.clone())
                .pt(px(0.0))
                .text_size(px(12.0))
                .text_color(ctx.t.mutd),
        );
    block(Some(window_meta(ctx.focus, meta)), None, body)
}

fn week(ctx: &Ctx) -> Option<Block> {
    window_block(ctx, &ctx.wellness?.sleep.week, "7 nights · hours")
}

fn month(ctx: &Ctx) -> Option<Block> {
    window_block(ctx, &ctx.wellness?.sleep.month, "30 nights · hours")
}
