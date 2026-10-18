//! The Health screen as one view model.
//!
//! Each metric carries its series, a figure and a trend sentence. A metric
//! that the watch does not record has no reading in the window, so it is left
//! out. SpO2 and respiration come from the sleep nights, the rest from the
//! daily rows.

use crate::derive::{comparable_resting_hr, daily_series, date_key, mean};
use crate::text::num;
use crate::ApiError;
use chrono::NaiveDate;
use garmin_core::{DailyMetrics, Db};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// SpO2 and respiration of one night.
type NightFigures = (Option<f64>, Option<f64>);

/// The window that every series covers.
const DAYS: u32 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricTrend {
    /// `resting_hr`, `hrv`, `stress`, `body_battery`, `steps`, `spo2` or `respiration`.
    pub key: String,
    pub label: String,
    /// `bpm`, `ms`, `%` or `br/min`. Empty for a unitless score.
    pub unit: String,
    /// The latest reading, formatted without its unit.
    pub value_text: String,
    pub caption: String,
    /// One slot for each day, oldest first. A day with no reading is `None`.
    pub points: Vec<Option<f64>>,
    pub sentence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthView {
    /// The day the view was built for, `YYYY-MM-DD`.
    pub date: String,
    pub days: u32,
    /// True when no metric has a reading. The UI then shows `note`.
    pub empty: bool,
    pub note: String,
    pub metrics: Vec<MetricTrend>,
}

/// How to draw and read one metric.
struct Spec {
    key: &'static str,
    label: &'static str,
    unit: &'static str,
    decimals: usize,
    /// `Some(true)` when a rise is good news, `Some(false)` when it is bad,
    /// and `None` when a rise has no verdict.
    up_is_good: Option<bool>,
    points: Vec<Option<f64>>,
}

impl Spec {
    fn fmt(&self, v: f64) -> String {
        if self.decimals == 0 {
            num(v)
        } else {
            format!("{:.*}", self.decimals, v)
        }
    }

    fn with_unit(&self, v: f64) -> String {
        match self.unit {
            "" => self.fmt(v),
            "%" => format!("{}%", self.fmt(v)),
            unit => format!("{} {unit}", self.fmt(v)),
        }
    }
}

/// A plain-language read of the trend: the recent third against the earlier
/// two thirds. It makes no claim when the change is inside the noise.
fn trend_sentence(spec: &Spec) -> String {
    let values = &spec.points;
    let real: Vec<f64> = values.iter().flatten().copied().collect();
    if real.len() < 4 || values.len() < 6 {
        return match real.len() {
            0 => "Nothing recorded in this window.".into(),
            1 => format!(
                "One reading in this window: {}. A longer history gives a trend.",
                spec.with_unit(real[0])
            ),
            n => format!(
                "{n} readings in this window, averaging {}. A longer history gives a trend.",
                spec.with_unit(mean(real.iter().map(|v| Some(*v))).unwrap_or(0.0))
            ),
        };
    }
    let cut = values.len() / 3;
    let (earlier, recent) = values.split_at(values.len() - cut);
    let (Some(recent), Some(earlier)) =
        (mean(recent.iter().copied()), mean(earlier.iter().copied()))
    else {
        return "Not enough history to call a trend yet.".into();
    };
    if earlier.abs() < f64::EPSILON {
        return "Not enough history to call a trend yet.".into();
    }
    let change = (recent - earlier) / earlier.abs() * 100.0;
    if change.abs() < 3.0 {
        return format!(
            "Flat across the window: recent average {}, against {} before.",
            spec.with_unit(recent),
            spec.with_unit(earlier)
        );
    }
    let rising = change > 0.0;
    let verdict = match spec.up_is_good {
        Some(good) if good == rising => " — the direction you want",
        Some(_) => " — worth keeping an eye on",
        None => "",
    };
    format!(
        "{} {:.0}% on the earlier part of this window ({} against {}){verdict}.",
        if rising { "Up" } else { "Down" },
        change.abs(),
        spec.with_unit(recent),
        spec.with_unit(earlier)
    )
}

fn specs(rows: &[DailyMetrics], nights: &HashMap<String, NightFigures>) -> Vec<Spec> {
    let comparable = comparable_resting_hr(rows);
    let daily = |get: fn(&DailyMetrics) -> Option<f64>| rows.iter().map(get).collect::<Vec<_>>();
    let night = |pick: fn(&NightFigures) -> Option<f64>| {
        rows.iter()
            .map(|r| nights.get(&r.date).and_then(pick))
            .collect::<Vec<_>>()
    };
    let spec = |key, label, unit, decimals, up_is_good, points| Spec {
        key,
        label,
        unit,
        decimals,
        up_is_good,
        points,
    };
    vec![
        spec(
            "resting_hr",
            "Resting HR",
            "bpm",
            0,
            Some(false),
            comparable.iter().map(|d| d.resting_hr).collect(),
        ),
        spec(
            "hrv",
            "HRV",
            "ms",
            0,
            Some(true),
            daily(|d| d.hrv_last_night),
        ),
        spec(
            "stress",
            "Stress",
            "",
            0,
            Some(false),
            daily(|d| d.stress_avg),
        ),
        spec(
            "body_battery",
            "Body battery",
            "",
            0,
            Some(true),
            daily(|d| d.body_battery_high),
        ),
        // Zero steps is a watch that was not worn, not a day without a walk.
        spec(
            "steps",
            "Steps",
            "",
            0,
            Some(true),
            daily(|d| d.steps.filter(|s| *s > 0).map(|s| s as f64)),
        ),
        spec("spo2", "SpO2", "%", 0, Some(true), night(|n| n.0)),
        spec(
            "respiration",
            "Respiration",
            "br/min",
            1,
            None,
            night(|n| n.1),
        ),
    ]
}

fn trend(spec: Spec) -> Option<MetricTrend> {
    let (date_idx, latest) = spec
        .points
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, p)| p.filter(|v| v.is_finite()).map(|v| (i, v)))?;
    let readings = spec.points.iter().flatten().count();
    let average = mean(spec.points.iter().copied())?;
    let age = spec.points.len() - 1 - date_idx;
    let when = match age {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        n => format!("{n} days ago"),
    };
    Some(MetricTrend {
        caption: format!(
            "Latest {when}. Average {} over {readings} {}.",
            spec.with_unit(average),
            if readings == 1 { "day" } else { "days" }
        ),
        sentence: trend_sentence(&spec),
        value_text: spec.fmt(latest),
        key: spec.key.into(),
        label: spec.label.into(),
        unit: spec.unit.into(),
        points: spec.points,
    })
}

/// Build the Health view for `today`.
pub fn health_view(db: &Db, today: NaiveDate) -> Result<HealthView, ApiError> {
    let from = date_key(today - chrono::Duration::days(DAYS as i64 - 1));
    let rows = daily_series(db.daily_since(&from)?, DAYS, today);
    let nights: HashMap<_, _> = db
        .sleep_nights_since(&from)?
        .into_iter()
        .map(|n| (n.date, (n.avg_spo2, n.avg_respiration)))
        .collect();
    let metrics: Vec<MetricTrend> = specs(&rows, &nights)
        .into_iter()
        .filter_map(trend)
        .collect();
    let empty = metrics.is_empty();
    Ok(HealthView {
        date: date_key(today),
        days: DAYS,
        empty,
        note: if empty {
            "No health readings are cached yet. The first sync fills this in.".into()
        } else {
            format!(
                "The last {DAYS} days. Each chart has its own scale, so the shape matters more than the height."
            )
        },
        metrics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()
    }

    fn db(tag: &str) -> Db {
        let dir =
            std::env::temp_dir().join(format!("garmin-api-health-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Db::open(&dir.join("cache.sqlite3")).unwrap()
    }

    fn series(points: Vec<Option<f64>>, good: Option<bool>) -> Spec {
        Spec {
            key: "hrv",
            label: "HRV",
            unit: "ms",
            decimals: 0,
            up_is_good: good,
            points,
        }
    }

    #[test]
    fn empty_cache_has_no_metric() {
        let v = health_view(&db("empty"), today()).unwrap();
        assert!(v.empty && v.metrics.is_empty());
        assert!(v.note.starts_with("No health readings"));
    }

    #[test]
    fn a_metric_with_no_reading_is_hidden() {
        let db = db("sleep-only");
        db.upsert_daily(&DailyMetrics {
            date: "2026-03-11".into(),
            hrv_last_night: Some(48.0),
            steps: Some(0),
            ..DailyMetrics::default()
        })
        .unwrap();
        let keys: Vec<_> = health_view(&db, today())
            .unwrap()
            .metrics
            .into_iter()
            .map(|m| m.key)
            .collect();
        assert_eq!(keys, ["hrv"]);
    }

    #[test]
    fn the_sentence_reads_direction_and_verdict() {
        let mut points: Vec<Option<f64>> = vec![Some(50.0); 20];
        points.extend(vec![Some(40.0); 10]);
        assert_eq!(
            trend_sentence(&series(points.clone(), Some(true))),
            "Down 20% on the earlier part of this window (40 ms against 50 ms) — worth keeping an eye on."
        );
        assert!(trend_sentence(&series(points, None)).ends_with("50 ms)."));
        assert!(trend_sentence(&series(vec![Some(50.0); 30], Some(true))).starts_with("Flat"));
    }

    #[test]
    fn a_short_series_makes_no_trend_claim() {
        let mut points = vec![None; 29];
        points.push(Some(47.0));
        assert!(trend_sentence(&series(points, Some(true))).starts_with("One reading"));
        assert_eq!(
            trend_sentence(&series(vec![None; 30], Some(true))),
            "Nothing recorded in this window."
        );
    }

    #[test]
    fn the_caption_says_how_old_the_latest_reading_is() {
        let mut points = vec![None; 30];
        points[27] = Some(52.0);
        let t = trend(series(points, Some(true))).unwrap();
        assert_eq!(t.caption, "Latest 2 days ago. Average 52 ms over 1 day.");
        assert_eq!(t.value_text, "52");
    }
}
