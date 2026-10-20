//! The Insights, Fitness and Reports screens, from `app/src/screens/Insights.tsx`,
//! `Fitness.tsx` and `Reports.tsx`.
//!
//! The API builds the figures and the sentences, and it decides which
//! sections a user sees. A user who does not run gets no running analysis,
//! because the API leaves it out. This file lays the views out.

use anyhow::{ensure, Context, Result};
use garmin_api::fitness::FitnessView;
use garmin_api::insights::{Figure, InsightItem, InsightsView};
use garmin_api::reports::ReportView;
use garmin_api::training_fixtures::TrainingViews;
use gpui::{div, prelude::*, px, relative, AnyElement, ClickEvent, Context as GpuiContext, Div};

use crate::body::{empty_state, head, row, section, tile, tiles};
use crate::data::{Metric, Source};
use crate::shell::Root;
use crate::theme::{Theme, FS_BASE, FS_CAPTION, FS_MD, FS_SMALL};
use crate::ui::{axis_labels, body, ch, eyebrow, line_chart, metric, underlined, LH_NORMAL, SANS};

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

/// Read only the report, for a click on another week.
pub fn load_report(source: &Source, offset: usize) -> Result<ReportView> {
    let report = |db: &garmin_core::Db, day| {
        garmin_api::reports::report_view(db, day, offset).context("cannot build the report")
    };
    match source {
        Source::Default => {
            let db = garmin_api::open_default_db().context("cannot open the cache")?;
            report(&db, chrono::Local::now().date_naive())
        }
        Source::Persona(name) => {
            let mut views = persona(name)?;
            let at = offset.min(views.reports.len().saturating_sub(1));
            Ok(views.reports.swap_remove(at))
        }
        Source::Db { path, date } => {
            ensure!(path.is_file(), "no cache copy at {}", path.display());
            let db = garmin_api::open_db(path).context("cannot open the cache copy")?;
            report(
                &db,
                date.unwrap_or_else(|| chrono::Local::now().date_naive()),
            )
        }
    }
}

/// The screen for a sidebar label, or `None` for a label that this file does not draw.
pub fn screen(
    label: &str,
    data: &Training,
    t: &Theme,
    cx: &mut GpuiContext<Root>,
) -> Option<AnyElement> {
    Some(match label {
        "Insights" => insights(&data.insights, t),
        "Reports" => report(&data.report, t, cx),
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

/// The serif sentence that a finding is.
fn claim(text: &str, t: &Theme) -> Div {
    div()
        .font_family(SANS)
        .text_size(px(27.0))
        .line_height(relative(1.3))
        .text_color(t.fg)
        .max_w(px(560.0))
        .child(text.to_string())
}

/* ---------------------------------------------------------------- insights --- */

fn insight_chart(item: &InsightItem, t: &Theme) -> Option<Div> {
    let first = item.series.iter().find(|s| !s.muted)?;
    let values: Vec<f32> = first.values.iter().flatten().map(|v| *v as f32).collect();
    (values.len() > 1).then(|| {
        let mut out = div()
            .flex_none()
            .w(px(240.0))
            .child(line_chart(&values, 70.0, 6.0, t));
        if item.ends.len() == 2 {
            out = out.child(axis_labels(&item.ends, t));
        }
        out
    })
}

fn insight(item: &InsightItem, t: &Theme) -> Div {
    let mut text = div().flex_1().min_w_0().child(
        body(item.detail.clone(), FS_MD, t.mutd)
            .line_height(relative(1.6))
            .max_w(ch(62.0, FS_MD)),
    );
    if !item.figures.is_empty() {
        text = text.child(div().mt(px(18.0)).child(figure_tiles(&item.figures, t)));
    }
    let mut pair = div()
        .flex()
        .flex_wrap()
        .items_end()
        .gap(px(28.0))
        .child(text);
    if let Some(chart) = insight_chart(item, t) {
        pair = pair.child(chart);
    }
    let mut out = div()
        .mb(px(52.0))
        .child(claim(&item.claim, t))
        .child(div().mt(px(14.0)).child(pair));
    if !item.rows.is_empty() {
        out = out.child(div().mt(px(20.0)).child(insight_rows(item, t)));
    }
    match &item.basis {
        Some(b) => out.child(body(b.clone(), FS_CAPTION, t.faint).mt(px(10.0))),
        None => out,
    }
}

fn insight_rows(item: &InsightItem, t: &Theme) -> Div {
    item.rows.iter().fold(div(), |list, r| {
        let colour = if r.accent { t.acc } else { t.fg };
        let key = match &r.note {
            Some(n) => format!("{} ({n})", r.label),
            None => r.label.clone(),
        };
        list.child(row(key, &[(r.value.clone(), colour)], t))
    })
}

fn insights(v: &InsightsView, t: &Theme) -> AnyElement {
    let mut page = div().child(head(v.eyebrow.clone(), "Insights", &v.lede, t));
    if let Some(e) = &v.empty {
        return page.child(empty_state(e, t)).into_any_element();
    }
    for s in v.sections.iter() {
        let items = s
            .items
            .iter()
            .fold(div(), |col, item| col.child(insight(item, t)));
        page = page.child(div().child(section(&s.title, t)).child(items));
    }
    page.into_any_element()
}

/* ----------------------------------------------------------------- reports --- */

fn week_link(
    label: &'static str,
    target: Option<usize>,
    t: &Theme,
    cx: &mut GpuiContext<Root>,
) -> Div {
    let Some(offset) = target else {
        return div();
    };
    div().child(
        underlined(("week", offset), label, FS_SMALL, t)
            .cursor_pointer()
            .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.show_report(offset, cx))),
    )
}

fn report_text(v: &ReportView, t: &Theme) -> Div {
    let paragraphs = v.paragraphs.iter().enumerate().fold(div(), |col, (i, p)| {
        col.child(
            body(p.clone(), FS_MD, if i == 0 { t.fg } else { t.mutd })
                .line_height(relative(1.7))
                .mb(px(14.0)),
        )
    });
    div()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .font_family(SANS)
                .text_size(px(26.0))
                .line_height(relative(1.3))
                .mb(px(14.0))
                .child(v.headline.clone()),
        )
        .child(paragraphs)
}

fn report_rail(v: &ReportView, t: &Theme) -> Div {
    v.figures.iter().fold(
        div()
            .flex_none()
            .w(px(180.0))
            .pl(px(28.0))
            .border_l_1()
            .border_color(t.line)
            .flex()
            .flex_col()
            .gap(px(24.0)),
        |col, f| col.child(metric(&metric_of(f), t)),
    )
}

fn report_sessions(v: &ReportView, t: &Theme) -> Div {
    v.sessions.iter().fold(div(), |list, s| {
        let name = if s.sport.is_empty() {
            s.name.clone()
        } else {
            format!("{} ({})", s.name, s.sport)
        };
        let mut cells = vec![(s.day.clone(), t.faint), (s.duration.clone(), t.mutd)];
        if !s.distance.is_empty() {
            cells.insert(1, (s.distance.clone(), t.fg));
        }
        list.child(row(name, &cells, t))
    })
}

fn report(v: &ReportView, t: &Theme, cx: &mut GpuiContext<Root>) -> AnyElement {
    let mut page = div().child(head(v.eyebrow.clone(), "The Weekly", &v.lede, t));
    if let Some(e) = &v.empty {
        return page.child(empty_state(e, t)).into_any_element();
    }
    let columns = div()
        .flex()
        .gap(px(40.0))
        .child(report_text(v, t))
        .child(report_rail(v, t));
    let values: Vec<f32> = v.daily.iter().map(|d| d.value as f32).collect();
    let labels: Vec<String> = v.daily.iter().map(|d| d.label.clone()).collect();
    let earlier = v.has_earlier.then_some(v.offset + 1);
    let later = v.offset.checked_sub(1).filter(|_| v.has_later);
    page = page
        .child(columns)
        .child(section(&v.daily_title, t))
        .child(line_chart(&values, 80.0, 6.0, t))
        .child(axis_labels(&labels, t))
        .child(section("Sessions", t))
        .child(report_sessions(v, t))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(24.0))
                .mt(px(36.0))
                .font_family(SANS)
                .text_size(px(FS_BASE))
                .line_height(relative(LH_NORMAL))
                .child(week_link("Earlier week", earlier, t, cx))
                .child(week_link("Later week", later, t, cx))
                .child(eyebrow("Built from the local cache", t)),
        );
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
