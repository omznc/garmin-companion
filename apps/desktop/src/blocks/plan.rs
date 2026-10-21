//! The blocks of the Plan screen: the week, the goal and the next workout.

use crate::scale::px;
use chrono::{Datelike, Duration, NaiveDate};
use garmin_api::activities::ActivityCard;
use garmin_api::extras::PlannedItem;
use garmin_core::calendar::PlanStep;
use gpui::{div, prelude::*, Div};

use super::{Block, BlockDef, Ctx, Width};
use crate::charts::Tone;
use crate::focus::Focus;
use crate::icons::Icon;
use crate::kit::{self, Cell, ListRow};
use crate::screen::Screen;
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

/// The blocks that belong to this screen.
pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "plan_week",
        title: "This week",
        group: Screen::Plan,
        default_width: Width::Full,
        render: week,
    },
    BlockDef {
        id: "plan_goal",
        title: "Goal",
        group: Screen::Plan,
        default_width: Width::Third,
        render: goal,
    },
    BlockDef {
        id: "plan_next",
        title: "Next workout",
        group: Screen::Plan,
        default_width: Width::TwoThirds,
        render: next_workout,
    },
    BlockDef {
        id: "plan_upcoming",
        title: "Next 14 days",
        group: Screen::Plan,
        default_width: Width::TwoThirds,
        render: upcoming,
    },
    BlockDef {
        id: "plan_summary",
        title: "Done and planned",
        group: Screen::Plan,
        default_width: Width::Third,
        render: summary,
    },
    BlockDef {
        id: "plan_saved",
        title: "Saved workouts",
        group: Screen::Plan,
        default_width: Width::Full,
        render: saved,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("plan_week", Width::Full),
    ("plan_goal", Width::Third),
    ("plan_next", Width::TwoThirds),
    ("plan_upcoming", Width::TwoThirds),
    ("plan_summary", Width::Third),
    ("plan_saved", Width::Full),
];

pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

pub fn lede(_ctx: &Ctx) -> Option<String> {
    Some("The planned workouts of the week, and the sessions that you did.".into())
}

pub fn crumb(ctx: &Ctx) -> Option<String> {
    Some(week_focus(ctx.focus).label())
}

fn block(meta: Option<String>, body: impl IntoElement) -> Option<Block> {
    Some(Block {
        meta: meta.map(Into::into),
        link: None,
        body: body.into_any_element(),
    })
}

fn key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// The Monday to Sunday week that holds the end of the focus.
fn week_focus(focus: Focus) -> Focus {
    let monday = focus.end - Duration::days(i64::from(focus.end.weekday().num_days_from_monday()));
    Focus::range(monday, monday + Duration::days(6))
}

fn sport_text(raw: &str) -> String {
    raw.replace('_', " ")
}

/* ------------------------------------------------------------------- week --- */

/// One day of the week as the cell shows it.
struct Day {
    label: String,
    today: bool,
    title: Option<String>,
    sub: Option<String>,
    /// The state chip: its text, and whether it takes the accent.
    chip: Option<(&'static str, bool)>,
}

fn sessions_on<'a>(ctx: &'a Ctx, date: &str) -> Vec<&'a ActivityCard> {
    ctx.activities
        .as_ref()
        .ok()
        .and_then(|v| v.groups.iter().find(|g| g.date == date))
        .map(|g| g.sessions.iter().collect())
        .unwrap_or_default()
}

fn planned_on<'a>(ctx: &'a Ctx, date: &str) -> Vec<&'a PlannedItem> {
    ctx.extras_view
        .into_iter()
        .flat_map(|e| &e.planned)
        .filter(|p| p.item.date == date)
        .collect()
}

fn day_text(done: &[&ActivityCard], planned: &[&PlannedItem]) -> (Option<String>, Option<String>) {
    if let Some(first) = done.first() {
        let sub = match done.len() {
            1 => first.headline.clone(),
            n => format!("{} + {} more", first.headline, n - 1),
        };
        return (Some(first.name.clone()), Some(sub));
    }
    match planned.first() {
        Some(p) => {
            let sport = p.item.sport.as_deref().map(sport_text);
            let title = p.item.title.clone().or_else(|| sport.clone());
            (title.or(Some("Workout".into())), sport)
        }
        None => (None, None),
    }
}

fn day_of(ctx: &Ctx, date: NaiveDate) -> Day {
    let k = key(date);
    let (done, planned) = (sessions_on(ctx, &k), planned_on(ctx, &k));
    let today = date == ctx.focus.end;
    let chip = if !done.is_empty() {
        Some(("Done", true))
    } else if today {
        Some(("Today", true))
    } else if !planned.is_empty() {
        Some(("Planned", false))
    } else {
        None
    };
    let (title, sub) = day_text(&done, &planned);
    Day {
        label: date.format("%a %-d").to_string(),
        today,
        title,
        sub,
        chip,
    }
}

fn day_cell(day: Day, last: bool, t: &Theme) -> Div {
    let label_colour = if day.today { t.acc } else { t.faint };
    div()
        .flex_1()
        .min_w(px(110.0))
        .min_h(px(110.0))
        .px(px(12.0))
        .py(px(10.0))
        .flex()
        .flex_col()
        .gap(px(3.0))
        .text_size(px(13.0))
        .when(day.today, |d| d.bg(t.acctint))
        .when(!last, |d| d.border_r_1().border_color(t.line2))
        .child(
            div()
                .font_family(MONO)
                .text_size(px(11.0))
                .text_color(label_colour)
                .child(day.label),
        )
        .children(
            day.title
                .map(|title| div().font_weight(gpui::FontWeight::MEDIUM).child(title)),
        )
        .children(day.sub.map(|sub| {
            div()
                .font_family(SANS)
                .text_size(px(12.0))
                .text_color(t.mutd)
                .child(sub)
        }))
        .child(div().flex_1())
        .children(
            day.chip
                .map(|(text, accent)| div().child(kit::chip(text, accent, false, t))),
        )
}

fn has_no_plan(ctx: &Ctx) -> bool {
    ctx.extras_view
        .is_some_and(|e| e.planned.is_empty() && e.race.is_none() && e.active_plan.is_none())
}

fn week(ctx: &Ctx) -> Option<Block> {
    let range = week_focus(ctx.focus);
    let days: Vec<Day> = (0..7)
        .map(|i| day_of(ctx, range.start + Duration::days(i)))
        .collect();
    let mut row = div().flex().overflow_hidden();
    for (i, day) in days.into_iter().enumerate() {
        row = row.child(day_cell(day, i == 6, ctx.t));
    }
    let note = has_no_plan(ctx).then(|| {
        div()
            .px(px(12.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(ctx.t.line2)
            .text_size(px(12.0))
            .text_color(ctx.t.faint)
            .child("Garmin has no plan for this account.")
    });
    block(Some(range.label()), div().child(row).children(note))
}

/* ------------------------------------------------------------------- goal --- */

fn clock(secs: f64) -> String {
    let s = secs.round() as i64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// The label of the race prediction that suits the name of the race.
fn distance_key(name: &str) -> Option<&'static str> {
    let name = name.to_lowercase();
    if name.contains("half") {
        Some("Half")
    } else if name.contains("marathon") {
        Some("Marathon")
    } else if name.contains("10k") || name.contains("10 k") {
        Some("10K")
    } else if name.contains("5k") || name.contains("5 k") {
        Some("5K")
    } else {
        None
    }
}

fn predicted(ctx: &Ctx, name: &str) -> Option<String> {
    let want = distance_key(name)?;
    let rows = &ctx.training?.fitness.predictions.as_ref()?.rows;
    rows.iter()
        .find(|r| r.label == want)
        .map(|r| r.value.clone())
}

fn cell(label: &str, value: String) -> Cell {
    Cell {
        label: label.to_string().into(),
        value: value.into(),
        unit: None,
        sub: None,
        spark: None,
        big: false,
    }
}

fn goal(ctx: &Ctx) -> Option<Block> {
    let race = ctx.extras_view?.race.as_ref()?;
    let date = NaiveDate::parse_from_str(&race.date, "%Y-%m-%d").ok()?;
    let left = (date - ctx.focus.end).num_days();
    if left < 0 {
        return None;
    }
    let name = race.name.clone().unwrap_or_default();
    let mut cells = vec![
        cell("Race day", date.format("%-d %b").to_string()),
        cell("Days left", left.to_string()),
    ];
    cells.extend(race.target_secs.map(|s| cell("Target", clock(s))));
    cells.extend(predicted(ctx, &name).map(|p| cell("Predicted", p)));
    let meta = (!name.is_empty()).then_some(name);
    block(meta, kit::stat_grid(cells, 2, ctx.t))
}

/* ----------------------------------------------------------- next workout --- */

fn kind_label(kind: &str) -> String {
    match kind {
        "warmup" => "Warm-up".into(),
        "cooldown" => "Cool-down".into(),
        "interval" => "Interval".into(),
        "recovery" => "Recovery".into(),
        "rest" => "Rest".into(),
        other => {
            let text = sport_text(other);
            let mut chars = text.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

/// The length of a step: seconds for `time`, metres for `distance`.
fn step_length(step: &PlanStep) -> Option<String> {
    let value = step.end_value?;
    match step.end_type.as_deref()? {
        "time" if value >= 120.0 => Some(format!("{} min", (value / 60.0).round())),
        "time" => Some(format!("{} s", value.round())),
        "distance" if value >= 1000.0 => Some(format!("{} km", value / 1000.0)),
        "distance" => Some(format!("{} m", value.round())),
        _ => None,
    }
}

/// The target of a step: the zone, or a heart rate range.
fn step_target(step: &PlanStep) -> Option<String> {
    if let Some(zone) = step.zone {
        return Some(format!("Z{zone}"));
    }
    if step.target_type.as_deref() != Some("heart.rate.zone") {
        return None;
    }
    let (low, high) = (step.target_low?, step.target_high?);
    Some(format!("{}-{} bpm", low.round(), high.round()))
}

fn step_row(step: &PlanStep) -> ListRow {
    let kind = kind_label(&step.kind);
    let title = if step.repeat > 1 {
        format!("{} × {kind}", step.repeat)
    } else {
        kind
    };
    ListRow {
        glyph: Icon::NavFitness,
        tone: if step.kind == "interval" {
            Tone::Accent
        } else {
            Tone::Muted
        },
        title: title.into(),
        sub: step.description.clone().map(Into::into),
        right: step_length(step).map(Into::into),
        right_sub: step_target(step).map(Into::into),
    }
}

/// The next planned workout that has steps. A hard one (with an interval) goes first.
fn next_with_steps<'a>(ctx: &'a Ctx) -> Option<&'a PlannedItem> {
    let items = || {
        ctx.extras_view
            .into_iter()
            .flat_map(|e| &e.planned)
            .filter(|p| p.steps.as_ref().is_some_and(|s| !s.steps.is_empty()))
    };
    items()
        .find(|p| {
            p.steps
                .iter()
                .any(|s| s.steps.iter().any(|st| st.kind == "interval"))
        })
        .or_else(|| items().next())
}

fn next_workout(ctx: &Ctx) -> Option<Block> {
    let item = next_with_steps(ctx)?;
    let steps = item.steps.as_ref()?;
    let date = NaiveDate::parse_from_str(&item.item.date, "%Y-%m-%d").ok()?;
    let name = item.item.title.clone().or_else(|| steps.name.clone());
    let meta = match name {
        Some(n) => format!("{} · {n}", date.format("%a %-d %b")),
        None => date.format("%a %-d %b").to_string(),
    };
    let rows = steps.steps.iter().map(step_row).collect();
    block(Some(meta), kit::list_rows(rows, ctx.t))
}

/* ------------------------------------------------------------ more blocks --- */

/// The planned items of the next days, soonest first.
fn upcoming(ctx: &Ctx) -> Option<Block> {
    let planned = &ctx.extras_view?.planned;
    if planned.is_empty() {
        return None;
    }
    let rows = planned
        .iter()
        .map(|p| {
            let date = NaiveDate::parse_from_str(&p.item.date, "%Y-%m-%d").ok();
            ListRow {
                glyph: if p.item.is_race {
                    Icon::Trophy
                } else {
                    Icon::NavPlan
                },
                tone: if p.item.is_race {
                    Tone::Accent
                } else {
                    Tone::Muted
                },
                title: p
                    .item
                    .title
                    .clone()
                    .unwrap_or_else(|| "Workout".into())
                    .into(),
                sub: p.item.sport.as_deref().map(|s| sport_text(s).into()),
                right: date.map(|d| d.format("%a %-d %b").to_string().into()),
                right_sub: None,
            }
        })
        .collect();
    block(
        Some(format!("{} items", planned.len())),
        kit::list_rows(rows, ctx.t),
    )
}

/// How many sessions the week holds, done and still planned.
fn summary(ctx: &Ctx) -> Option<Block> {
    let range = week_focus(ctx.focus);
    let days: Vec<NaiveDate> = (0..7).map(|i| range.start + Duration::days(i)).collect();
    let done: usize = days.iter().map(|d| sessions_on(ctx, &key(*d)).len()).sum();
    let ahead = ctx.extras_view.map_or(0, |e| {
        e.planned
            .iter()
            .filter(|p| days.iter().any(|d| key(*d) == p.item.date))
            .count()
    });
    if done == 0 && ahead == 0 {
        return None;
    }
    let cells = vec![
        cell("Sessions done", done.to_string()),
        cell("Planned ahead", ahead.to_string()),
    ];
    block(Some(range.label()), kit::stat_grid(cells, 2, ctx.t))
}

/// The workouts that the user saved in Garmin Connect.
fn saved(ctx: &Ctx) -> Option<Block> {
    let view = ctx.places.plan()?;
    if view.workouts.is_empty() {
        return None;
    }
    let columns = [
        kit::Column {
            title: "Workout".into(),
            numeric: false,
        },
        kit::Column {
            title: "Sport".into(),
            numeric: false,
        },
        kit::Column {
            title: "Length".into(),
            numeric: true,
        },
    ];
    let rows = view
        .workouts
        .iter()
        .map(|w| {
            vec![
                w.name.clone().into(),
                sport_text(&w.sport).into(),
                w.size.clone().unwrap_or_default().into(),
            ]
        })
        .collect();
    block(
        Some(format!("{} workouts", view.workouts.len())),
        kit::table(&columns, rows, ctx.t),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_week_runs_from_monday_to_sunday() {
        let wed = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        let w = week_focus(Focus::day(wed));
        assert_eq!(key(w.start), "2026-10-19");
        assert_eq!(key(w.end), "2026-10-25");
    }

    #[test]
    fn a_race_name_picks_a_prediction() {
        assert_eq!(distance_key("Valencia Half Marathon"), Some("Half"));
        assert_eq!(distance_key("City marathon"), Some("Marathon"));
        assert_eq!(distance_key("Park run"), None);
    }

    #[test]
    fn a_step_has_a_length_and_a_target() {
        let step = PlanStep {
            end_type: Some("distance".into()),
            end_value: Some(1000.0),
            zone: Some(4),
            repeat: 5,
            ..PlanStep::default()
        };
        assert_eq!(step_length(&step).as_deref(), Some("1 km"));
        assert_eq!(step_target(&step).as_deref(), Some("Z4"));
    }
}
