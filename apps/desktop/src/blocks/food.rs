//! The blocks of the Food screen. Each one hides when its data is missing.

use crate::scale::px;
use chrono::NaiveDate;
use garmin_api::body::FoodDay;
use garmin_core::nutrition::Meal;
use gpui::{div, prelude::*, SharedString};

use super::daily::{block, head_row, padded, thousands, weekday_initial, window_meta};
use super::{Block, BlockDef, Ctx, Go, Width};
use crate::charts::{self, Bar, MeterRow, Tone};
use crate::icons::Icon;
use crate::kit::{self, Column, ListRow};
use crate::screen::Screen;

pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "food_empty",
        title: "No food",
        group: Screen::Food,
        default_width: Width::Full,
        render: empty,
    },
    BlockDef {
        id: "food_fuel",
        title: "Fuel",
        group: Screen::Food,
        default_width: Width::Third,
        render: fuel,
    },
    BlockDef {
        id: "food_meals",
        title: "Meals today",
        group: Screen::Food,
        default_width: Width::TwoThirds,
        render: meals,
    },
    BlockDef {
        id: "food_calories",
        title: "Calories",
        group: Screen::Food,
        default_width: Width::Full,
        render: calories,
    },
    BlockDef {
        id: "food_days",
        title: "Eaten and burned",
        group: Screen::Food,
        default_width: Width::TwoThirds,
        render: days,
    },
    BlockDef {
        id: "food_water",
        title: "Water",
        group: Screen::Food,
        default_width: Width::Third,
        render: water,
    },
    BlockDef {
        id: "food_month",
        title: "Calories, 30 days",
        group: Screen::Food,
        default_width: Width::Full,
        render: month,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("food_empty", Width::Full),
    ("food_fuel", Width::Third),
    ("food_meals", Width::TwoThirds),
    ("food_calories", Width::Full),
    ("food_days", Width::TwoThirds),
    ("food_water", Width::Third),
    ("food_month", Width::Full),
];

pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

/// What was eaten so far, against the goal.
pub fn lede(ctx: &Ctx) -> Option<String> {
    let eaten = eaten_today(ctx)?;
    Some(match goal(ctx) {
        Some(g) => format!("{} of {} kcal so far.", thousands(eaten), thousands(g)),
        None => format!("{} kcal logged so far.", thousands(eaten)),
    })
}

pub fn crumb(_ctx: &Ctx) -> Option<String> {
    None
}

/* ------------------------------------------------------------------ data --- */

fn date_of(d: &FoodDay) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok()
}

/// The logged days up to the end of the focus, oldest first. A range keeps
/// only the days inside it, and `count` caps the length for one day.
fn food_days<'a>(ctx: &Ctx<'a>, count: usize) -> Vec<(NaiveDate, &'a FoodDay)> {
    let mut days: Vec<_> = ctx
        .body
        .0
        .food
        .days
        .iter()
        .filter_map(|d| Some((date_of(d)?, d)))
        .filter(|(date, _)| *date <= ctx.focus.end)
        .filter(|(date, _)| ctx.focus.days() <= 1 || *date >= ctx.focus.start)
        .collect();
    days.sort_by_key(|(date, _)| *date);
    let skip = days.len().saturating_sub(count);
    days.split_off(skip)
}

fn today_row<'a>(ctx: &Ctx<'a>) -> Option<&'a FoodDay> {
    ctx.body
        .0
        .food
        .days
        .iter()
        .find(|d| d.logged && date_of(d) == Some(ctx.focus.end))
}

/// The calories of the meals of the day, else the daily total.
fn eaten_today(ctx: &Ctx) -> Option<f64> {
    ctx.extras_view
        .and_then(|e| e.nutrition.logged_kcal)
        .or_else(|| today_row(ctx).and_then(|d| d.eaten_kcal))
        .filter(|k| *k > 0.0)
}

/// The goal in kcal. The goal that follows the activity of the day comes first.
fn goal(ctx: &Ctx) -> Option<f64> {
    let from_goals = ctx
        .extras_view
        .and_then(|e| e.nutrition.goals.as_ref())
        .and_then(|g| g.adjusted_calories.or(g.calories));
    from_goals
        .or(ctx.body.0.food.goal_kcal)
        .filter(|g| *g > 0.0)
}

/// The sum of a macro over the meals of the day. `None` when no meal has one.
fn macro_sum(meals: &[Meal], pick: fn(&Meal) -> Option<f64>) -> Option<f64> {
    let values: Vec<f64> = meals.iter().filter_map(pick).collect();
    (!values.is_empty()).then(|| values.iter().sum())
}

/// Why the screen is empty: no day has a food log and no meal is cached.
fn empty(ctx: &Ctx) -> Option<Block> {
    let note = ctx.body.0.food.empty.as_ref()?;
    let meals = ctx
        .extras_view
        .is_some_and(|e| !e.nutrition.meals.is_empty());
    (!meals).then(|| super::daily::empty_note(note, ctx.t))?
}

/* ------------------------------------------------------------------ fuel --- */

fn macro_rows(ctx: &Ctx) -> Vec<MeterRow> {
    let Some(extras) = ctx.extras_view else {
        return Vec::new();
    };
    let goals = extras.nutrition.goals.as_ref();
    let list: [(&str, Option<f64>, Option<f64>, Tone); 3] = [
        (
            "Protein",
            macro_sum(&extras.nutrition.meals, |m| m.protein_g),
            goals.and_then(|g| g.protein_g),
            Tone::Accent,
        ),
        (
            "Carbs",
            macro_sum(&extras.nutrition.meals, |m| m.carbs_g),
            goals.and_then(|g| g.carbs_g),
            Tone::Accent,
        ),
        (
            "Fat",
            macro_sum(&extras.nutrition.meals, |m| m.fat_g),
            goals.and_then(|g| g.fat_g),
            Tone::Muted,
        ),
    ];
    list.into_iter()
        .filter_map(|(name, eaten, goal, tone)| {
            let eaten = eaten?;
            let goal = goal.filter(|g| *g > 0.0);
            Some(MeterRow {
                name: name.into(),
                fraction: goal.map_or(1.0, |g| (eaten / g) as f32),
                value: format!("{eaten:.0} g").into(),
                share: goal.map_or(String::new(), |g| format!("of {g:.0}")).into(),
                tone,
            })
        })
        .collect()
}

fn fuel(ctx: &Ctx) -> Option<Block> {
    let eaten = eaten_today(ctx)?;
    let t = ctx.t;
    let (unit, words) = match goal(ctx) {
        Some(g) => (
            format!("of {} kcal", thousands(g)),
            format!("{:.0}%", eaten / g * 100.0),
        ),
        None => ("kcal".to_string(), String::new()),
    };
    let rows = macro_rows(ctx);
    let body = padded(
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(head_row(thousands(eaten), unit, words, t))
            .children((!rows.is_empty()).then(|| charts::meter_rows(rows, t))),
    );
    let meta = if ctx.focus.is_live() {
        "today".to_string()
    } else {
        ctx.focus.end.format("%-d %b").to_string()
    };
    block(Some(meta), Some(("Food", Go::Screen(Screen::Food))), body)
}

/* ----------------------------------------------------------------- meals --- */

fn macros_text(m: &Meal) -> Option<String> {
    let parts: Vec<String> = [("P", m.protein_g), ("C", m.carbs_g), ("F", m.fat_g)]
        .into_iter()
        .filter_map(|(k, v)| Some(format!("{k} {:.0}", v?)))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn meals(ctx: &Ctx) -> Option<Block> {
    let list = &ctx.extras_view?.nutrition.meals;
    if list.is_empty() {
        return None;
    }
    let rows: Vec<ListRow> = list
        .iter()
        .map(|m| ListRow {
            glyph: Icon::Check,
            tone: Tone::Accent,
            title: m.name.clone().unwrap_or_else(|| "Meal".into()).into(),
            sub: m.time.clone().map(Into::into),
            right: m.kcal.map(|k| format!("{} kcal", thousands(k)).into()),
            right_sub: macros_text(m).map(Into::into),
        })
        .collect();
    block(
        Some(format!("{} logged", rows.len())),
        None,
        kit::list_rows(rows, ctx.t),
    )
}

/* ------------------------------------------------------------- calories --- */

fn kcal_bars(days: &[(NaiveDate, &FoodDay)], long: bool) -> Vec<Bar> {
    days.iter()
        .map(|(date, d)| {
            let value = d.eaten_kcal.filter(|_| d.logged);
            Bar {
                value: value.unwrap_or(0.0) as f32,
                label: value
                    .filter(|_| !long)
                    .map_or(String::new(), thousands)
                    .into(),
                day: if long {
                    date.format("%-d").to_string().into()
                } else {
                    weekday_initial(*date)
                },
            }
        })
        .collect()
}

fn calories_block(ctx: &Ctx, count: usize, meta: &str, height: f32) -> Option<Block> {
    let days = food_days(ctx, count);
    if !days.iter().any(|(_, d)| d.logged && d.eaten_kcal.is_some()) {
        return None;
    }
    let line = goal(ctx).map(|g| (g as f32, format!("target {}", thousands(g))));
    block(
        Some(window_meta(ctx.focus, meta)),
        None,
        charts::bar_chart_sized(&kcal_bars(&days, count > 9), line, height, None, ctx.t),
    )
}

fn calories(ctx: &Ctx) -> Option<Block> {
    calories_block(ctx, 7, "7 days · kcal", 110.0)
}

fn month(ctx: &Ctx) -> Option<Block> {
    calories_block(ctx, 30, "30 days · kcal", 110.0)
}

/* ------------------------------------------------------------------ days --- */

fn signed(v: f64) -> String {
    if v > 0.5 {
        format!("+{}", thousands(v))
    } else {
        thousands(v)
    }
}

fn days(ctx: &Ctx) -> Option<Block> {
    let list = food_days(ctx, 7);
    let dash = || SharedString::from("-");
    let mut rows: Vec<Vec<SharedString>> = list
        .iter()
        .filter(|(_, d)| d.eaten_kcal.is_some() || d.burned_kcal.is_some())
        .map(|(date, d)| {
            vec![
                date.format("%a %-d %b").to_string().into(),
                d.eaten_kcal
                    .filter(|_| d.logged)
                    .map_or_else(dash, |k| thousands(k).into()),
                d.burned_kcal.map_or_else(dash, |k| thousands(k).into()),
                d.balance_kcal
                    .filter(|_| d.logged)
                    .map_or_else(dash, |k| signed(k).into()),
                d.water_ml
                    .map_or_else(dash, |ml| format!("{:.1} L", ml / 1000.0).into()),
            ]
        })
        .collect();
    if rows.is_empty() || !list.iter().any(|(_, d)| d.logged) {
        return None;
    }
    rows.reverse();
    let columns = [
        ("Date", false),
        ("Eaten", true),
        ("Burned", true),
        ("Balance", true),
        ("Water", true),
    ]
    .map(|(title, numeric)| Column {
        title: title.into(),
        numeric,
    });
    block(
        Some(window_meta(ctx.focus, "7 days · kcal")),
        None,
        kit::table(&columns, rows, ctx.t),
    )
}

/* ----------------------------------------------------------------- water --- */

fn water(ctx: &Ctx) -> Option<Block> {
    let w = ctx.body.0.food.water.as_ref()?;
    if w.avg_ml <= 0.0 {
        return None;
    }
    let litres = |ml: f64| format!("{:.1}", ml / 1000.0);
    let words = w
        .goal_ml
        .map_or(String::new(), |g| format!("goal {} L", litres(g)));
    let row = w.goal_ml.filter(|g| *g > 0.0).map(|g| {
        charts::meter_rows(
            vec![MeterRow {
                name: "Average".into(),
                fraction: (w.avg_ml / g) as f32,
                value: format!("{} L", litres(w.avg_ml)).into(),
                share: format!("{:.0}%", w.avg_ml / g * 100.0).into(),
                tone: Tone::Accent,
            }],
            ctx.t,
        )
    });
    let body = padded(
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(head_row(litres(w.avg_ml), "L a day".into(), words, ctx.t))
            .children(row),
    );
    block(Some("30-day average".into()), None, body)
}
