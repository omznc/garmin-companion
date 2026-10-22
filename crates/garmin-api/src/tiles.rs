//! Series for the Today and Sleep blocks: the health tiles with a 30-day
//! baseline, the sleep debt, the sleep score and the heart rate at a fixed pace.
//!
//! Each function takes rows and a day. A tile with no reading is left out, and
//! the pace block names the run only when the cache holds easy runs.

use crate::derive::{
    above_z2_pct, comparable_resting_hr, daily_series, date_key, has_zone_data, is_run, latest,
    mean,
};
use crate::ApiError;
use chrono::{Duration, NaiveDate};
use garmin_core::sleep::SleepNight;
use garmin_core::{CachedActivity, DailyMetrics, Db};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Days in a tile series, the sleep debt and the sleep score.
const SERIES_DAYS: u32 = 14;
/// Days in the baseline of a tile.
const BASELINE_DAYS: u32 = 30;
/// Days of runs that the pace block reads.
const PACE_WINDOW_DAYS: i64 = 90;
/// The most easy runs that the pace block reads.
const PACE_RUNS: usize = 10;
/// The fewest runs near the median pace that make a drift.
const PACE_MIN_RUNS: usize = 4;
/// A run counts as near the median pace within this share of the pace.
const PACE_BAND: f64 = 0.05;
/// A run counts as easy when at most this share of its time is above Z2.
const EASY_ABOVE_Z2_PCT: f64 = 30.0;
/// A run under this length says little about the heart rate at a pace.
const MIN_RUN_M: f64 = 3000.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct HealthTile {
    /// `resting_hr`, `hrv`, `sleep`, `body_battery` or `stress`.
    pub key: String,
    pub label: String,
    /// `bpm`, `ms`, `s` (seconds of sleep) or empty.
    pub unit: String,
    /// The newest reading in the baseline window.
    pub latest: Option<f64>,
    /// 14 days, oldest first. A day with no reading is `None`.
    pub series: Vec<Option<f64>>,
    /// The mean of the readings in the last 30 days.
    pub baseline: Option<f64>,
    /// `latest` minus `baseline`.
    pub delta: Option<f64>,
    /// A second figure for the tile: the sleep score, or the Body Battery
    /// charge over the night.
    pub secondary: Option<f64>,
    pub secondary_label: Option<String>,
    /// The balanced band of the HRV, in ms.
    pub band_low: Option<f64>,
    pub band_high: Option<f64>,
    /// Garmin's HRV status word, for example `BALANCED`.
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SleepDebt {
    /// Nights with both a need and a sleep time.
    pub nights: usize,
    /// Need minus sleep, summed over the nights, never below zero. A night
    /// above its need pays back a night below it.
    pub debt_secs: f64,
}

/// The average bed and wake times of the last nights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BedWake {
    pub nights: usize,
    /// Minutes after midnight, 0 to 1439.
    pub bed_minute: f64,
    pub wake_minute: f64,
    /// The spread of the bed time: one standard deviation, in minutes.
    pub consistency_mins: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SleepScores {
    /// The last 14 nights, oldest first. A night with no score is `None`.
    pub points: Vec<Option<f64>>,
    pub average: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PaceRun {
    pub activity_id: i64,
    pub date: String,
    pub avg_hr: f64,
    pub pace_min_per_km: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PaceDrift {
    /// The median pace of the easy runs, in minutes per kilometre.
    pub pace_min_per_km: f64,
    /// The runs near that pace, oldest first.
    pub runs: Vec<PaceRun>,
    /// The mean heart rate of the newest two runs minus that of the oldest two.
    pub drift_bpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TilesView {
    pub date: String,
    pub tiles: Vec<HealthTile>,
    pub sleep_debt: Option<SleepDebt>,
    pub sleep_scores: SleepScores,
    pub pace_drift: Option<PaceDrift>,
    #[serde(default)]
    pub bed_wake: Option<BedWake>,
}

fn tail<T: Clone>(v: &[T], n: usize) -> Vec<T> {
    v[v.len().saturating_sub(n)..].to_vec()
}

/// The figures that vary from tile to tile.
struct Spec {
    key: &'static str,
    label: &'static str,
    unit: &'static str,
    points: Vec<Option<f64>>,
}

fn tile(spec: Spec) -> Option<HealthTile> {
    let latest = spec
        .points
        .iter()
        .rev()
        .find_map(|p| p.filter(|v| v.is_finite()));
    let baseline = mean(spec.points.iter().copied());
    Some(HealthTile {
        key: spec.key.into(),
        label: spec.label.into(),
        unit: spec.unit.into(),
        delta: latest.zip(baseline).map(|(l, b)| l - b),
        series: tail(&spec.points, SERIES_DAYS as usize),
        latest: Some(latest?),
        baseline,
        secondary: None,
        secondary_label: None,
        band_low: None,
        band_high: None,
        status: None,
    })
}

/// The tiles for the five health figures. `rows` holds the last 30 days,
/// oldest first, one row for each day. A tile with no reading is left out.
pub fn health_tiles(rows: &[DailyMetrics], nights: &[SleepNight]) -> Vec<HealthTile> {
    let comparable = comparable_resting_hr(rows);
    let col = |get: fn(&DailyMetrics) -> Option<f64>| rows.iter().map(get).collect::<Vec<_>>();
    let mut resting = tile(Spec {
        key: "resting_hr",
        label: "Resting HR",
        unit: "bpm",
        points: comparable.iter().map(|d| d.resting_hr).collect(),
    });
    let mut hrv = tile(Spec {
        key: "hrv",
        label: "HRV",
        unit: "ms",
        points: col(|d| d.hrv_last_night),
    });
    let mut sleep = tile(Spec {
        key: "sleep",
        label: "Sleep",
        unit: "s",
        points: col(|d| d.sleep_secs),
    });
    let mut battery = tile(Spec {
        key: "body_battery",
        label: "Body Battery",
        unit: "",
        points: col(|d| d.body_battery_high),
    });
    let mut stress = tile(Spec {
        key: "stress",
        label: "Stress",
        unit: "",
        points: col(|d| d.stress_avg),
    });

    if let (Some(t), Some(row)) = (
        hrv.as_mut(),
        rows.iter().rev().find(|d| d.hrv_balanced_low.is_some()),
    ) {
        t.band_low = row.hrv_balanced_low;
        t.band_high = row.hrv_balanced_upper;
    }
    if let (Some(t), Some(row)) = (
        hrv.as_mut(),
        rows.iter().rev().find(|d| d.hrv_status.is_some()),
    ) {
        t.status = row.hrv_status.clone();
    }
    if let Some(t) = sleep.as_mut() {
        t.secondary = latest(rows, |d| d.sleep_score).map(|l| l.value);
        t.secondary_label = t.secondary.map(|_| "score".into());
    }
    if let (Some(t), Some(night)) = (
        battery.as_mut(),
        nights
            .iter()
            .rev()
            .find(|n| n.body_battery_change.is_some()),
    ) {
        t.secondary = night.body_battery_change;
        t.secondary_label = Some("overnight".into());
    }
    [
        resting.take(),
        hrv.take(),
        sleep.take(),
        battery.take(),
        stress.take(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Need minus sleep over the nights from `from` to `to`, both included.
pub fn sleep_debt(nights: &[SleepNight], from: &str, to: &str) -> Option<SleepDebt> {
    let counted: Vec<f64> = nights
        .iter()
        .filter(|n| n.date.as_str() >= from && n.date.as_str() <= to)
        .filter_map(|n| Some(n.need_secs? - n.total_secs?))
        .collect();
    (!counted.is_empty()).then(|| SleepDebt {
        nights: counted.len(),
        debt_secs: counted.iter().sum::<f64>().max(0.0),
    })
}

/// Minutes after midnight from `YYYY-MM-DDTHH:MM:SS`.
fn minute_of_day(local: &str) -> Option<f64> {
    let h: f64 = local.get(11..13)?.parse().ok()?;
    let m: f64 = local.get(14..16)?.parse().ok()?;
    Some(h * 60.0 + m)
}

/// The mean of clock minutes. Times before `cut` count as the next day, so
/// 23:30 and 00:30 average to 00:00 and not to noon.
fn clock_mean(minutes: &[f64], cut: f64) -> f64 {
    let shifted: Vec<f64> = minutes
        .iter()
        .map(|m| if *m < cut { m + 1440.0 } else { *m })
        .collect();
    (shifted.iter().sum::<f64>() / shifted.len() as f64).rem_euclid(1440.0)
}

/// The mean bed and wake time over the nights from `from` to `to`, with the
/// spread of the bed time. `None` when no night has both times.
pub fn bed_wake(nights: &[SleepNight], from: &str, to: &str) -> Option<BedWake> {
    let times: Vec<(f64, f64)> = nights
        .iter()
        .filter(|n| n.date.as_str() >= from && n.date.as_str() <= to)
        .filter_map(|n| {
            Some((
                minute_of_day(n.start_local.as_deref()?)?,
                minute_of_day(n.end_local.as_deref()?)?,
            ))
        })
        .collect();
    if times.is_empty() {
        return None;
    }
    let beds: Vec<f64> = times.iter().map(|t| t.0).collect();
    let wakes: Vec<f64> = times.iter().map(|t| t.1).collect();
    // A bed time before 12:00 is after midnight.
    let bed = clock_mean(&beds, 720.0);
    let spread = beds
        .iter()
        .map(|b| {
            let d = (b - bed).rem_euclid(1440.0);
            let d = if d > 720.0 { d - 1440.0 } else { d };
            d * d
        })
        .sum::<f64>()
        / beds.len() as f64;
    Some(BedWake {
        nights: times.len(),
        bed_minute: bed,
        wake_minute: clock_mean(&wakes, 0.0),
        consistency_mins: spread.sqrt(),
    })
}

/// The sleep score of each of the last 14 nights, oldest first.
pub fn sleep_scores(nights: &[SleepNight], today: NaiveDate) -> SleepScores {
    let by_date: HashMap<&str, f64> = nights
        .iter()
        .filter_map(|n| Some((n.date.as_str(), n.score?)))
        .collect();
    let points: Vec<Option<f64>> = (0..SERIES_DAYS)
        .rev()
        .map(|i| {
            by_date
                .get(date_key(today - Duration::days(i as i64)).as_str())
                .copied()
        })
        .collect();
    SleepScores {
        average: mean(points.iter().copied()),
        points,
    }
}

/// Pace in minutes per kilometre, over elapsed time.
fn pace(a: &CachedActivity) -> Option<f64> {
    a.pace_min_per_km().filter(|p| p.is_finite() && *p > 0.0)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    let mid = v.len() / 2;
    if v.len().is_multiple_of(2) {
        (v[mid - 1] + v[mid]) / 2.0
    } else {
        v[mid]
    }
}

fn pace_run(a: &CachedActivity) -> Option<PaceRun> {
    Some(PaceRun {
        activity_id: a.activity_id,
        date: a.local_date.clone()?,
        avg_hr: a.avg_hr.filter(|hr| *hr > 0.0)?,
        pace_min_per_km: pace(a)?,
    })
}

/// The heart rate over the newest easy runs that were near one pace.
///
/// An easy run has heart-rate zones and at most 30% of its time above Z2. The
/// pace is the median of the newest 10. The runs within 5% of it are compared,
/// so a slower or faster run does not look like a drift. With fewer than 4 such
/// runs there is no drift to report.
pub fn pace_drift(activities: &[CachedActivity]) -> Option<PaceDrift> {
    let mut easy: Vec<&CachedActivity> = activities
        .iter()
        .filter(|a| is_run(&a.type_key) && a.distance_m.is_some_and(|m| m >= MIN_RUN_M))
        .filter(|a| has_zone_data(a) && above_z2_pct(a) <= EASY_ABOVE_Z2_PCT)
        .filter(|a| pace_run(a).is_some())
        .collect();
    easy.sort_by(|a, b| b.start_time_local.cmp(&a.start_time_local));
    easy.truncate(PACE_RUNS);
    let runs: Vec<PaceRun> = easy.iter().rev().filter_map(|a| pace_run(a)).collect();
    if runs.len() < PACE_MIN_RUNS {
        return None;
    }
    let mid = median(runs.iter().map(|r| r.pace_min_per_km).collect());
    let near: Vec<PaceRun> = runs
        .into_iter()
        .filter(|r| (r.pace_min_per_km - mid).abs() <= mid * PACE_BAND)
        .collect();
    if near.len() < PACE_MIN_RUNS {
        return None;
    }
    let avg = |part: &[PaceRun]| part.iter().map(|r| r.avg_hr).sum::<f64>() / part.len() as f64;
    let drift_bpm = avg(&near[near.len() - 2..]) - avg(&near[..2]);
    Some(PaceDrift {
        pace_min_per_km: mid,
        runs: near,
        drift_bpm,
    })
}

/// Build the series for `today`.
pub fn tiles_view(db: &Db, today: NaiveDate) -> Result<TilesView, ApiError> {
    let from = date_key(today - Duration::days(BASELINE_DAYS as i64 - 1));
    let rows = daily_series(db.daily_since(&from)?, BASELINE_DAYS, today);
    let nights = db.sleep_nights_since(&from)?;
    let activities = db.activities_since(&date_key(today - Duration::days(PACE_WINDOW_DAYS)))?;
    let debt_from = date_key(today - Duration::days(SERIES_DAYS as i64 - 1));
    Ok(TilesView {
        date: date_key(today),
        tiles: health_tiles(&rows, &nights),
        sleep_debt: sleep_debt(&nights, &debt_from, &date_key(today)),
        sleep_scores: sleep_scores(&nights, today),
        pace_drift: pace_drift(&activities),
        bed_wake: bed_wake(&nights, &debt_from, &date_key(today)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()
    }

    fn night(date: &str, need: Option<f64>, slept: Option<f64>, score: Option<f64>) -> SleepNight {
        SleepNight {
            date: date.into(),
            need_secs: need,
            total_secs: slept,
            score,
            ..Default::default()
        }
    }

    fn rows(f: impl Fn(u32, &mut DailyMetrics)) -> Vec<DailyMetrics> {
        let list = (0..30u32)
            .map(|i| DailyMetrics {
                date: date_key(today() - Duration::days(29 - i as i64)),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        list.into_iter()
            .enumerate()
            .map(|(i, mut d)| {
                f(i as u32, &mut d);
                d
            })
            .collect()
    }

    #[test]
    fn a_tile_has_a_14_day_series_a_30_day_baseline_and_a_delta() {
        let r = rows(|i, d| {
            d.hrv_last_night = Some(if i == 29 { 70.0 } else { 50.0 });
            d.hrv_balanced_low = Some(45.0);
            d.hrv_balanced_upper = Some(60.0);
            d.hrv_status = Some("BALANCED".into());
            d.sleep_secs = (i > 10).then_some(25_200.0);
            d.sleep_score = (i == 29).then_some(81.0);
            d.stress_avg = Some(30.0);
        });
        let night = SleepNight {
            body_battery_change: Some(40.0),
            ..night("2026-03-11", None, None, None)
        };
        let tiles = health_tiles(&r, &[night]);
        let keys: Vec<_> = tiles.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(
            keys,
            ["hrv", "sleep", "stress"],
            "no resting HR and no battery, no tile"
        );
        let hrv = &tiles[0];
        assert_eq!(hrv.series.len(), 14);
        assert_eq!(hrv.latest, Some(70.0));
        let base = (29.0 * 50.0 + 70.0) / 30.0;
        assert!((hrv.baseline.unwrap() - base).abs() < 1e-9);
        assert!((hrv.delta.unwrap() - (70.0 - base)).abs() < 1e-9);
        assert_eq!((hrv.band_low, hrv.band_high), (Some(45.0), Some(60.0)));
        assert_eq!(hrv.status.as_deref(), Some("BALANCED"));
        assert_eq!(tiles[1].secondary, Some(81.0));
        assert_eq!(
            tiles[1].baseline,
            Some(25_200.0),
            "the baseline skips empty days"
        );
        assert_eq!(tiles[2].secondary, None);
    }

    #[test]
    fn the_battery_tile_carries_the_overnight_charge() {
        let r = rows(|_, d| d.body_battery_high = Some(80.0));
        let night = SleepNight {
            body_battery_change: Some(55.0),
            ..night("2026-03-11", None, None, None)
        };
        let t = &health_tiles(&r, &[night])[0];
        assert_eq!(
            (t.key.as_str(), t.secondary, t.secondary_label.as_deref()),
            ("body_battery", Some(55.0), Some("overnight"))
        );
        assert!(health_tiles(&rows(|_, _| {}), &[]).is_empty());
    }

    #[test]
    fn sleep_debt_sums_the_nights_that_have_both_figures() {
        let h = 3600.0;
        let nights = [
            night("2026-03-10", Some(8.0 * h), Some(6.0 * h), None),
            night("2026-03-09", Some(8.0 * h), Some(7.0 * h), None),
            night("2026-03-08", Some(8.0 * h), None, None),
            night("2026-03-07", None, Some(7.0 * h), None),
            night("2026-02-01", Some(8.0 * h), Some(1.0 * h), None),
        ];
        let d = sleep_debt(&nights, "2026-02-26", "2026-03-11").unwrap();
        assert_eq!((d.nights, d.debt_secs), (2, 3.0 * h));
        // Nights above the need pay the debt back, and the debt never goes negative.
        let rested = [night("2026-03-10", Some(7.0 * h), Some(9.0 * h), None)];
        assert_eq!(
            sleep_debt(&rested, "2026-02-26", "2026-03-11")
                .unwrap()
                .debt_secs,
            0.0
        );
        assert_eq!(sleep_debt(&[], "2026-02-26", "2026-03-11"), None);
    }

    #[test]
    fn bed_times_across_midnight_average_near_midnight() {
        let at = |date: &str, start: &str, end: &str| SleepNight {
            date: date.into(),
            start_local: Some(start.into()),
            end_local: Some(end.into()),
            ..Default::default()
        };
        let nights = [
            at("2026-03-10", "2026-03-09T23:30:00", "2026-03-10T07:00:00"),
            at("2026-03-11", "2026-03-11T00:30:00", "2026-03-11T08:00:00"),
        ];
        let b = bed_wake(&nights, "2026-03-01", "2026-03-11").unwrap();
        assert_eq!(b.nights, 2);
        assert!(
            b.bed_minute < 1.0 || b.bed_minute > 1439.0,
            "{}",
            b.bed_minute
        );
        assert_eq!(b.wake_minute, 450.0);
        assert!((b.consistency_mins - 30.0).abs() < 1e-9);
        assert_eq!(bed_wake(&[], "a", "b"), None);
    }

    #[test]
    fn sleep_scores_fill_14_slots() {
        let nights = [
            night("2026-03-11", None, None, Some(80.0)),
            night("2026-03-09", None, None, Some(70.0)),
            night("2026-02-20", None, None, Some(10.0)),
        ];
        let s = sleep_scores(&nights, today());
        assert_eq!(s.points.len(), 14);
        assert_eq!(
            (s.points[13], s.points[12], s.points[11]),
            (Some(80.0), None, Some(70.0))
        );
        assert_eq!(s.average, Some(75.0));
        assert_eq!(sleep_scores(&[], today()).average, None);
    }

    fn run(id: i64, date: &str, km: f64, mins: f64, hr: f64, above: f64) -> CachedActivity {
        CachedActivity {
            activity_id: id,
            name: None,
            type_key: Some("running".into()),
            start_time_local: Some(format!("{date} 07:00:00")),
            local_date: Some(date.into()),
            distance_m: Some(km * 1000.0),
            duration_s: Some(mins * 60.0),
            moving_duration_s: None,
            avg_hr: Some(hr),
            max_hr: None,
            avg_cadence: None,
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: [100.0 - above, 0.0, above, 0.0, 0.0],
            training_load: None,
        }
    }

    #[test]
    fn the_drift_compares_only_easy_runs_near_the_median_pace() {
        // 5:30 per km is 8 km in 44 minutes.
        let acts = [
            run(6, "2026-03-10", 8.0, 44.0, 142.0, 5.0),
            run(5, "2026-03-08", 8.0, 44.0, 141.0, 5.0),
            run(4, "2026-03-06", 8.0, 44.0, 139.0, 5.0),
            run(3, "2026-03-04", 8.0, 44.0, 138.0, 5.0),
            run(9, "2026-03-09", 8.0, 36.0, 170.0, 5.0),
            run(8, "2026-03-07", 8.0, 44.0, 175.0, 60.0),
            run(7, "2026-03-05", 2.0, 11.0, 120.0, 5.0),
        ];
        let d = pace_drift(&acts).unwrap();
        assert_eq!(
            d.runs.iter().map(|r| r.activity_id).collect::<Vec<_>>(),
            [3, 4, 5, 6]
        );
        assert!((d.pace_min_per_km - 5.5).abs() < 1e-9);
        assert_eq!(d.drift_bpm, 3.0, "141.5 against 138.5");
    }

    #[test]
    fn too_few_runs_and_no_run_make_no_drift() {
        let few = [
            run(1, "2026-03-10", 8.0, 44.0, 140.0, 5.0),
            run(2, "2026-03-08", 8.0, 44.0, 141.0, 5.0),
        ];
        assert_eq!(pace_drift(&few), None);
        let mut ride = run(3, "2026-03-09", 20.0, 44.0, 120.0, 5.0);
        ride.type_key = Some("cycling".into());
        assert_eq!(
            pace_drift(&[ride.clone(), ride.clone(), ride.clone(), ride]),
            None
        );
        assert_eq!(pace_drift(&[]), None);
    }

    #[test]
    fn the_view_builds_from_an_empty_cache() {
        let db = Db::open(&std::env::temp_dir().join(format!(
            "garmin-api-tiles-{}/cache.sqlite3",
            std::process::id()
        )))
        .unwrap();
        let v = tiles_view(&db, today()).unwrap();
        assert!(v.tiles.is_empty() && v.sleep_debt.is_none() && v.pace_drift.is_none());
        assert_eq!(v.sleep_scores.points.len(), 14);
    }
}
