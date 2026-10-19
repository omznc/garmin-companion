//! The Health and Sleep screens, from `Health.tsx` and `Sleep.tsx`.
//!
//! `garmin-api` builds the figures and the sentences. These screens only draw
//! them. Neither screen needs an activity, so a user who wears the watch only
//! to bed still gets a full Sleep screen.

use anyhow::{ensure, Context, Result};
use gpui::{div, prelude::*, px, relative, AnyElement, ColorExt as _, Div};

use garmin_api::health::MetricTrend;
use garmin_api::sleep_view::{NightBar, NightView, SleepWindow, StageShare};
use garmin_api::{HealthView, SleepView};

use crate::data::{Metric, Part, Source};
use crate::theme::{Theme, FS_BASE, FS_CAPTION, FS_LG, FS_MD, FS_SMALL};
use crate::ui::{
    axis_labels, block_in, body, ch, eyebrow, eyebrow_lg, h1, metric, spark_wide, LH_NORMAL, SANS,
};

/// The made-up users of `garmin-api`, as the Health and Sleep views.
const PERSONAS: [(&str, &str, &str); 5] = [
    (
        "sample",
        include_str!("../../../crates/garmin-api/fixtures/health.sample.json"),
        include_str!("../../../crates/garmin-api/fixtures/sleep.sample.json"),
    ),
    (
        "golfer",
        include_str!("../../../crates/garmin-api/fixtures/health.golfer.json"),
        include_str!("../../../crates/garmin-api/fixtures/sleep.golfer.json"),
    ),
    (
        "gym",
        include_str!("../../../crates/garmin-api/fixtures/health.gym.json"),
        include_str!("../../../crates/garmin-api/fixtures/sleep.gym.json"),
    ),
    (
        "health_only",
        include_str!("../../../crates/garmin-api/fixtures/health.health_only.json"),
        include_str!("../../../crates/garmin-api/fixtures/sleep.health_only.json"),
    ),
    (
        "new_user",
        include_str!("../../../crates/garmin-api/fixtures/health.new_user.json"),
        include_str!("../../../crates/garmin-api/fixtures/sleep.new_user.json"),
    ),
];

pub struct Wellness {
    pub health: HealthView,
    pub sleep: SleepView,
}

/// Read both views from the same source as the Today screen.
pub fn load(source: &Source) -> Result<Wellness> {
    match source {
        Source::Default => {
            let db = garmin_api::open_default_db().context("cannot open the cache")?;
            let day = chrono::Local::now().date_naive();
            Ok(Wellness {
                health: garmin_api::health_view(&db, day).context("cannot build Health")?,
                sleep: garmin_api::sleep_view(&db, day).context("cannot build Sleep")?,
            })
        }
        Source::Persona(name) => {
            let (_, health, sleep) = PERSONAS
                .iter()
                .find(|(n, ..)| n == name)
                .with_context(|| format!("no persona {name}"))?;
            Ok(Wellness {
                health: serde_json::from_str(health).context("the health file does not parse")?,
                sleep: serde_json::from_str(sleep).context("the sleep file does not parse")?,
            })
        }
        Source::Db { path, date } => {
            ensure!(path.is_file(), "no cache copy at {}", path.display());
            let db = garmin_api::open_db(path).context("cannot open the cache copy")?;
            let day = date.unwrap_or_else(|| chrono::Local::now().date_naive());
            Ok(Wellness {
                health: garmin_api::health_view(&db, day).context("cannot build Health")?,
                sleep: garmin_api::sleep_view(&db, day).context("cannot build Sleep")?,
            })
        }
    }
}

fn header(eyebrow_text: &str, title: &str, lede: &str, key: usize, t: &Theme) -> impl IntoElement {
    div()
        .child(block_in(key, 0, eyebrow_lg(eyebrow_text.to_string(), t)))
        .child(block_in(key, 0, h1(title.to_string()).mt(px(18.0))))
        .child(block_in(
            key,
            1,
            div().mt(px(28.0)).mb(px(44.0)).child(
                div()
                    .max_w(ch(62.0, FS_MD))
                    .font_family(SANS)
                    .text_size(px(FS_MD))
                    .line_height(relative(1.65))
                    .text_color(t.mutd)
                    .child(lede.to_string()),
            ),
        ))
}

fn tile(label: &str, value: &str, unit: &str) -> Metric {
    let mut parts = vec![Part {
        text: value.to_string(),
        unit: false,
    }];
    if !unit.is_empty() {
        parts.push(Part {
            text: format!(" {unit}"),
            unit: true,
        });
    }
    Metric {
        label: label.to_string(),
        value: parts,
        accent: false,
    }
}

fn metric_row(m: &MetricTrend, t: &Theme) -> Div {
    let points: Vec<Option<f32>> = m.points.iter().map(|p| p.map(|v| v as f32)).collect();
    div()
        .flex()
        .flex_wrap()
        .items_start()
        .gap_x(px(40.0))
        .gap_y(px(14.0))
        .mb(px(48.0))
        .child(
            div()
                .w(px(190.0))
                .child(metric(&tile(&m.label, &m.value_text, &m.unit), t)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(260.0))
                .child(body(m.sentence.clone(), FS_BASE, t.mutd).line_height(relative(1.6)))
                .child(body(m.caption.clone(), FS_CAPTION, t.faint).mt(px(7.0)))
                .child(div().mt(px(12.0)).child(spark_wide(&points, t))),
        )
}

pub fn render_health(view: &HealthView, key: usize, t: &Theme) -> impl IntoElement {
    let eyebrow_text = format!("Last {} days", view.days);
    let mut screen = div().child(header(&eyebrow_text, "Health", &view.note, key, t));
    for (i, m) in view.metrics.iter().enumerate() {
        screen = screen.child(block_in(key, 2 + i, metric_row(m, t)));
    }
    screen
}

/// The stage colours, deep to awake. Deep is the part that you want more of,
/// so it takes the accent.
fn stage_fill(key: &str, t: &Theme) -> gpui::Hsla {
    match key {
        "deep" => t.acc,
        "rem" => t.acc.opacity(0.45),
        "light" => t.mutd,
        _ => t.warn,
    }
}

fn stage_bar(stages: &[StageShare], t: &Theme) -> Div {
    let bar = stages.iter().fold(
        div().flex().h(px(10.0)).overflow_hidden().rounded(px(2.0)),
        |bar, s| {
            bar.child(
                div()
                    .h_full()
                    .w(relative(s.pct as f32 / 100.0))
                    .bg(stage_fill(&s.key, t)),
            )
        },
    );
    let legend = stages.iter().fold(
        div()
            .flex()
            .flex_wrap()
            .gap(px(16.0))
            .mt(px(9.0))
            .font_family(SANS)
            .text_size(px(FS_CAPTION))
            .line_height(relative(LH_NORMAL))
            .text_color(t.mutd),
        |row, s| {
            row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .size(px(6.0))
                            .rounded(px(1.0))
                            .bg(stage_fill(&s.key, t)),
                    )
                    .child(format!("{} {}", s.label, s.text)),
            )
        },
    );
    div().child(bar).child(legend)
}

fn last_night(night: &NightView, note: Option<&str>, t: &Theme) -> Div {
    let mut tiles = vec![tile("Duration", &night.duration_text, "")];
    if let Some(score) = night.score {
        let label = night
            .qualifier
            .as_ref()
            .map_or("Score".to_string(), |q| format!("Score, {q}"));
        tiles.insert(0, tile(&label, &format!("{score:.0}"), ""));
    }
    if let (Some(start), Some(end)) = (&night.start, &night.end) {
        tiles.push(tile("Asleep", start, ""));
        tiles.push(tile("Woke", end, ""));
    }
    let row = tiles
        .iter()
        .fold(div().flex().flex_wrap().gap(px(54.0)), |row, m| {
            row.child(metric(m, t))
        });
    let mut out = div().child(row);
    if !night.stages.is_empty() {
        out = out.child(
            div()
                .mt(px(32.0))
                .max_w(px(560.0))
                .child(stage_bar(&night.stages, t)),
        );
    }
    out.children(note.map(|n| body(n.to_string(), FS_SMALL, t.faint).mt(px(14.0))))
}

/// Hours that fill a whole bar. A longer night fills it.
const FULL_BAR_HOURS: f32 = 10.0;
const BAR_H: f32 = 96.0;
/// A night under seven hours is a short night.
const SHORT_NIGHT_HOURS: f64 = 7.0;

fn night_bar(bar: &NightBar, gap_label: Option<&str>, t: &Theme) -> Div {
    let hours = bar.hours;
    let fill = if hours.is_some_and(|h| h < SHORT_NIGHT_HOURS) {
        t.mutd
    } else {
        t.acc
    };
    let height = hours.map_or(0.0, |h| (h as f32 / FULL_BAR_HOURS).min(1.0) * BAR_H);
    let column = div().flex_1().flex().flex_col().justify_end().h(px(BAR_H));
    let column = match hours {
        Some(_) => column.child(
            div()
                .w_full()
                .h(px(height.max(2.0)))
                .bg(fill)
                .rounded_t(px(2.0)),
        ),
        // A night with no data is a faint hairline, not a zero.
        None => column.child(div().w_full().h(px(1.0)).bg(t.line)),
    };
    match gap_label {
        Some(label) => div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(column)
            .child(body(label.to_string(), FS_CAPTION, t.faint).text_center()),
        None => column,
    }
}

fn window(title: &str, w: &SleepWindow, weekdays: bool, t: &Theme) -> Div {
    let bars = w.bars.iter().fold(
        div()
            .flex()
            .items_end()
            .gap(px(if weekdays { 10.0 } else { 3.0 })),
        |row, b| row.child(night_bar(b, weekdays.then_some(b.label.as_str()), t)),
    );
    let ends: Vec<String> = [w.bars.first(), w.bars.last()]
        .into_iter()
        .flatten()
        .map(|b| garmin_short_date(&b.date))
        .collect();
    div()
        .mt(px(72.0))
        .child(div().mb(px(18.0)).child(eyebrow(title.to_string(), t)))
        .child(bars)
        .children((!weekdays).then(|| axis_labels(&ends, t)))
        .child(
            body(w.summary.clone(), FS_BASE, t.mutd)
                .line_height(relative(1.6))
                .mt(px(16.0))
                .max_w(ch(58.0, FS_BASE)),
        )
}

/// `2026-03-11` as `11 Mar`.
fn garmin_short_date(date: &str) -> String {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_or_else(|_| date.to_string(), |d| d.format("%d %b").to_string())
}

fn insights(view: &SleepView, t: &Theme) -> Div {
    let list = view
        .insights
        .iter()
        .fold(div().flex().flex_col().gap(px(20.0)), |col, i| {
            col.child(
                div()
                    .max_w(ch(58.0, FS_BASE))
                    .child(body(i.claim.clone(), FS_LG, t.fg))
                    .child(
                        body(i.detail.clone(), FS_SMALL, t.faint)
                            .line_height(relative(1.55))
                            .mt(px(4.0)),
                    ),
            )
        });
    div()
        .mt(px(72.0))
        .child(div().mb(px(16.0)).child(eyebrow("What your nights say", t)))
        .child(list)
}

pub fn render_sleep(view: &SleepView, key: usize, t: &Theme) -> impl IntoElement {
    let title = view
        .last_night
        .as_ref()
        .map_or("Sleep", |n| n.title.as_str());
    let mut screen = div().child(header(title, "Sleep", &view.lede, key, t));
    if view.empty {
        return screen;
    }
    let mut blocks: Vec<AnyElement> = Vec::new();
    if let Some(night) = &view.last_night {
        blocks.push(last_night(night, view.note.as_deref(), t).into_any_element());
    }
    blocks.push(window("Last 7 nights", &view.week, true, t).into_any_element());
    blocks.push(window("Last 30 nights", &view.month, false, t).into_any_element());
    if !view.insights.is_empty() {
        blocks.push(insights(view, t).into_any_element());
    }
    for (i, block) in blocks.into_iter().enumerate() {
        screen = screen.child(block_in(key, 2 + i, block));
    }
    screen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::PERSONAS as TODAY_PERSONAS;
    use crate::theme::Palette;

    fn persona(name: &str) -> Wellness {
        load(&Source::Persona(name.into())).unwrap()
    }

    #[test]
    fn every_today_persona_has_health_and_sleep_files() {
        for (name, _) in TODAY_PERSONAS {
            persona(name);
        }
    }

    #[test]
    fn a_watch_without_a_sensor_has_no_row_for_it() {
        let keys = |n: &str| -> Vec<String> {
            persona(n)
                .health
                .metrics
                .into_iter()
                .map(|m| m.key)
                .collect()
        };
        assert!(keys("sample").contains(&"spo2".to_string()));
        assert!(!keys("golfer").contains(&"spo2".to_string()));
        assert!(!keys("golfer").contains(&"respiration".to_string()));
    }

    #[test]
    fn the_sleep_screen_needs_no_activity() {
        let sleep = persona("health_only").sleep;
        assert!(!sleep.empty);
        let night = sleep.last_night.unwrap();
        assert_eq!(night.stages.len(), 4);
        assert_eq!(sleep.week.bars.len(), 7);
        assert_eq!(sleep.month.bars.len(), 30);
    }

    #[test]
    fn both_screens_draw_for_every_persona() {
        let t = Palette::Paper.theme();
        for (name, _) in TODAY_PERSONAS {
            let w = persona(name);
            let _ = render_health(&w.health, 0, &t);
            let _ = render_sleep(&w.sleep, 0, &t);
        }
    }
}
