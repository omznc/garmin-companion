//! The Activities list and the Activity detail screen.
//!
//! From `app/src/screens/Activities.tsx` and `ActivityDetail.tsx`. The
//! `garmin-api` view model holds the figures and the sentences. This file only
//! lays them out. The map and the per-activity chat are not here yet.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use chrono::NaiveDate;
use garmin_api::activities::{
    ActivityCard, ActivityDetailView, Chart, DayGroup, LapRow, SetRow, StrengthBlock, ZoneBlock,
};
use garmin_api::ActivitiesView;
use gpui::{
    div, prelude::*, px, relative, AnyElement, ClickEvent, ColorExt as _, Context, Div, Hsla,
};

use crate::data::Source;
use crate::shell::Root;
use crate::theme::{Theme, FS_BASE, FS_CAPTION, FS_LG, FS_MICRO, FS_SMALL};
use crate::ui::{block_in, body, ch, eyebrow, eyebrow_lg, h1, line_chart, small_stat, MONO, SANS};

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
        let dir = std::env::temp_dir().join(format!("gc-desktop-{name}-{}", std::process::id()));
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

    pub fn list(&self) -> Result<ActivitiesView> {
        let db = garmin_api::open_db(&self.path)?;
        Ok(garmin_api::activities_view(&db, self.today, 60)?)
    }

    pub fn detail(&self, id: i64) -> Result<Option<ActivityDetailView>> {
        let db = garmin_api::open_db(&self.path)?;
        Ok(garmin_api::activity_detail_view(&db, id)?)
    }
}

/// The two screens of the Activities entry.
#[derive(Debug, Clone, Default)]
pub enum Page {
    #[default]
    List,
    Detail(Box<ActivityDetailView>),
    /// The id has no row in the cache.
    Missing,
}

/* ------------------------------------------------------------------- list --- */

fn row(card: &ActivityCard, t: &Theme, cx: &mut Context<Root>) -> impl IntoElement {
    let id = card.activity_id;
    let sel = t.sel;
    div()
        .id(("activity", id as usize))
        .flex()
        .items_baseline()
        .gap(px(16.0))
        .py(px(13.0))
        .border_b_1()
        .border_color(t.line2)
        .cursor_pointer()
        .hover(move |s| s.bg(sel))
        .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.open_activity(id, cx)))
        .child(
            div()
                .flex_none()
                .w(px(50.0))
                .font_family(SANS)
                .text_size(px(FS_CAPTION))
                .text_color(t.faint)
                .child(card.time.clone().unwrap_or_default()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(body(card.name.clone(), FS_BASE, t.fg))
                .child(body(card.sport_label.clone(), FS_SMALL, t.faint)),
        )
        .child(
            div()
                .flex_none()
                .font_family(MONO)
                .text_size(px(FS_BASE))
                .text_color(t.fg)
                .child(card.headline.clone()),
        )
        .child(
            div()
                .flex_none()
                .w(px(230.0))
                .whitespace_nowrap()
                .text_size(px(FS_SMALL))
                .text_color(t.mutd)
                .child(card.detail.clone()),
        )
}

fn group(g: &DayGroup, t: &Theme, cx: &mut Context<Root>) -> Div {
    let rows = g
        .sessions
        .iter()
        .fold(div(), |col, c| col.child(row(c, t, cx)));
    div()
        .child(
            div()
                .mt(px(26.0))
                .mb(px(4.0))
                .child(eyebrow(g.label.clone(), t)),
        )
        .child(rows)
}

fn empty_note(t: &Theme) -> Div {
    body(
        "No sessions yet. When your watch records a session, it shows here.",
        16.0,
        t.mutd,
    )
    .mt(px(28.0))
    .max_w(ch(62.0, 16.0))
}

pub fn list(view: &ActivitiesView, key: usize, t: &Theme, cx: &mut Context<Root>) -> AnyElement {
    let head = div()
        .child(block_in(key, 0, eyebrow_lg("Activities", t)))
        .child(block_in(key, 0, h1("Every session").mt(px(18.0))))
        .child(block_in(
            key,
            1,
            body("Newest first.", 16.0, t.mutd).mt(px(14.0)),
        ));
    if view.empty {
        return head.child(empty_note(t)).into_any_element();
    }
    let groups = view
        .groups
        .iter()
        .fold(div(), |col, g| col.child(group(g, t, cx)));
    head.child(block_in(key, 2, groups)).into_any_element()
}

/* ----------------------------------------------------------------- detail --- */

fn zone_fill(t: &Theme, zone: usize) -> Hsla {
    match zone {
        0 => t.line,
        1 => t.mutd,
        2 => t.acc.opacity(0.55),
        3 => t.acc.opacity(0.78),
        _ => t.acc,
    }
}

fn section_head(text: &str, t: &Theme) -> Div {
    div()
        .mt(px(56.0))
        .mb(px(16.0))
        .child(eyebrow(text.to_string(), t))
}

fn zones(z: &ZoneBlock, t: &Theme) -> Div {
    let bar = z.rows.iter().filter(|r| r.percent > 0.0).fold(
        div().flex().h(px(10.0)).overflow_hidden().rounded(px(2.0)),
        |bar, r| {
            let i = usize::from(r.zone) - 1;
            bar.child(
                div()
                    .h_full()
                    .w(relative(r.percent as f32 / 100.0))
                    .bg(zone_fill(t, i)),
            )
        },
    );
    let legend = z.rows.iter().fold(
        div()
            .flex()
            .flex_wrap()
            .gap(px(16.0))
            .mt(px(9.0))
            .font_family(SANS)
            .text_size(px(FS_CAPTION))
            .text_color(t.mutd),
        |row, r| {
            row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .size(px(6.0))
                            .rounded(px(1.0))
                            .bg(zone_fill(t, usize::from(r.zone) - 1)),
                    )
                    .child(format!("Z{} {}", r.zone, r.time)),
            )
        },
    );
    div()
        .child(section_head("Heart rate zones", t))
        .child(bar)
        .child(legend)
        .child(body(z.sentence.clone(), FS_BASE, t.mutd).mt(px(14.0)))
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

fn chart(c: &Chart, t: &Theme) -> Div {
    let mut out =
        div()
            .child(section_head("Heart rate", t))
            .child(line_chart(&solid(&c.hr), 110.0, 6.0, t));
    if !c.elevation.is_empty() {
        out = out.child(section_head("Elevation", t)).child(line_chart(
            &solid(&c.elevation),
            70.0,
            6.0,
            t,
        ));
    }
    out.child(
        div()
            .mt(px(9.0))
            .flex()
            .justify_between()
            .text_size(px(FS_MICRO))
            .text_color(t.faint)
            .child("0")
            .child(c.span.clone()),
    )
}

fn cell(text: &str, width: f32, colour: Hsla, mono: bool) -> Div {
    let d = div()
        .flex_none()
        .w(px(width))
        .text_size(px(FS_SMALL))
        .text_color(colour)
        .child(text.to_string());
    if mono {
        d.font_family(MONO)
    } else {
        d
    }
}

fn table_row(cells: [(&str, f32, bool); 5], t: &Theme) -> Div {
    cells.into_iter().fold(
        div()
            .flex()
            .gap(px(12.0))
            .py(px(9.0))
            .border_b_1()
            .border_color(t.line2),
        |row, (text, w, strong)| {
            row.child(cell(text, w, if strong { t.fg } else { t.mutd }, strong))
        },
    )
}

fn laps(rows: &[LapRow], t: &Theme) -> Div {
    let list = rows.iter().fold(div(), |col, l| {
        let none = "-";
        col.child(table_row(
            [
                (&l.label, 70.0, false),
                (l.distance.as_deref().unwrap_or(none), 80.0, true),
                (l.time.as_deref().unwrap_or(none), 80.0, true),
                (l.pace.as_deref().unwrap_or(none), 100.0, true),
                (l.avg_hr.as_deref().unwrap_or(none), 50.0, false),
            ],
            t,
        ))
    });
    div().child(section_head("Laps", t)).child(list)
}

fn set_row(s: &SetRow, t: &Theme) -> Div {
    let none = "-";
    let name = s.exercise.clone().unwrap_or_default();
    table_row(
        [
            (&s.label, 60.0, false),
            (
                &format!("{} reps", s.reps.as_deref().unwrap_or(none)),
                80.0,
                true,
            ),
            (s.work.as_deref().unwrap_or(none), 60.0, true),
            (
                &format!("rest {}", s.rest.as_deref().unwrap_or(none)),
                90.0,
                false,
            ),
            (&name, 190.0, false),
        ],
        t,
    )
}

fn strength(s: &StrengthBlock, t: &Theme) -> Div {
    let rows = s.sets.iter().fold(div(), |col, r| col.child(set_row(r, t)));
    div()
        .child(section_head("Sets", t))
        .child(body(s.sentence.clone(), FS_BASE, t.mutd).mb(px(12.0)))
        .child(rows)
}

fn back(t: &Theme, cx: &mut Context<Root>) -> impl IntoElement {
    let fg = t.fg;
    div()
        .id("back")
        .flex_none()
        .font_family(SANS)
        .text_size(px(FS_SMALL))
        .text_color(t.mutd)
        .cursor_pointer()
        .hover(move |s| s.text_color(fg))
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.close_activity(cx)))
        .child("< All activities")
}

pub fn detail(v: &ActivityDetailView, key: usize, t: &Theme, cx: &mut Context<Root>) -> AnyElement {
    let stats = v.stats.iter().fold(
        div()
            .flex()
            .flex_wrap()
            .gap_x(px(38.0))
            .gap_y(px(20.0))
            .mt(px(28.0)),
        |row, s| row.child(small_stat(&s.label, &s.value, t)),
    );
    let mut page = div()
        .child(block_in(key, 0, back(t, cx)))
        .child(block_in(
            key,
            0,
            eyebrow_lg(format!("{} - {}", v.sport_label, v.when), t).mt(px(26.0)),
        ))
        .child(block_in(key, 0, h1(v.name.clone()).mt(px(18.0))))
        .child(block_in(key, 1, stats));
    if let Some(z) = &v.zones {
        page = page.child(block_in(key, 2, zones(z, t)));
    }
    if let Some(c) = &v.chart {
        page = page.child(block_in(key, 3, chart(c, t)));
    }
    if !v.laps.is_empty() {
        page = page.child(block_in(key, 4, laps(&v.laps, t)));
    }
    if let Some(s) = &v.strength {
        page = page.child(block_in(key, 4, strength(s, t)));
    }
    page.into_any_element()
}

/// Said when the list cannot be read. The reason stays in the text.
pub fn failed(error: &anyhow::Error, t: &Theme) -> AnyElement {
    body(
        format!("The list could not be read. {error:#}"),
        FS_LG,
        t.mutd,
    )
    .mt(px(28.0))
    .into_any_element()
}

/// Said when the id has no row, for example after a cache reset.
pub fn missing(t: &Theme, cx: &mut Context<Root>) -> AnyElement {
    div()
        .child(back(t, cx))
        .child(body("This session is not in the cache.", FS_LG, t.mutd).mt(px(28.0)))
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
        let id = list.groups[0].sessions[0].activity_id;
        assert!(store.detail(id).unwrap().is_some());
        assert!(store.detail(1).unwrap().is_none());
        let _ = std::fs::remove_dir_all(store.path.parent().unwrap());
    }
}
