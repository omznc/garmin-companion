//! The blocks of the Fitness screen. Each one hides when its data is missing.

use crate::scale::px;
use chrono::{Duration, NaiveDate};
use garmin_api::fitness::LoadBar;
use gpui::{div, prelude::*, relative, Div};

use super::activities::{block, meta_for, padded, parse_day};
use super::{Block, BlockDef, Ctx, Width};
use crate::charts::{self, AreaChart, Bar};
use crate::kit::{self, Cell};
use crate::screen::Screen;
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

/// The weeks in the block of the weekly load.
const LOAD_WEEKS: i64 = 8;

pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "fitness_vo2",
        title: "VO2 max",
        group: Screen::Fitness,
        default_width: Width::Half,
        render: vo2,
    },
    BlockDef {
        id: "fitness_load",
        title: "Weekly training load",
        group: Screen::Fitness,
        default_width: Width::Half,
        render: load,
    },
    BlockDef {
        id: "fitness_predictions",
        title: "Race predictions",
        group: Screen::Fitness,
        default_width: Width::Third,
        render: predictions,
    },
    BlockDef {
        id: "fitness_status",
        title: "Training status",
        group: Screen::Fitness,
        default_width: Width::TwoThirds,
        render: status,
    },
    BlockDef {
        id: "fitness_age",
        title: "Fitness age",
        group: Screen::Fitness,
        default_width: Width::Third,
        render: age,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("fitness_vo2", Width::Half),
    ("fitness_load", Width::Half),
    ("fitness_status", Width::TwoThirds),
    ("fitness_predictions", Width::Third),
    ("fitness_age", Width::Third),
    ("activities_records", Width::Third),
];

pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

pub fn lede(ctx: &Ctx) -> Option<String> {
    ctx.training?.fitness.lede.clone()
}

pub fn crumb(ctx: &Ctx) -> Option<String> {
    Some(ctx.focus.label())
}

/* -------------------------------------------------------------------- VO2 --- */

fn vo2(ctx: &Ctx) -> Option<Block> {
    let days = &ctx.extras_view?.vo2;
    let from = if ctx.focus.days() > 1 {
        ctx.focus.start
    } else {
        ctx.focus.end - Duration::days(365)
    };
    let mut points: Vec<(NaiveDate, f64)> = days
        .iter()
        .filter_map(|d| Some((parse_day(&d.date)?, d.vo2)))
        .filter(|(d, _)| *d >= from && *d <= ctx.focus.end)
        .collect();
    points.sort_by_key(|p| p.0);
    let (first, last) = (points.first()?, points.last()?);
    if points.len() < 2 {
        return None;
    }
    let change = last.1 - first.1;
    let words = if change.abs() >= 0.5 {
        format!("{change:+.0} over the period")
    } else {
        "no change over the period".into()
    };
    let values: Vec<f32> = points.iter().map(|p| p.1 as f32).collect();
    let chart = charts::area_chart(
        AreaChart {
            values: &values,
            colour: ctx.t.acc,
            height: None,
            head: Some((
                format!("{:.0}", last.1).into(),
                "ml/kg/min".into(),
                words.into(),
            )),
            head_colour: None,
            x_first: first.0.format("%b %Y").to_string().into(),
            x_last: last.0.format("%b %Y").to_string().into(),
        },
        ctx.t,
    );
    block(Some(meta_for(&ctx.focus, "12 months")), None, chart)
}

/* ------------------------------------------------------------------- load --- */

fn load(ctx: &Ctx) -> Option<Block> {
    let rows = &ctx.activities.as_ref().ok()?.rows;
    let last = super::activities::monday(ctx.focus.end);
    let first = if ctx.focus.days() > 1 {
        super::activities::monday(ctx.focus.start).max(last - Duration::weeks(LOAD_WEEKS - 1))
    } else {
        last - Duration::weeks(LOAD_WEEKS - 1)
    };
    let mut bars = Vec::new();
    let mut start = first;
    while start <= last {
        let sum: f64 = super::activities::week_rows(rows, start)
            .filter_map(|r| r.load)
            .sum();
        bars.push(Bar {
            value: sum as f32,
            label: if sum > 0.0 {
                format!("{sum:.0}").into()
            } else {
                "".into()
            },
            day: start.format("%-d %b").to_string().into(),
        });
        start += Duration::weeks(1);
    }
    if bars.iter().all(|b| b.value <= 0.0) {
        return None;
    }
    let avg = bars.iter().map(|b| b.value).sum::<f32>() / bars.len() as f32;
    block(
        Some(meta_for(&ctx.focus, "8 weeks")),
        None,
        charts::bar_chart(&bars, Some((avg, format!("avg {avg:.0}"))), ctx.t),
    )
}

/* ------------------------------------------------------------ predictions --- */

fn figure_cells(figures: &[garmin_api::insights::Figure]) -> Vec<Cell> {
    figures
        .iter()
        .map(|f| Cell {
            label: f.label.clone().into(),
            value: f.value.clone().into(),
            unit: None,
            sub: None,
            spark: None,
            big: false,
        })
        .collect()
}

fn predictions(ctx: &Ctx) -> Option<Block> {
    let p = ctx.training?.fitness.predictions.as_ref()?;
    if p.rows.is_empty() {
        return None;
    }
    let grid = kit::stat_grid(figure_cells(&p.rows), 2, ctx.t);
    let intro = (!p.intro.is_empty()).then(|| note(&p.intro, ctx.t));
    block(
        Some("Garmin estimate".into()),
        None,
        div().flex_1().flex().flex_col().child(grid).children(intro),
    )
}

fn note(words: &str, t: &Theme) -> Div {
    // The top line closes the stat grid above it.
    padded(words.to_string())
        .text_size(px(12.0))
        .text_color(t.faint)
        .border_t_1()
        .border_color(t.line2)
}

/* ----------------------------------------------------------------- status --- */

/// One load bucket against the range that Garmin wants it in.
fn load_bar(bar: &LoadBar, t: &Theme) -> Div {
    let scale = (bar.max * 1.25).max(bar.value * 1.05);
    let fill = match bar.state.as_str() {
        "over" => t.warn,
        "under" => t.mutd,
        _ => t.acc,
    };
    let share = |v: f64| relative((v / scale).clamp(0.0, 1.0) as f32);
    let track = div()
        .relative()
        .h(px(8.0))
        .rounded(px(2.0))
        .bg(t.line2)
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(share(bar.min))
                .w(share(bar.max - bar.min))
                .rounded(px(2.0))
                .bg(t.line),
        )
        .child(
            div()
                .absolute()
                .top(px(2.0))
                .bottom(px(2.0))
                .left_0()
                .w(share(bar.value))
                .rounded(px(2.0))
                .bg(fill),
        );
    let caption = div()
        .flex()
        .justify_between()
        .mb(px(6.0))
        .text_size(px(12.0))
        .child(
            div()
                .font_family(SANS)
                .text_color(t.mutd)
                .child(bar.label.clone()),
        )
        .child(
            div()
                .font_family(MONO)
                .text_color(if bar.state == "over" { t.warn } else { t.mutd })
                .child(bar.text.clone()),
        );
    div().child(caption).child(track)
}

fn status(ctx: &Ctx) -> Option<Block> {
    let v = &ctx.training?.fitness;
    if v.empty.is_some() || (v.figures.is_empty() && v.balance.is_none()) {
        return None;
    }
    let t = ctx.t;
    let mut body = div();
    if !v.figures.is_empty() {
        body = body.child(kit::stat_grid(figure_cells(&v.figures), 4, t));
    }
    for n in &v.notes {
        body = body.child(note(n, t).border_t_1().border_color(t.line2));
    }
    if let Some(b) = &v.balance {
        let bars = b.bars.iter().fold(
            div().flex().flex_col().gap(px(14.0)).mt(px(10.0)),
            |col, bar| col.child(load_bar(bar, t)),
        );
        body = body.child(
            padded(
                div()
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.faint)
                            .child("Load balance, this month"),
                    )
                    .child(bars)
                    .child(
                        div()
                            .mt(px(10.0))
                            .text_size(px(12.0))
                            .text_color(t.faint)
                            .child(b.intro.clone()),
                    ),
            )
            .border_t_1()
            .border_color(t.line2),
        );
    }
    block(Some(ctx.focus.label()), None, body)
}

/* -------------------------------------------------------------------- age --- */

fn age(ctx: &Ctx) -> Option<Block> {
    let a = ctx.extras_view?.fitness_age.as_ref()?;
    let fitness = a.fitness_age?;
    let cell = |label: &str, value: f64| Cell {
        label: label.to_string().into(),
        value: format!("{value:.0}").into(),
        unit: None,
        sub: None,
        spark: None,
        big: false,
    };
    let mut cells = vec![cell("Fitness age", fitness)];
    cells.extend(a.chronological_age.map(|v| cell("Your age", v)));
    cells.extend(a.achievable_age.map(|v| cell("Achievable", v)));
    let columns = cells.len();
    block(None, None, kit::stat_grid(cells, columns, ctx.t))
}
