//! The Weight, Food and Strength screens, from `app/src/screens/Weight.tsx`,
//! `Food.tsx` and `Strength.tsx`.
//!
//! The API builds the numbers and the sentences. This file lays them out.
//! The screens only read. They log nothing, as the current app does not.

use anyhow::{ensure, Context, Result};
use garmin_api::body::{BodyViews, EmptyNote, ExerciseHistory, FoodView, StrengthView, WeightView};
use gpui::{div, prelude::*, px, relative, AnyElement, Div, Hsla};

use crate::data::{Metric, Part, Source};
use crate::theme::{Theme, FS_BASE, FS_CAPTION, FS_LG, FS_MD, FS_SMALL};
use crate::ui::{
    axis_labels, block_in, body, ch, eyebrow, eyebrow_lg, h1, line_chart, metric, small_stat,
    spark, LH_NORMAL, MONO, SANS,
};

const SAMPLE: &str = include_str!("../../../crates/garmin-api/fixtures/body.sample.json");
const EMPTY: &str = include_str!("../../../crates/garmin-api/fixtures/body.empty.json");
const DASH: &str = "—";

/// The three views. Both sources fill all of them.
pub struct BodyData(pub BodyViews);

/// Made-up personas that show a body log use the sample. The others have none.
pub fn load_body(source: &Source) -> Result<BodyData> {
    match source {
        Source::Default => {
            let db = garmin_api::open_default_db().context("cannot open the cache")?;
            let today = chrono::Local::now().date_naive();
            Ok(BodyData(
                garmin_api::body::body_views(&db, today).context("cannot build the body views")?,
            ))
        }
        Source::Persona(name) => {
            let json = if matches!(name.as_str(), "sample" | "gym") {
                SAMPLE
            } else {
                EMPTY
            };
            Ok(BodyData(
                serde_json::from_str(json).context("the body fixture does not parse")?,
            ))
        }
        Source::Db { path, date } => {
            ensure!(path.is_file(), "no cache copy at {}", path.display());
            let db = garmin_api::open_db(path).context("cannot open the cache copy")?;
            let today = date.unwrap_or_else(|| chrono::Local::now().date_naive());
            Ok(BodyData(
                garmin_api::body::body_views(&db, today).context("cannot build the body views")?,
            ))
        }
    }
}

/// The screen for a sidebar label, or `None` for a label that this file does not draw.
pub fn screen(label: &str, data: &BodyData, key: usize, t: &Theme) -> Option<AnyElement> {
    let v = &data.0;
    Some(match label {
        "Weight" => weight(&v.weight, key, t),
        "Food" => food(&v.food, key, t),
        "Strength" => strength(&v.strength, key, t),
        _ => return None,
    })
}

pub(crate) fn tile(label: &str, value: Option<String>, unit: &str, accent: bool) -> Metric {
    let mut parts = vec![Part {
        text: value.clone().unwrap_or_else(|| DASH.into()),
        unit: false,
    }];
    if value.is_some() && !unit.is_empty() {
        parts.push(Part {
            text: format!(" {unit}"),
            unit: true,
        });
    }
    Metric {
        label: label.into(),
        value: parts,
        accent,
    }
}

pub(crate) fn tiles(items: &[Metric], t: &Theme) -> Div {
    items.iter().fold(
        div().flex().flex_wrap().gap_x(px(44.0)).gap_y(px(24.0)),
        |row, m| row.child(metric(m, t)),
    )
}

pub(crate) fn head(eyebrow_text: String, title: &str, lede: &str, key: usize, t: &Theme) -> Div {
    div()
        .mb(px(28.0))
        .child(block_in(key, 0, eyebrow_lg(eyebrow_text, t)))
        .child(block_in(key, 0, h1(title.to_string()).mt(px(18.0))))
        .children((!lede.is_empty()).then(|| {
            block_in(
                key,
                1,
                body(lede.to_string(), FS_LG, t.mutd)
                    .mt(px(20.0))
                    .max_w(ch(58.0, FS_LG)),
            )
        }))
}

pub(crate) fn section(text: &str, t: &Theme) -> Div {
    div()
        .mt(px(64.0))
        .mb(px(16.0))
        .child(eyebrow(text.to_string(), t))
}

pub(crate) fn note(text: &str, t: &Theme) -> Div {
    body(text.to_string(), FS_SMALL, t.faint)
        .mt(px(12.0))
        .max_w(ch(62.0, FS_SMALL))
}

/// The state for a screen with no data. It says why and what to do.
pub(crate) fn empty_state(e: &EmptyNote, t: &Theme) -> Div {
    div().child(body(e.title.clone(), FS_LG, t.fg)).child(
        body(e.body.clone(), FS_MD, t.mutd)
            .mt(px(14.0))
            .max_w(ch(62.0, FS_MD)),
    )
}

/// One list row: a key at the left and figures at the right.
pub(crate) fn row(key: String, figures: &[(String, Hsla)], t: &Theme) -> Div {
    let cells = figures.iter().fold(
        div()
            .flex()
            .gap(px(20.0))
            .font_family(MONO)
            .text_size(px(FS_CAPTION)),
        |r, (text, colour)| r.child(div().text_color(*colour).child(text.clone())),
    );
    div()
        .flex()
        .items_baseline()
        .justify_between()
        .gap(px(16.0))
        .py(px(11.0))
        .border_b_1()
        .border_color(t.line2)
        .font_family(SANS)
        .text_size(px(FS_BASE))
        .line_height(relative(LH_NORMAL))
        .child(div().child(key))
        .child(cells)
}

/// `28 Sep` from `2026-09-28`. A date that does not parse stays as it is.
fn short_day(date: &str) -> String {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_or_else(|_| date.to_string(), |d| d.format("%d %b").to_string())
}

fn kcal(v: Option<f64>) -> String {
    v.map_or(DASH.into(), |v| group(v.round()))
}

/// `1,980`, with a comma between thousands. The sign stays.
fn group(v: f64) -> String {
    let digits = format!("{:.0}", v.abs());
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if v < -0.5 {
        format!("-{out}")
    } else {
        out
    }
}

fn balance(v: Option<f64>) -> String {
    match v.map(f64::round) {
        Some(b) if b > 0.0 => format!("+{}", group(b)),
        Some(b) => group(b),
        None => DASH.into(),
    }
}

fn kg(v: Option<f64>) -> Option<String> {
    v.map(|v| format!("{v:.1}"))
}

fn signed_kg(v: Option<f64>) -> Option<String> {
    v.map(|v| format!("{}{v:.1}", if v > 0.0 { "+" } else { "" }))
}

/// A chart and the two dates under it.
pub(crate) fn chart(values: &[f32], labels: [&str; 2], t: &Theme) -> Div {
    div()
        .child(line_chart(values, 140.0, 8.0, t))
        .child(axis_labels(&labels.map(String::from), t))
}

/* ------------------------------------------------------------------ weight --- */

fn weight(v: &WeightView, key: usize, t: &Theme) -> AnyElement {
    if let Some(e) = &v.empty {
        return div()
            .child(head("From Garmin".into(), "Weight", "", key, t))
            .child(block_in(key, 1, empty_state(e, t)))
            .into_any_element();
    }
    let eyebrow_text = format!(
        "Last 180 days · {} {}",
        v.count,
        if v.count == 1 {
            "weigh-in"
        } else {
            "weigh-ins"
        }
    );
    let numbers = [
        tile("Trend", kg(v.trend_kg), "kg", false),
        tile("Latest", kg(v.latest_kg), "kg", false),
        tile("Change", signed_kg(v.change_kg), "kg", false),
        tile("BMI", kg(v.bmi), "", false),
    ];
    let shown: Vec<Metric> = numbers
        .into_iter()
        .filter(|m| m.value[0].text != DASH)
        .collect();
    let mut page = div()
        .child(head(
            eyebrow_text,
            "Weight",
            v.sentence.as_deref().unwrap_or(""),
            key,
            t,
        ))
        .child(block_in(key, 2, tiles(&shown, t)));
    for w in &v.windows {
        page = page.child(note(&w.sentence, t));
    }
    page = page.child(weight_chart(v, t));
    if let Some(c) = &v.composition {
        page = page.child(composition(c, t));
    }
    if let Some(g) = &v.goal_sentence {
        page = page
            .child(section("Goal", t))
            .child(body(g.clone(), FS_MD, t.fg));
    }
    page.child(section("By day", t))
        .child(weight_rows(v, t))
        .into_any_element()
}

fn weight_chart(v: &WeightView, t: &Theme) -> Div {
    let line: Vec<f32> = v
        .points
        .iter()
        .map(|p| p.trend_kg.unwrap_or(p.kg) as f32)
        .collect();
    let (Some(first), Some(last)) = (v.points.first(), v.points.last()) else {
        return div();
    };
    div()
        .child(section("Trend", t))
        .child(chart(&line, [&first.label, &last.label], t))
}

fn composition(c: &garmin_api::body::Composition, t: &Theme) -> Div {
    let mut stats = Vec::new();
    let mut add = |label: &str, value: Option<f64>, unit: &str| {
        if let Some(v) = value {
            stats.push(small_stat(label, &format!("{v:.1}{unit}"), t));
        }
    };
    add("Body fat", c.body_fat_pct, "%");
    add("Body water", c.body_water_pct, "%");
    add("Muscle", c.muscle_kg, " kg");
    add("Bone", c.bone_kg, " kg");
    div()
        .child(section("Body composition", t))
        .child(stats.into_iter().fold(
            div().flex().flex_wrap().gap_x(px(38.0)).gap_y(px(20.0)),
            |r, s| r.child(s),
        ))
        .child(note(
            &format!("From the scale reading of {}.", short_day(&c.date)),
            t,
        ))
}

fn weight_rows(v: &WeightView, t: &Theme) -> Div {
    v.points.iter().rev().take(60).fold(div(), |list, p| {
        let trend = p.trend_kg.map_or(DASH.into(), |x| format!("{x:.1}"));
        let flag = if p.outlier { "check this" } else { "" };
        let colour = if p.outlier { t.acc } else { t.faint };
        list.child(row(
            p.label.clone(),
            &[
                (flag.into(), colour),
                (trend, t.faint),
                (format!("{:.1} kg", p.kg), t.fg),
            ],
            t,
        ))
    })
}

/* -------------------------------------------------------------------- food --- */

fn food(v: &FoodView, key: usize, t: &Theme) -> AnyElement {
    let lede = "Intake against expenditure, matched to what Garmin says you burned.";
    let mut page = div().child(head("Last 30 days".into(), "Food", lede, key, t));
    match &v.empty {
        Some(e) => page = page.child(block_in(key, 2, empty_state(e, t))),
        None => {
            page = page
                .child(block_in(key, 2, tiles(&food_tiles(v), t)))
                .child(note(&v.sentence, t))
                .child(food_chart(v, t));
        }
    }
    if let Some(w) = &v.water {
        page = page
            .child(section("Water", t))
            .child(body(w.sentence.clone(), FS_MD, t.fg).max_w(ch(62.0, FS_MD)));
    }
    page.child(section("By day", t))
        .child(food_rows(v, t))
        .into_any_element()
}

fn food_tiles(v: &FoodView) -> [Metric; 3] {
    let whole = |x: Option<f64>| x.map(|x| group(x.round()));
    [
        tile("Avg eaten", whole(v.avg_eaten_kcal), "", false),
        tile("Avg burned", whole(v.avg_burned_kcal), "", false),
        tile(
            "Avg balance",
            v.avg_balance_kcal.map(|b| balance(Some(b))),
            "",
            v.avg_balance_kcal.unwrap_or(0.0) > 0.0,
        ),
    ]
}

/// Eaten kcal, oldest first. A day with no log is skipped, not drawn as zero.
fn food_chart(v: &FoodView, t: &Theme) -> Div {
    let logged: Vec<_> = v.days.iter().rev().filter(|d| d.logged).collect();
    let (Some(first), Some(last)) = (logged.first(), logged.last()) else {
        return div();
    };
    let values: Vec<f32> = logged
        .iter()
        .filter_map(|d| d.eaten_kcal)
        .map(|k| k as f32)
        .collect();
    // The goal is a line only when Garmin has one. An unset goal draws nothing.
    div()
        .child(section("Eaten", t))
        .child(chart(&values, [&first.label, &last.label], t))
}

fn food_rows(v: &FoodView, t: &Theme) -> Div {
    v.days.iter().fold(div(), |list, d| {
        let water = d
            .water_ml
            .map_or(DASH.into(), |ml| format!("{:.2} L", ml / 1000.0));
        let figures = if d.logged {
            vec![
                (kcal(d.eaten_kcal), t.fg),
                (kcal(d.burned_kcal), t.mutd),
                (balance(d.balance_kcal), t.mutd),
                (water, t.faint),
            ]
        } else {
            vec![
                ("no food logged".into(), t.faint),
                (kcal(d.burned_kcal), t.faint),
                (water, t.faint),
            ]
        };
        list.child(row(d.label.clone(), &figures, t))
    })
}

/* ---------------------------------------------------------------- strength --- */

fn strength(v: &StrengthView, key: usize, t: &Theme) -> AnyElement {
    if let Some(e) = &v.empty {
        return div()
            .child(head("Strength".into(), "Lifting", "", key, t))
            .child(block_in(key, 1, empty_state(e, t)))
            .into_any_element();
    }
    let pct = v.identified_pct.map(|p| p.to_string());
    let stats = [
        tile(
            "Sets per session",
            v.avg_sets.map(|x| format!("{x:.0}")),
            "",
            false,
        ),
        tile(
            "Reps per session",
            v.avg_reps.map(|x| format!("{x:.0}")),
            "",
            false,
        ),
        tile(
            "Typical rest",
            v.median_rest_s.map(garmin_api::body::mmss),
            "min",
            false,
        ),
        tile("Sets identified", pct, "%", false),
    ];
    let mut page = div()
        .child(head("Strength".into(), "Lifting", &v.sentence, key, t))
        .child(block_in(key, 2, tiles(&stats, t)))
        .child(note(&v.caveat, t));
    if !v.exercises.is_empty() {
        page = page.child(section("By movement", t));
        for e in &v.exercises {
            page = page.child(exercise_row(e, t));
        }
    }
    page.child(section("Sessions", t))
        .child(session_rows(v, t))
        .into_any_element()
}

fn exercise_row(e: &ExerciseHistory, t: &Theme) -> Div {
    let reps: Vec<Option<f32>> = e.history.iter().map(|d| Some(d.reps as f32)).collect();
    row(
        e.name.clone(),
        &[
            (e.detail.clone(), t.faint),
            (
                format!("{} {}", e.reps, if e.reps == 1 { "rep" } else { "reps" }),
                t.fg,
            ),
        ],
        t,
    )
    .child(div().child(spark(&reps, t)))
}

fn session_rows(v: &StrengthView, t: &Theme) -> Div {
    v.sessions.iter().fold(div(), |list, s| {
        let line = row(s.label.clone(), &[(s.summary.clone(), t.mutd)], t);
        match &s.movements {
            Some(m) => list
                .child(line)
                .child(body(m.clone(), FS_CAPTION, t.faint).pt(px(4.0))),
            None => list.child(line),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_fixtures_parse_and_the_empty_one_says_why() {
        let sample: BodyViews = serde_json::from_str(SAMPLE).unwrap();
        let none: BodyViews = serde_json::from_str(EMPTY).unwrap();
        assert!(sample.weight.empty.is_none() && sample.strength.shown);
        assert!(none.weight.empty.is_some() && none.food.empty.is_some());
        assert!(!none.strength.shown);
    }
}
