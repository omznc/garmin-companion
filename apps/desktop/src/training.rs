//! The Insights, Fitness and Reports screens, from `app/src/screens/Insights.tsx`,
//! `Fitness.tsx` and `Reports.tsx`.
//!
//! The API builds the figures and the sentences, and it decides which
//! sections a user sees. A user who does not run gets no running analysis,
//! because the API leaves it out. This file lays the views out.

use anyhow::{ensure, Context, Result};
use garmin_api::fitness::{Balance, FitnessView, LoadBar};
use garmin_api::insights::{Figure, InsightsView};
use garmin_api::reports::ReportView;
use garmin_api::training_fixtures::TrainingViews;
use gpui::{div, prelude::*, px, relative, AnyElement, Context as GpuiContext, Div};

use crate::body::{empty_state, head, note, row, section, tile, tiles};
use crate::data::{Metric, Source};
use crate::shell::Root;
use crate::theme::{Theme, FS_CAPTION};
use crate::ui::{MONO, SANS};

/// The made-up users of `garmin-api`, as the three training views.
const PERSONAS: [(&str, &str); 5] = [
    (
        "sample",
        include_str!("../../../crates/garmin-api/fixtures/training.sample.json"),
    ),
    (
        "golfer",
        include_str!("../../../crates/garmin-api/fixtures/training.golfer.json"),
    ),
    (
        "gym",
        include_str!("../../../crates/garmin-api/fixtures/training.gym.json"),
    ),
    (
        "health_only",
        include_str!("../../../crates/garmin-api/fixtures/training.health_only.json"),
    ),
    (
        "new_user",
        include_str!("../../../crates/garmin-api/fixtures/training.new_user.json"),
    ),
];

pub struct Training {
    pub insights: InsightsView,
    pub fitness: FitnessView,
    /// The week on show. Earlier and Later load another one.
    pub report: ReportView,
}

fn persona(name: &str) -> Result<TrainingViews> {
    let (_, json) = PERSONAS
        .iter()
        .find(|(n, _)| *n == name)
        .with_context(|| format!("no persona {name}"))?;
    serde_json::from_str(json).context("the training file does not parse")
}

fn from_db(db: &garmin_core::Db, day: chrono::NaiveDate, offset: usize) -> Result<Training> {
    Ok(Training {
        insights: garmin_api::insights::insights_view(db, day).context("cannot build Insights")?,
        fitness: garmin_api::fitness::fitness_view(db, day).context("cannot build Fitness")?,
        report: garmin_api::reports::report_view(db, day, offset)
            .context("cannot build the report")?,
    })
}

/// Read the three views from the same source as the Today screen.
pub fn load(source: &Source, offset: usize) -> Result<Training> {
    match source {
        Source::Default => {
            let db = garmin_api::open_default_db().context("cannot open the cache")?;
            from_db(&db, chrono::Local::now().date_naive(), offset)
        }
        Source::Persona(name) => {
            let mut views = persona(name)?;
            let at = offset.min(views.reports.len().saturating_sub(1));
            Ok(Training {
                insights: views.insights,
                fitness: views.fitness,
                report: views.reports.swap_remove(at),
            })
        }
        Source::Db { path, date } => {
            ensure!(path.is_file(), "no cache copy at {}", path.display());
            let db = garmin_api::open_db(path).context("cannot open the cache copy")?;
            from_db(
                &db,
                date.unwrap_or_else(|| chrono::Local::now().date_naive()),
                offset,
            )
        }
    }
}

/// The screen for a sidebar label, or `None` for a label that this file does not draw.
pub fn screen(
    label: &str,
    data: &Training,
    t: &Theme,
    _cx: &mut GpuiContext<Root>,
) -> Option<AnyElement> {
    Some(match label {
        "Fitness" => fitness(&data.fitness, t),
        _ => return None,
    })
}

fn metric_of(f: &Figure) -> Metric {
    tile(&f.label, Some(f.value.clone()), "", f.accent)
}

fn figure_tiles(figures: &[Figure], t: &Theme) -> Div {
    let list: Vec<Metric> = figures.iter().map(metric_of).collect();
    tiles(&list, t)
}

/* ----------------------------------------------------------------- fitness --- */

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
        .font_family(SANS)
        .text_size(px(FS_CAPTION))
        .child(div().text_color(t.mutd).child(bar.label.clone()))
        .child(
            div()
                .font_family(MONO)
                .text_color(if bar.state == "over" { t.warn } else { t.mutd })
                .child(bar.text.clone()),
        );
    div().mb(px(18.0)).child(caption).child(track)
}

fn balance(b: &Balance, t: &Theme) -> Div {
    let bars = b
        .bars
        .iter()
        .fold(div().mt(px(18.0)), |col, bar| col.child(load_bar(bar, t)));
    div()
        .child(section("Load balance, this month", t))
        .child(note(&b.intro, t))
        .child(bars)
}

fn fitness(v: &FitnessView, t: &Theme) -> AnyElement {
    let lede = v.lede.as_deref().unwrap_or("");
    let mut page = div().child(head("Fitness".into(), "What Garmin makes of it", lede, t));
    if let Some(e) = &v.empty {
        return page.child(empty_state(e, t)).into_any_element();
    }
    if !v.figures.is_empty() {
        page = page.child(figure_tiles(&v.figures, t));
    }
    for n in &v.notes {
        page = page.child(note(n, t));
    }
    if let Some(b) = &v.balance {
        page = page.child(balance(b, t));
    }
    if let Some(p) = &v.predictions {
        page = page.child(
            div()
                .child(section("Race predictions", t))
                .child(note(&p.intro, t))
                .child(div().mt(px(18.0)).child(figure_tiles(&p.rows, t))),
        );
    }
    if !v.records.is_empty() {
        let list = v.records.iter().fold(div(), |col, r| {
            col.child(row(
                r.label.clone(),
                &[(r.value.clone(), t.fg), (r.date.clone(), t.faint)],
                t,
            ))
        });
        page = page.child(div().child(section("Records", t)).child(list));
    }
    page.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_persona_parses_and_the_runner_has_weeks() {
        for (name, _) in PERSONAS {
            let t = load(&Source::Persona(name.into()), 0).unwrap();
            assert_eq!(t.report.offset, 0, "{name}");
        }
        let t = load(&Source::Persona("sample".into()), 3).unwrap();
        assert_eq!(t.report.offset, 3);
        let past = load(&Source::Persona("sample".into()), 999).unwrap();
        assert_eq!(past.report.offset, past.report.weeks - 1);
    }
}
