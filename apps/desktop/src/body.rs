//! The Weight, Food and Strength screens, from `app/src/screens/Weight.tsx`,
//! `Food.tsx` and `Strength.tsx`.
//!
//! The API builds the numbers and the sentences. This file lays them out.
//! The screens only read. They log nothing, as the current app does not.

use anyhow::{ensure, Context, Result};
use garmin_api::body::{BodyViews, EmptyNote, ExerciseHistory, StrengthView};
use gpui::{div, prelude::*, px, relative, AnyElement, Div, Hsla};

use crate::data::{Metric, Part, Source};
use crate::theme::{Theme, FS_BASE, FS_CAPTION, FS_LG, FS_MD, FS_SMALL};
use crate::ui::{body, ch, eyebrow, eyebrow_lg, h1, metric, spark, LH_NORMAL, MONO, SANS};

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
pub fn screen(label: &str, data: &BodyData, t: &Theme) -> Option<AnyElement> {
    let v = &data.0;
    Some(match label {
        "Strength" => strength(&v.strength, t),
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

pub(crate) fn head(eyebrow_text: String, title: &str, lede: &str, t: &Theme) -> Div {
    div()
        .mb(px(28.0))
        .child(eyebrow_lg(eyebrow_text, t))
        .child(h1(title.to_string()).mt(px(18.0)))
        .children((!lede.is_empty()).then(|| {
            body(lede.to_string(), FS_LG, t.mutd)
                .mt(px(20.0))
                .max_w(ch(58.0, FS_LG))
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

/* ---------------------------------------------------------------- strength --- */

fn strength(v: &StrengthView, t: &Theme) -> AnyElement {
    if let Some(e) = &v.empty {
        return div()
            .child(head("Strength".into(), "Lifting", "", t))
            .child(empty_state(e, t))
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
        .child(head("Strength".into(), "Lifting", &v.sentence, t))
        .child(tiles(&stats, t))
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
        &[(e.detail.clone(), t.faint), (e.reps_text.clone(), t.fg)],
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
