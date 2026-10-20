//! The blocks of the Strength screen. Each one hides when its data is missing.

use chrono::Duration;
use garmin_api::body::StrengthRow;
use gpui::prelude::*;

use super::activities::{block, meta_for, padded, parse_day};
use super::{Block, BlockDef, Ctx, Width};
use crate::charts::{self, Bar, MeterRow, Tone};
use crate::icons::Icon;
use crate::kit::{self, Cell, ListRow};
use crate::screen::Screen;

/// The days that the list of workouts and the lede look back.
const WINDOW_DAYS: i64 = 14;

pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "strength_recent",
        title: "Recent workouts",
        group: Screen::Strength,
        default_width: Width::TwoThirds,
        render: recent,
    },
    BlockDef {
        id: "strength_muscle",
        title: "Sets by muscle group",
        group: Screen::Strength,
        default_width: Width::Third,
        render: muscle,
    },
    BlockDef {
        id: "strength_weeks",
        title: "Sets per week",
        group: Screen::Strength,
        default_width: Width::Full,
        render: weeks,
    },
    BlockDef {
        id: "strength_exercises",
        title: "Top exercises",
        group: Screen::Strength,
        default_width: Width::TwoThirds,
        render: exercises,
    },
    BlockDef {
        id: "strength_numbers",
        title: "Per workout",
        group: Screen::Strength,
        default_width: Width::Third,
        render: numbers,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("strength_recent", Width::TwoThirds),
    ("strength_muscle", Width::Third),
    ("strength_weeks", Width::Full),
    ("strength_exercises", Width::TwoThirds),
    ("strength_numbers", Width::Third),
];

pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

/// The workouts in the window, and the group with the most sets.
pub fn lede(ctx: &Ctx) -> Option<String> {
    let n = window(ctx).len();
    if n == 0 {
        return None;
    }
    let mut text = match n {
        1 => "1 workout".to_string(),
        n => format!("{n} workouts"),
    };
    text.push_str(&if ctx.focus.days() > 1 {
        " in the range.".to_string()
    } else {
        format!(" in the last {WINDOW_DAYS} days.")
    });
    if let Some(top) = ctx
        .charts_view
        .and_then(|c| c.sets_by_muscle.groups.first())
    {
        text.push_str(&format!(" {} get the most sets.", top.group));
    }
    Some(text)
}

pub fn crumb(ctx: &Ctx) -> Option<String> {
    Some(if ctx.focus.days() > 1 {
        ctx.focus.label()
    } else {
        "Last 30 days".into()
    })
}

/// The workouts in the window: the range, or the last 14 days to the end.
fn window<'a>(ctx: &'a Ctx) -> Vec<&'a StrengthRow> {
    let (from, to) = if ctx.focus.days() > 1 {
        (ctx.focus.start, ctx.focus.end)
    } else {
        (
            ctx.focus.end - Duration::days(WINDOW_DAYS - 1),
            ctx.focus.end,
        )
    };
    ctx.body
        .0
        .strength
        .sessions
        .iter()
        .filter(|s| parse_day(&s.date).is_some_and(|d| d >= from && d <= to))
        .collect()
}

fn recent(ctx: &Ctx) -> Option<Block> {
    let sessions = window(ctx);
    if sessions.is_empty() {
        return None;
    }
    let rows = sessions
        .iter()
        .take(10)
        .map(|s| ListRow {
            glyph: Icon::NavStrength,
            tone: Tone::Muted,
            title: s.label.clone().into(),
            sub: s.movements.clone().map(Into::into),
            right: Some(s.summary.clone().into()),
            right_sub: None,
        })
        .collect();
    block(
        Some(meta_for(&ctx.focus, "last 14 days")),
        None,
        kit::list_rows(rows, ctx.t),
    )
}

fn muscle(ctx: &Ctx) -> Option<Block> {
    let groups = &ctx.charts_view?.sets_by_muscle.groups;
    let most = groups.first()?.sets.max(1) as f32;
    let rows = groups
        .iter()
        .enumerate()
        .map(|(i, g)| MeterRow {
            name: g.group.clone().into(),
            fraction: g.sets as f32 / most,
            value: g.sets.to_string().into(),
            share: "sets".into(),
            tone: if i == 0 { Tone::Accent } else { Tone::Muted },
        })
        .collect();
    block(
        Some("30 days".into()),
        None,
        padded(charts::meter_rows(rows, ctx.t)),
    )
}

fn weeks(ctx: &Ctx) -> Option<Block> {
    let list: Vec<_> = ctx
        .charts_view?
        .sets_per_week
        .iter()
        .filter(|w| {
            ctx.focus.days() <= 1
                || parse_day(&w.week_start).is_some_and(|start| {
                    start <= ctx.focus.end && start + Duration::days(6) >= ctx.focus.start
                })
        })
        .collect();
    if list.iter().all(|w| w.sets == 0) {
        return None;
    }
    let bars: Vec<Bar> = list
        .iter()
        .map(|w| Bar {
            value: w.sets as f32,
            label: if w.sets > 0 {
                w.sets.to_string().into()
            } else {
                "".into()
            },
            day: w
                .iso_week
                .rsplit("-W")
                .next()
                .unwrap_or("")
                .to_string()
                .into(),
        })
        .collect();
    let avg = bars.iter().map(|b| b.value).sum::<f32>() / bars.len() as f32;
    block(
        Some(meta_for(&ctx.focus, "12 weeks")),
        None,
        charts::bar_chart(&bars, Some((avg, format!("avg {avg:.0}"))), ctx.t),
    )
}

fn exercises(ctx: &Ctx) -> Option<Block> {
    let list = &ctx.body.0.strength.exercises;
    if list.is_empty() {
        return None;
    }
    let rows = list
        .iter()
        .take(8)
        .map(|e| ListRow {
            glyph: Icon::NavStrength,
            tone: Tone::Muted,
            title: e.name.clone().into(),
            sub: (!e.detail.is_empty()).then(|| e.detail.clone().into()),
            right: (!e.reps_text.is_empty()).then(|| e.reps_text.clone().into()),
            right_sub: None,
        })
        .collect();
    block(
        Some("names are guesses".into()),
        None,
        kit::list_rows(rows, ctx.t),
    )
}

fn numbers(ctx: &Ctx) -> Option<Block> {
    let v = &ctx.body.0.strength;
    let mut cells = Vec::new();
    let mut add = |label: &str, value: Option<String>, unit: Option<&str>| {
        cells.extend(value.map(|value| Cell {
            label: label.to_string().into(),
            value: value.into(),
            unit: unit.map(|u| u.to_string().into()),
            sub: None,
            spark: None,
            big: false,
        }));
    };
    add("Sets", v.avg_sets.map(|x| format!("{x:.0}")), None);
    add("Reps", v.avg_reps.map(|x| format!("{x:.0}")), None);
    add(
        "Typical rest",
        v.median_rest_s.map(garmin_api::body::mmss),
        Some("min"),
    );
    add(
        "Sets named",
        v.identified_pct.map(|p| p.to_string()),
        Some("%"),
    );
    if cells.is_empty() {
        return None;
    }
    let grid = kit::stat_grid(cells, 2, ctx.t);
    let caveat = (!v.caveat.is_empty()).then(|| {
        padded(v.caveat.clone())
            .text_size(crate::scale::px(12.0))
            .text_color(ctx.t.faint)
            .border_t_1()
            .border_color(ctx.t.line2)
    });
    block(
        Some("average".into()),
        None,
        gpui::div().child(grid).children(caveat),
    )
}
