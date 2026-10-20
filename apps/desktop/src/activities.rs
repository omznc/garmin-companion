//! The activity store, the detail page of one session and the page of all
//! sessions. The `garmin-api` view model holds the figures and the sentences.
//! This file lays them out with the block kit. The blocks of the list are in
//! `crate::blocks::activities`.

use std::path::PathBuf;

use crate::scale::px;
use anyhow::{Context as _, Result};
use chrono::NaiveDate;
use garmin_api::activities::{
    ActivityDetailView, Chart, ExerciseSummary, LapRow, SetRow, StrengthBlock, ZoneBlock,
};
use garmin_api::ActivitiesView;
use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, Div, SharedString};

use crate::blocks::{self, Width};
use crate::charts::{self, AreaChart, MeterRow, Tone};
use crate::data::Source;
use crate::kit::{self, Cell, Column};
use crate::shell::{page_heading, Root};
use crate::theme::Theme;
use crate::ui::MONO;

/// The sessions that the store reads for the tables.
const ROWS: u32 = 400;
/// The rows on one page of all sessions.
const PAGE_ROWS: usize = 25;
const DASH: &str = "—";

/// Where the activity views come from. Both read a cache file through
/// `garmin-api`. A persona is a made-up cache that the app seeds in a temp folder.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
    today: NaiveDate,
}

impl Store {
    pub fn new(path: PathBuf, today: NaiveDate) -> Self {
        Self { path, today }
    }

    /// Seed a persona into a fresh cache file in the temp folder.
    pub fn persona(name: &str) -> Result<Self> {
        let persona = garmin_api::personas::Persona::from_name(name)
            .with_context(|| format!("no persona {name}"))?;
        // A counter keeps two stores in one process apart. Tests run in parallel
        // and one store removed the folder of the other.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("gc-desktop-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("cache.sqlite3");
        let db = garmin_api::open_db(&path)?;
        let day = garmin_api::personas::sample_day();
        garmin_api::personas::seed(&db, persona, day).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Self::new(path, day))
    }

    /// The store for a data source. A missing cache copy is an error, as in `data`.
    pub fn for_source(source: &Source) -> Result<Self> {
        Ok(match source {
            Source::Default => Self::new(
                garmin_core::db::default_path().context("could not locate a data directory")?,
                chrono::Local::now().date_naive(),
            ),
            Source::Persona(name) => Self::persona(name)?,
            Source::Db { path, date } => Self::new(
                path.clone(),
                date.unwrap_or_else(|| chrono::Local::now().date_naive()),
            ),
        })
    }

    /// The day the store reads for.
    #[allow(dead_code)] // The block workers read it through `Ctx::store`.
    pub fn today(&self) -> NaiveDate {
        self.today
    }

    /// The number of cached activities. `None` when the cache does not open.
    pub fn count(&self) -> Option<i64> {
        garmin_api::open_db(&self.path).ok()?.activity_count().ok()
    }

    /// Open the cache, for a read that no view has.
    pub fn open(&self) -> Result<garmin_core::Db> {
        Ok(garmin_api::open_db(&self.path)?)
    }

    pub fn list(&self) -> Result<ActivitiesView> {
        let db = garmin_api::open_db(&self.path)?;
        Ok(garmin_api::activities_view_rows(&db, self.today, 0, ROWS)?)
    }

    pub fn detail(&self, id: i64) -> Result<Option<ActivityDetailView>> {
        let db = garmin_api::open_db(&self.path)?;
        Ok(garmin_api::activity_detail_view(&db, id)?)
    }
}

/// The pages of the Activities entry. `List` is the block layout.
#[derive(Debug, Clone, Default)]
pub enum Page {
    #[default]
    List,
    Detail(Box<ActivityDetailView>),
    /// The table of every session. The number is the page, from 0.
    All(usize),
    /// The detail of one Insights item, by id. Insights uses it, not Activities.
    Insight(String),
    /// The id has no row in the cache.
    Missing,
}

/* ----------------------------------------------------------------- helpers --- */

fn col(title: &str, numeric: bool) -> Column {
    Column {
        title: title.to_string().into(),
        numeric,
    }
}

fn text(value: Option<&str>) -> SharedString {
    value.unwrap_or(DASH).to_string().into()
}

fn padded(body: impl IntoElement) -> Div {
    div().px(px(12.0)).py(px(12.0)).child(body)
}

fn note(words: &str, t: &Theme) -> Div {
    padded(words.to_string())
        .text_size(px(12.0))
        .text_color(t.mutd)
}

/// The crumb of a session: the name and the day.
pub fn crumb(v: &ActivityDetailView) -> String {
    match v.when.split(',').next().filter(|d| !d.is_empty()) {
        Some(day) => format!("{} · {day}", v.name),
        None => v.name.clone(),
    }
}

/// The line under the title: the sport, the day with the year and time, the duration.
fn lede(v: &ActivityDetailView) -> String {
    let time = v.when.split(", ").nth(1);
    let day = NaiveDate::parse_from_str(&v.date, "%Y-%m-%d")
        .ok()
        .map(|d| d.format("%a %-d %b %Y").to_string());
    let when = match (day, time) {
        (Some(d), Some(t)) => Some(format!("{d}, {t}")),
        (Some(d), None) => Some(d),
        _ => None,
    };
    let duration = ["Elapsed time", "Time"]
        .iter()
        .find_map(|label| v.stats.iter().find(|s| s.label == *label))
        .map(|s| s.value.clone());
    let mut parts = vec![v.sport_label.clone()];
    parts.extend(when);
    parts.extend(duration);
    parts.join(" · ")
}

/* ------------------------------------------------------------------ detail --- */

fn zone_rows(z: &ZoneBlock) -> Vec<MeterRow> {
    let most = z.rows.iter().map(|r| r.percent).fold(1.0, f64::max);
    z.rows
        .iter()
        .map(|r| MeterRow {
            name: format!("Z{}", r.zone).into(),
            fraction: (r.percent / most) as f32,
            value: r.time.clone().into(),
            share: format!("{:.0}%", r.percent).into(),
            tone: match r.zone {
                1 => Tone::Muted,
                4 | 5 => Tone::Warn,
                _ => Tone::Accent,
            },
        })
        .collect()
}

fn zones_card(z: &ZoneBlock, t: &Theme) -> Div {
    div()
        .child(padded(charts::meter_rows(zone_rows(z), t)))
        .child(note(&z.sentence, t).pt(px(0.0)))
}

/// Fill a gap with the last value, so the line stays whole.
fn solid(values: &[Option<f64>]) -> Vec<f32> {
    let first = values.iter().flatten().next().copied().unwrap_or(0.0);
    let mut last = first;
    values
        .iter()
        .map(|v| {
            last = v.unwrap_or(last);
            last as f32
        })
        .collect()
}

/// A line chart of one series, or `None` when it has too few points.
fn series_card(
    title: &'static str,
    unit: &str,
    values: &[Option<f64>],
    chart: &Chart,
    width: Width,
    t: &Theme,
) -> Option<(u8, Div)> {
    if values.iter().flatten().count() < 2 {
        return None;
    }
    let line = charts::area_chart(
        AreaChart {
            values: &solid(values),
            colour: t.acc,
            height: Some(110.0),
            head: None,
            head_colour: None,
            x_first: "0".into(),
            x_last: chart.span.clone().into(),
        },
        t,
    );
    let meta = format!("{unit} · {}", chart.span);
    Some((
        width.columns(),
        kit::card(title, Some(meta.into()), None, line, t),
    ))
}

fn chart_cards(c: &Chart, t: &Theme) -> Vec<(u8, Div)> {
    [
        series_card("Heart rate", "bpm", &c.hr, c, Width::TwoThirds, t),
        series_card("Pace", "min/km", &c.pace, c, Width::Half, t),
        series_card("Elevation", "m", &c.elevation, c, Width::Half, t),
        series_card("Cadence", "per minute", &c.cadence, c, Width::Half, t),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn laps_table(rows: &[LapRow], t: &Theme) -> Div {
    let columns = [
        col("Lap", false),
        col("Distance", true),
        col("Time", true),
        col("Pace", true),
        col("Avg HR", true),
    ];
    let lines = rows
        .iter()
        .map(|l| {
            vec![
                l.label.clone().into(),
                text(l.distance.as_deref()),
                text(l.time.as_deref()),
                text(l.pace.as_deref()),
                text(l.avg_hr.as_deref()),
            ]
        })
        .collect();
    kit::table(&columns, lines, t)
}

fn sets_table(rows: &[SetRow], t: &Theme) -> Div {
    let columns = [
        col("Set", false),
        col("Exercise", false),
        col("Reps", true),
        col("Weight", true),
        col("Time", true),
        col("Rest", true),
    ];
    let lines = rows
        .iter()
        .map(|s| {
            vec![
                s.label.clone().into(),
                text(s.exercise.as_deref()),
                text(s.reps.as_deref()),
                text(s.weight.as_deref()),
                text(s.work.as_deref()),
                text(s.rest.as_deref()),
            ]
        })
        .collect();
    kit::table(&columns, lines, t)
}

fn exercises_table(rows: &[ExerciseSummary], t: &Theme) -> Div {
    let columns = [
        col("Exercise", false),
        col("Sets", true),
        col("Reps", true),
        col("Volume", true),
        col("Top set", true),
    ];
    let kg = |v: Option<f64>| text(v.map(|v| format!("{v:.0} kg")).as_deref());
    let lines = rows
        .iter()
        .map(|e| {
            vec![
                e.name.clone().into(),
                e.sets.to_string().into(),
                e.reps.to_string().into(),
                kg(e.volume_kg),
                kg(e.top_kg),
            ]
        })
        .collect();
    kit::table(&columns, lines, t)
}

fn muscle_rows(muscles: &[(String, usize)]) -> Vec<MeterRow> {
    let most = muscles.iter().map(|m| m.1).max().unwrap_or(1).max(1);
    muscles
        .iter()
        .enumerate()
        .map(|(i, (group, n))| MeterRow {
            name: group.clone().into(),
            fraction: *n as f32 / most as f32,
            value: n.to_string().into(),
            share: "sets".into(),
            tone: if i == 0 { Tone::Accent } else { Tone::Muted },
        })
        .collect()
}

fn strength_cards(s: &StrengthBlock, t: &Theme) -> Vec<(u8, Div)> {
    let sets = div()
        .child(note(&s.sentence, t))
        .child(sets_table(&s.sets, t));
    let mut cards = vec![(12, kit::card("Sets", None, None, sets, t))];
    if !s.exercises.is_empty() {
        let table = exercises_table(&s.exercises, t);
        cards.push((8, kit::card("Exercises", None, None, table, t)));
    }
    if !s.muscles.is_empty() {
        let meter = padded(charts::meter_rows(muscle_rows(&s.muscles), t));
        cards.push((4, kit::card("Muscle groups", None, None, meter, t)));
    }
    cards
}

fn summary_card(v: &ActivityDetailView, t: &Theme) -> Option<(u8, Div)> {
    if v.stats.is_empty() {
        return None;
    }
    let cells = v
        .stats
        .iter()
        .map(|s| Cell {
            label: s.label.clone().into(),
            value: s.value.clone().into(),
            unit: None,
            sub: None,
            spark: None,
            big: false,
        })
        .collect();
    let grid = kit::stat_grid(cells, 4, t);
    Some((12, kit::card("Summary", None, None, grid, t)))
}

fn detail_cards(v: &ActivityDetailView, t: &Theme) -> Vec<(u8, Div)> {
    let mut cards: Vec<(u8, Div)> = summary_card(v, t).into_iter().collect();
    let charts = v
        .chart
        .as_ref()
        .map(|c| chart_cards(c, t))
        .unwrap_or_default();
    let mut charts = charts.into_iter();
    cards.extend(charts.next());
    if let Some(z) = &v.zones {
        cards.push((
            4,
            kit::card("Time in zones", None, None, zones_card(z, t), t),
        ));
    }
    cards.extend(charts);
    if !v.laps.is_empty() {
        let table = laps_table(&v.laps, t);
        cards.push((12, kit::card("Laps", None, None, table, t)));
    }
    if let Some(s) = &v.strength {
        cards.extend(strength_cards(s, t));
    }
    cards
}

pub fn detail(
    v: &ActivityDetailView,
    available: f32,
    narrow: bool,
    t: &Theme,
    _cx: &mut Context<Root>,
) -> AnyElement {
    div()
        .child(page_heading(v.name.clone(), Some(lede(v)), t))
        .child(blocks::lay_out(detail_cards(v, t), available, narrow))
        .into_any_element()
}

/* --------------------------------------------------------------------- all --- */

fn page_link(
    id: &'static str,
    label: &'static str,
    to: Option<usize>,
    t: &Theme,
    cx: &mut Context<Root>,
) -> AnyElement {
    let Some(to) = to else {
        return div().into_any_element();
    };
    kit::link(id, label, t)
        .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.show_all_activities(to, cx)))
        .into_any_element()
}

/// The table of every session, in pages of newest first.
pub fn all(
    list: &Result<ActivitiesView>,
    at: usize,
    available: f32,
    t: &Theme,
    cx: &mut Context<Root>,
) -> AnyElement {
    let rows = list.as_ref().map(|v| v.rows.as_slice()).unwrap_or_default();
    let pages = rows.len().div_ceil(PAGE_ROWS).max(1);
    let at = at.min(pages - 1);
    let shown: Vec<_> = rows.iter().skip(at * PAGE_ROWS).take(PAGE_ROWS).collect();
    let meta = format!(
        "{} - {} of {}",
        at * PAGE_ROWS + 1,
        at * PAGE_ROWS + shown.len(),
        rows.len()
    );
    let pager = div()
        .flex()
        .justify_between()
        .px(px(12.0))
        .py(px(10.0))
        .border_t_1()
        .border_color(t.line2)
        .font_family(MONO)
        .child(page_link("newer", "Newer", at.checked_sub(1), t, cx))
        .child(page_link(
            "older",
            "Older",
            (at + 1 < pages).then_some(at + 1),
            t,
            cx,
        ));
    let body = div()
        .child(blocks::activities::table(&shown, t))
        .child(pager);
    let card = kit::card("All sessions", Some(meta.into()), None, body, t);
    div()
        .child(page_heading(
            "Activities".into(),
            Some("Every session, newest first.".into()),
            t,
        ))
        .child(blocks::lay_out(vec![(12, card)], available, false))
        .into_any_element()
}

/// Said when the id has no row, for example after a cache reset.
pub fn missing(t: &Theme, _cx: &mut Context<Root>) -> AnyElement {
    padded("This session is not in the cache.")
        .text_color(t.mutd)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gap_takes_the_last_value() {
        assert_eq!(
            solid(&[None, Some(2.0), None, Some(4.0)]),
            vec![2.0, 2.0, 2.0, 4.0]
        );
    }

    #[test]
    fn a_persona_store_lists_and_opens_a_session() {
        let store = Store::persona("sample").unwrap();
        let list = store.list().unwrap();
        let id = list.rows[0].activity_id;
        assert!(list.total > 0);
        assert!(store.detail(id).unwrap().is_some());
        assert!(store.detail(1).unwrap().is_none());
        let _ = std::fs::remove_dir_all(store.path.parent().unwrap());
    }
}
