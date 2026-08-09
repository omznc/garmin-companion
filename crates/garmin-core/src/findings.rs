//! The deep findings — what a year of this data says when you ask it properly.
//!
//! These were written in TypeScript first and lived on one screen, which meant
//! the app could tell you at a glance that you now run 40 seconds per kilometre
//! faster at the same heart rate, and the coach you asked about it had no idea.
//! Everything here is the same analysis, moved to where both the MCP server and
//! the app's chat can reach it, per the rule in CLAUDE.md: an analysis lives in
//! `garmin-core` and both surfaces expose it.
//!
//! The port is not a transcription. Every claim that used to be a bare point
//! estimate now carries a bootstrap interval from [`crate::stats`], and a
//! finding whose interval straddles zero does not fire. That is a real change
//! in behaviour and it is the point of the exercise: with 51 runs in the whole
//! history, "the first three averaged X and the last three averaged Y" is a
//! sentence you can write about pure noise, and the old code would have.
//!
//! Findings are pure functions of rows that were already fetched. Nothing here
//! opens a database or touches the network, so the whole set can be exercised
//! against a hand-built history in tests.

use chrono::Datelike;
use serde::{Deserialize, Serialize};

use crate::db::{CachedActivity, DailyMetrics};
use crate::stats::{self, Estimate};

/// Seconds above threshold — Z4 and Z5. Z3 is tempo, which is not "hard".
pub fn hard_secs(a: &CachedActivity) -> f64 {
    a.zone_secs[3] + a.zone_secs[4]
}

/// Five minutes above threshold in a day. Below that it's a warm-up spike.
const HARD_DAY_SECS: f64 = 300.0;

const HOUR: f64 = 3600.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Good,
    Note,
    Watch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Section {
    Fitness,
    Recovery,
    Patterns,
}

/// How a charted series writes its own numbers.
///
/// An enum rather than the formatting closure the TypeScript carried, because
/// this shape crosses a serialization boundary now. The frontend maps these
/// back to the same formatters it always used.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Unit {
    Spm,
    Score,
    Pct,
    Pace,
    PerBeat,
    Load,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingSeries {
    pub name: String,
    pub values: Vec<Option<f64>>,
    pub format: Unit,
    /// Drawn as the comparison line rather than the subject.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub muted: bool,
    /// Low values at the top. Pace is the case: smaller is faster.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingRow {
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub accent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable slug, so the frontend can key on something other than an index
    /// and a model can refer to one by name.
    pub kind: String,
    pub section: Section,
    pub tone: Tone,
    /// One claim, no hedging — the hedge lives in `estimate` and `basis`.
    pub claim: String,
    pub detail: String,
    /// What was counted. A claim without one isn't shippable.
    pub basis: String,
    /// The estimate the claim rests on, where the claim is a number.
    ///
    /// This is the field that makes a finding checkable rather than merely
    /// readable: a model quoting one of these can quote the interval with it,
    /// and the system prompt tells it to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate: Option<Estimate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<FindingSeries>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<FindingRow>,
}

/* ---------------------------------------------------------------- helpers --- */

fn avg(xs: &[f64]) -> Option<f64> {
    stats::mean(xs)
}

fn parse(date: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d").ok()
}

fn days_between(a: &str, b: &str) -> i64 {
    match (parse(a), parse(b)) {
        (Some(x), Some(y)) => (y - x).num_days(),
        _ => 0,
    }
}

/// "8:34", from decimal minutes per kilometre.
fn pace_text(min_per_km: f64) -> String {
    let m = min_per_km.floor();
    let s = ((min_per_km - m) * 60.0).round();
    if s >= 60.0 {
        format!("{}:00", m as i64 + 1)
    } else {
        format!("{}:{:02}", m as i64, s as i64)
    }
}

const MONTH_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `2026-08` → "Aug". The year is carried by the basis line, not by every tick.
fn month_short(key: &str) -> String {
    key.get(5..7)
        .and_then(|m| m.parse::<usize>().ok())
        .and_then(|m| MONTH_SHORT.get(m.wrapping_sub(1)))
        .map(|s| (*s).to_string())
        .unwrap_or_else(|| key.to_string())
}

const DAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const DAY_SHORT: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// Days the watch was actually worn.
///
/// A padded window makes a blank row indistinguishable from a day off. Every
/// rate below divides by this rather than by the calendar, or a month of
/// unsynced history reads as a month of not training.
fn observed(daily: &[DailyMetrics]) -> Vec<&DailyMetrics> {
    daily
        .iter()
        .filter(|d| d.steps.is_some() || d.resting_hr.is_some() || d.sleep_secs.is_some())
        .collect()
}

/// Edwards' training impulse: minutes in each zone, weighted one through five.
///
/// One unit of load that knows an hour of Z2 and an hour of Z4 are not the same
/// hour, which raw duration doesn't.
pub fn edwards_trimp(a: &CachedActivity) -> f64 {
    let total: f64 = a.zone_secs.iter().sum();
    if total <= 0.0 {
        // No heart rate recorded — most strength sessions here. Counting it as
        // zero would make a lifting week read as a rest week; Z2 is the fair
        // guess, and the noise filter in `db` has already removed the rows
        // where this guess would have been applied to a phantom entry.
        return a.duration_s.unwrap_or(0.0) / 60.0 * 2.0;
    }
    a.zone_secs
        .iter()
        .enumerate()
        .map(|(i, secs)| secs / 60.0 * (i as f64 + 1.0))
        .sum()
}

/// Metres covered per heartbeat.
fn metres_per_beat(a: &CachedActivity) -> Option<f64> {
    let (d, s, hr) = (a.distance_m?, a.duration_s?, a.avg_hr?);
    if d <= 0.0 || s <= 0.0 || hr <= 0.0 {
        return None;
    }
    Some(d / (hr * (s / 60.0)))
}

/// Runs long enough to say anything about. A 70-metre entry is a false start.
const MIN_RUN_M: f64 = 400.0;
const MIN_RUN_S: f64 = 240.0;

fn scored_runs(activities: &[CachedActivity]) -> Vec<&CachedActivity> {
    let mut runs: Vec<&CachedActivity> = activities
        .iter()
        .filter(|a| {
            crate::query::is_run(a)
                && a.local_date.is_some()
                && a.distance_m.unwrap_or(0.0) >= MIN_RUN_M
                && a.duration_s.unwrap_or(0.0) >= MIN_RUN_S
                && a.avg_hr.unwrap_or(0.0) >= 100.0
        })
        .collect();
    runs.sort_by(|x, y| x.local_date.cmp(&y.local_date));
    runs
}

fn pace_min_per_km(a: &CachedActivity) -> Option<f64> {
    let (d, s) = (a.distance_m?, a.duration_s?);
    if d <= 0.0 {
        return None;
    }
    Some(s / 60.0 / (d / 1000.0))
}

/// Activities grouped by the local date they happened on.
fn by_date(
    activities: &[CachedActivity],
) -> std::collections::HashMap<String, Vec<&CachedActivity>> {
    let mut out: std::collections::HashMap<String, Vec<&CachedActivity>> = Default::default();
    for a in activities {
        if let Some(d) = &a.local_date {
            out.entry(d.clone()).or_default().push(a);
        }
    }
    out
}

/* ---------------------------------------------------------------- fitness --- */

/// Half-width of the heart-rate window two runs have to share to be compared.
const HR_BAND: f64 = 8.0;

/// Pace at a fixed heart rate — this account's substitute for a VO2 max.
///
/// Garmin will not compute VO2 max without outdoor GPS runs, and nearly every
/// run here is on a treadmill, so the number that would normally answer "am I
/// getting fitter?" is permanently blank. This answers it from the same
/// evidence: hold effort constant by comparing only runs whose average heart
/// rate landed within ±8 bpm of each other, and look at what the pace did.
///
/// The change to the TypeScript original is the gate. That version fired on any
/// gap above 15 s/km between the first third and the last third of the band —
/// over six runs, a threshold noise clears routinely. Here the trend is a fitted
/// slope over the whole band and it has to be a slope the interval actually
/// supports, which is a far higher bar and the right one.
pub fn fitness_at_fixed_hr(activities: &[CachedActivity]) -> Option<Finding> {
    let runs = scored_runs(activities);
    if runs.len() < 6 {
        return None;
    }

    // The band is chosen by where the runs are, not by a round number — the
    // window containing the most comparable sessions, so the comparison is made
    // over the largest sample the data actually offers.
    let mut band: Vec<&CachedActivity> = Vec::new();
    for a in &runs {
        let hr = a.avg_hr?;
        let members: Vec<&CachedActivity> = runs
            .iter()
            .filter(|x| (x.avg_hr.unwrap_or(0.0) - hr).abs() <= HR_BAND)
            .copied()
            .collect();
        if members.len() > band.len() {
            band = members;
        }
    }
    if band.len() < 6 {
        return None;
    }

    let from = band.first()?.local_date.clone()?;
    let to = band.last()?.local_date.clone()?;
    // Two months is the shortest window over which a change in aerobic fitness
    // is a change in aerobic fitness rather than a good day and a bad one.
    if days_between(&from, &to) < 60 {
        return None;
    }

    let paces: Vec<f64> = band.iter().filter_map(|a| pace_min_per_km(a)).collect();
    if paces.len() != band.len() {
        return None;
    }
    // Days since the first run in the band, so the slope is per day rather than
    // per session — sessions are not evenly spaced and a cluster would
    // otherwise weigh as much as a season.
    let xs: Vec<f64> = band
        .iter()
        .map(|a| days_between(&from, a.local_date.as_deref().unwrap_or(&from)) as f64)
        .collect();

    let fit = stats::linear_fit(&xs, &paces, 6)?;
    // A slope whose interval contains zero is not a direction. This is the gate
    // the original lacked.
    if !fit.slope.excludes_zero() {
        return None;
    }

    let span = days_between(&from, &to) as f64;
    // Read the slope out over the whole span, which is the number a person can
    // actually feel: seconds per kilometre gained since the band opened.
    let total = -fit.slope.value * span;
    let low = -fit.slope.high * span;
    let high = -fit.slope.low * span;
    let faster = total > 0.0;

    let hrs: Vec<f64> = band.iter().filter_map(|a| a.avg_hr).collect();
    let lo = hrs.iter().cloned().fold(f64::MAX, f64::min);
    let hi = hrs.iter().cloned().fold(f64::MIN, f64::max);

    // Minutes per kilometre, written the way a runner reads them. Past a minute
    // that means "4:34" rather than "274s" — the raw seconds are correct and
    // nobody can feel them, and this is the headline claim of the whole screen.
    // Under a minute the seconds are the clearer form, and no colon is needed.
    let magnitude = |v: f64| {
        let secs = (v.abs() * 60.0).round() as i64;
        if secs >= 60 {
            format!("{} min", pace_text(v.abs()))
        } else {
            format!("{secs}s")
        }
    };

    Some(Finding {
        kind: "fitness-at-fixed-hr".into(),
        section: Section::Fitness,
        tone: if faster { Tone::Good } else { Tone::Watch },
        claim: if faster {
            format!(
                "At the same heart rate you now run about {} per kilometre faster.",
                magnitude(total)
            )
        } else {
            format!(
                "At the same heart rate you're running about {} per kilometre slower than you were.",
                magnitude(total)
            )
        },
        detail: format!(
            "Across {} runs whose average heart rate landed between {:.0} and {:.0} bpm — the same \
             cost to you, whatever the treadmill said — pace has moved {} over {:.0} days. \
             Resampling those runs two thousand times puts the change between {} and {} per \
             kilometre, and every resample agrees on the direction, which is why this is stated as \
             a finding rather than as a hint. This is the closest thing your data has to a fitness \
             number: Garmin won't compute VO2 max without outdoor GPS runs, so it has never had \
             one to give you. {}",
            band.len(),
            lo,
            hi,
            if faster { "down" } else { "up" },
            span,
            magnitude(low.min(high)),
            magnitude(low.max(high)),
            if faster {
                "Same heartbeats, more ground. That is what getting fitter is."
            } else {
                "Worth reading against the calendar — a block of harder, shorter sessions can do \
                 this without anything being wrong."
            }
        ),
        basis: format!(
            "{} runs at {:.0}–{:.0} bpm · {} → {} · slope interval excludes zero",
            band.len(),
            lo,
            hi,
            from,
            to
        ),
        estimate: Some(Estimate {
            value: total,
            low: low.min(high),
            high: low.max(high),
            n: band.len(),
        }),
        series: vec![FindingSeries {
            name: "Pace at fixed HR".into(),
            values: paces.iter().map(|p| Some(*p)).collect(),
            format: Unit::Pace,
            muted: false,
            invert: true,
        }],
        labels: band.iter().filter_map(|a| a.local_date.clone()).collect(),
        rows: vec![],
    })
}

/// What a step rate is actually worth, in seconds per kilometre.
///
/// The app already nags about cadence. This says what the nag is worth, by
/// regressing metres-per-beat on cadence and reading the slope back out as pace
/// at a representative heart rate. "Aim for 170" is advice; "ten more steps a
/// minute has been worth forty seconds a kilometre to you" is a reason.
pub fn cadence_lever(activities: &[CachedActivity]) -> Option<Finding> {
    let runs: Vec<&CachedActivity> = scored_runs(activities)
        .into_iter()
        .filter(|a| a.avg_cadence.is_some() && metres_per_beat(a).is_some())
        .collect();
    if runs.len() < 10 {
        return None;
    }

    let cadence: Vec<f64> = runs.iter().filter_map(|a| a.avg_cadence).collect();
    let efficiency: Vec<f64> = runs.iter().filter_map(|a| metres_per_beat(a)).collect();

    let fit = stats::linear_fit(&cadence, &efficiency, 10)?;
    if !fit.slope.excludes_zero() {
        return None;
    }

    // Read the slope back out at a heart rate and a cadence actually run at,
    // rather than at the mean of a series spanning a year of changing form.
    let recent: Vec<&&CachedActivity> = runs.iter().rev().take(5).collect();
    let hr = avg(&recent.iter().filter_map(|a| a.avg_hr).collect::<Vec<_>>())?;
    let cad_now = avg(&recent
        .iter()
        .filter_map(|a| a.avg_cadence)
        .collect::<Vec<_>>())?;

    let pace_at = |spm: f64, slope: f64| {
        let per_beat = fit.intercept + slope * spm;
        if per_beat <= 0.0 {
            return None;
        }
        Some(1000.0 / (per_beat * hr))
    };
    let gain = |slope: f64| match (pace_at(cad_now, slope), pace_at(cad_now + 10.0, slope)) {
        (Some(a), Some(b)) => Some(a - b),
        _ => None,
    };

    let value = gain(fit.slope.value)?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    // The interval on the slope carried through to the interval on the answer,
    // so the sentence can say how well pinned down "worth forty seconds" is.
    let a = gain(fit.slope.low).unwrap_or(value);
    let b = gain(fit.slope.high).unwrap_or(value);

    let secs = |v: f64| format!("{}", (v * 60.0).round() as i64);

    Some(Finding {
        kind: "cadence-lever".into(),
        section: Section::Fitness,
        tone: Tone::Note,
        claim: format!(
            "Ten more steps a minute has been worth about {} seconds per kilometre to you.",
            secs(value)
        ),
        detail: format!(
            "Cadence and metres-per-heartbeat move together across {} runs, and the fitted slope \
             holds its sign through resampling — between {} and {} seconds per kilometre for ten \
             more steps a minute, explaining {:.0}% of the variance. Some of that is one fact told \
             twice, because on a treadmill turning the belt up raises both, so read the size of it \
             rather than the direction. You average {:.0} spm against a usual target near 170, and \
             at your recent {:.0} bpm the line puts {:.0} → {:.0} spm at {}/km → {}/km. Shorter, \
             quicker contacts also cut joint load, which matters more the heavier you are.",
            fit.slope.n,
            secs(a.min(b)),
            secs(a.max(b)),
            fit.r2 * 100.0,
            cad_now,
            hr,
            cad_now,
            cad_now + 10.0,
            pace_text(pace_at(cad_now, fit.slope.value)?),
            pace_text(pace_at(cad_now + 10.0, fit.slope.value)?),
        ),
        basis: format!(
            "{} runs · r² = {:.2} · slope {:.3} m/beat per 10 spm",
            fit.slope.n,
            fit.r2,
            fit.slope.value * 10.0
        ),
        estimate: Some(Estimate {
            value,
            low: a.min(b),
            high: a.max(b),
            n: fit.slope.n,
        }),
        series: vec![
            FindingSeries {
                name: "Cadence".into(),
                values: cadence.iter().map(|c| Some(*c)).collect(),
                format: Unit::Spm,
                muted: false,
                invert: false,
            },
            FindingSeries {
                name: "Per beat".into(),
                values: efficiency.iter().map(|e| Some(*e)).collect(),
                format: Unit::PerBeat,
                muted: true,
                invert: false,
            },
        ],
        labels: runs.iter().filter_map(|a| a.local_date.clone()).collect(),
        rows: vec![],
    })
}

/* --------------------------------------------------------------- recovery --- */

/// What actually moves overnight HRV, ranked — and whether the ranking is real.
///
/// The TypeScript version ranked seven candidates by |r| and named the winner.
/// At r ≈ 0.2 over a couple of hundred nights, first and second place swap on a
/// coin flip, and the copy still read "sleep score moves your HRV more than
/// anything else you record". This one asks [`stats::rank_stability`] how often
/// the leader survives resampling and refuses to name one when it doesn't.
pub fn recovery_drivers(daily: &[DailyMetrics], activities: &[CachedActivity]) -> Option<Finding> {
    let rows = observed(daily);
    if rows.len() < 60 {
        return None;
    }

    let mut load: std::collections::HashMap<&str, f64> = Default::default();
    for a in activities {
        if let Some(d) = &a.local_date {
            *load.entry(d.as_str()).or_insert(0.0) += edwards_trimp(a);
        }
    }

    let hrv: Vec<Option<f64>> = rows.iter().map(|d| d.hrv_last_night).collect();
    let rhr: Vec<Option<f64>> = rows.iter().map(|d| d.resting_hr).collect();

    let candidates: Vec<(&str, Vec<Option<f64>>)> = vec![
        ("Sleep score", rows.iter().map(|d| d.sleep_score).collect()),
        (
            "Hours asleep",
            rows.iter()
                .map(|d| d.sleep_secs.map(|s| s / HOUR))
                .collect(),
        ),
        (
            "Stress average",
            rows.iter().map(|d| d.stress_avg).collect(),
        ),
        (
            "Steps",
            rows.iter().map(|d| d.steps.map(|s| s as f64)).collect(),
        ),
        (
            "Training load that day",
            rows.iter()
                .map(|d| Some(*load.get(d.date.as_str()).unwrap_or(&0.0)))
                .collect(),
        ),
        (
            "Training load the day before",
            rows.iter()
                .enumerate()
                .map(|(i, _)| {
                    if i == 0 {
                        None
                    } else {
                        Some(*load.get(rows[i - 1].date.as_str()).unwrap_or(&0.0))
                    }
                })
                .collect(),
        ),
        (
            "Energy balance",
            rows.iter()
                .map(|d| match (d.consumed_kcal, d.total_burn_kcal) {
                    (Some(c), Some(b)) => Some(c - b),
                    _ => None,
                })
                .collect(),
        ),
    ];

    let mut ranked: Vec<(&str, Estimate, Option<Estimate>)> = candidates
        .iter()
        .filter_map(|(label, values)| {
            let r = stats::correlation(values, &hrv, 25)?;
            Some((*label, r, stats::correlation(values, &rhr, 25)))
        })
        .collect();
    if ranked.len() < 4 {
        return None;
    }
    ranked.sort_by(|a, b| {
        b.1.value
            .abs()
            .partial_cmp(&a.1.value.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Only the candidates that made the cut go into the stability check, so it
    // is asking about the ranking actually being displayed.
    let columns: Vec<Vec<Option<f64>>> = ranked
        .iter()
        .filter_map(|(label, _, _)| {
            candidates
                .iter()
                .find(|(l, _)| l == label)
                .map(|(_, v)| v.clone())
        })
        .collect();
    let stability = stats::rank_stability(&columns, &hrv);

    let top = ranked.first()?;
    let bottom = ranked.last()?;
    let training_rank = ranked
        .iter()
        .position(|(l, _, _)| l.starts_with("Training load that day"));

    // The threshold below which there is no leader, only a list. Two thirds is
    // where "it led in most resamples" stops being a defensible sentence.
    let stable = stability.map(|s| s >= 0.66).unwrap_or(false);

    let claim = if stable {
        format!(
            "{} moves your overnight HRV more than anything else you record.",
            top.0
        )
    } else {
        "Nothing you record clearly leads on overnight HRV — the ranking is not stable.".to_string()
    };

    Some(Finding {
        kind: "recovery-drivers".into(),
        section: Section::Recovery,
        tone: Tone::Note,
        detail: format!(
            "Every column of your daily table set against that night's HRV, strongest first. {} \
             leads at r = {:.2} over {} days; {} comes last at r = {:.2}.{} {} Training readiness \
             and body battery are missing on purpose: Garmin builds both partly out of HRV, so \
             they would top a table about what predicts HRV while teaching you nothing.",
            top.0,
            top.1.value,
            top.1.n,
            bottom.0,
            bottom.1.value,
            match training_rank {
                Some(i) => format!(
                    " How much you trained that day places {} of the {} — worth holding onto if \
                     you have ever assumed a hard session is what your recovery numbers are \
                     reacting to.",
                    ordinal(i + 1),
                    cardinal(ranked.len())
                ),
                None => String::new(),
            },
            match stability {
                Some(s) if s >= 0.66 => format!(
                    "Resampling the days two thousand times, that leader stayed on top {:.0}% of \
                     the time, which is why it is named at all.",
                    s * 100.0
                ),
                Some(s) => format!(
                    "Resampling the days two thousand times, the leader only stayed on top {:.0}% \
                     of the time — so read the table as a table, and do not act on which row is \
                     first. None of these correlations is large enough to be a mechanism.",
                    s * 100.0
                ),
                None => "There were too few complete days to test whether the order is stable, so \
                         treat the ranking as provisional."
                    .to_string(),
            },
        ),
        claim,
        basis: format!(
            "{} metrics over {} days · Pearson r against the same night's HRV{}",
            ranked.len(),
            rows.len(),
            match stability {
                Some(s) => format!(" · leader stable in {:.0}% of resamples", s * 100.0),
                None => String::new(),
            }
        ),
        estimate: Some(top.1),
        series: vec![],
        labels: vec![],
        rows: ranked
            .iter()
            .map(|(label, r, rhr_r)| FindingRow {
                label: (*label).to_string(),
                value: format!("{:.2}", r.value),
                note: Some(format!(
                    "{} days · 90% CI {:.2} to {:.2}{}",
                    r.n,
                    r.low,
                    r.high,
                    match rhr_r {
                        Some(x) => format!(" · resting HR r = {:.2}", x.value),
                        None => String::new(),
                    }
                )),
                accent: r.excludes_zero(),
            })
            .collect(),
    })
}

fn ordinal(n: usize) -> String {
    [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth",
    ]
    .get(n - 1)
    .map(|s| (*s).to_string())
    .unwrap_or_else(|| format!("{n}th"))
}

fn cardinal(n: usize) -> String {
    [
        "one", "two", "three", "four", "five", "six", "seven", "eight",
    ]
    .get(n - 1)
    .map(|s| (*s).to_string())
    .unwrap_or_else(|| format!("{n}"))
}

/* --------------------------------------------------------------- patterns --- */

/// Which day of the week your training actually falls on.
///
/// A weekly plan is written as seven equivalent slots. A year of data says
/// otherwise: some weekday is always the one that gets skipped, and it is rarely
/// the one you'd name. Knowing which it is turns "train more" into "put the long
/// easy run on a Sunday", which is a thing a person can do.
pub fn week_shape(daily: &[DailyMetrics], activities: &[CachedActivity]) -> Option<Finding> {
    let rows = observed(daily);
    if rows.len() < 60 {
        return None;
    }
    let days = by_date(activities);

    struct DayStat {
        seen: usize,
        trained: usize,
        sleep: Vec<f64>,
    }
    let mut stat: Vec<DayStat> = (0..7)
        .map(|_| DayStat {
            seen: 0,
            trained: 0,
            sleep: vec![],
        })
        .collect();

    for d in &rows {
        let Some(date) = parse(&d.date) else { continue };
        let w = date.weekday().num_days_from_monday() as usize;
        stat[w].seen += 1;
        if days.contains_key(&d.date) {
            stat[w].trained += 1;
        }
        if let Some(s) = d.sleep_secs {
            stat[w].sleep.push(s);
        }
    }
    if stat.iter().any(|s| s.seen < 6) {
        return None;
    }

    let rate: Vec<f64> = stat
        .iter()
        .map(|s| s.trained as f64 / s.seen as f64 * 100.0)
        .collect();
    let best = (0..7).max_by(|a, b| rate[*a].partial_cmp(&rate[*b]).unwrap())?;
    let worst = (0..7).min_by(|a, b| rate[*a].partial_cmp(&rate[*b]).unwrap())?;
    let second = (0..7)
        .filter(|i| *i != worst)
        .min_by(|a, b| rate[*a].partial_cmp(&rate[*b]).unwrap())?;
    // A flat week is a fine outcome and not a finding.
    if rate[best] - rate[worst] < 20.0 {
        return None;
    }

    Some(Finding {
        kind: "week-shape".into(),
        section: Section::Patterns,
        tone: Tone::Note,
        claim: format!("{} is the hole in your week.", DAY_NAMES[worst]),
        detail: format!(
            "Across {} days on the watch you trained on {:.0}% of {}s and {:.0}% of {}s, against \
             {:.0}% of {}s. That is not a motivation problem to be fixed — it is the shape of your \
             week, and it is more useful as a constraint than as a target. The one long easy run \
             you owe yourself each week wants the day you're most likely to have an hour, not the \
             day the plan says.",
            rows.len(),
            rate[worst],
            DAY_NAMES[worst],
            rate[second],
            DAY_NAMES[second],
            rate[best],
            DAY_NAMES[best],
        ),
        basis: format!(
            "{} days observed · {} of them with a session",
            rows.len(),
            days.len()
        ),
        estimate: None,
        series: vec![FindingSeries {
            name: "Trained".into(),
            values: rate.iter().map(|r| Some(*r)).collect(),
            format: Unit::Pct,
            muted: false,
            invert: false,
        }],
        labels: DAY_SHORT.iter().map(|s| (*s).to_string()).collect(),
        rows: (0..7)
            .map(|i| FindingRow {
                label: DAY_NAMES[i].to_string(),
                value: format!("{:.0}%", rate[i]),
                note: Some(format!(
                    "{} of {}{}",
                    stat[i].trained,
                    stat[i].seen,
                    match avg(&stat[i].sleep) {
                        Some(s) => format!(" · {:.1} h asleep", s / HOUR),
                        None => String::new(),
                    }
                )),
                accent: i == worst,
            })
            .collect(),
    })
}

/// How a training day compares with a rest day on the metrics that aren't about
/// training at all.
///
/// The intuition is that training is a cost paid in sleep and stress. Often the
/// data says the opposite, and it says it loudly enough to change how a rest day
/// gets planned — if your quiet days are the stressed ones, "rest" is doing
/// something other than resting.
///
/// Both gaps now carry an interval, and both have to exclude zero. The
/// TypeScript version fired on a 1.5-point difference in raw means, which over
/// a couple of hundred noisy days is not a difference at all.
pub fn rest_day_contrast(daily: &[DailyMetrics], activities: &[CachedActivity]) -> Option<Finding> {
    let rows = observed(daily);
    let days = by_date(activities);

    let collect = |pick: &dyn Fn(&DailyMetrics) -> bool,
                   field: &dyn Fn(&DailyMetrics) -> Option<f64>|
     -> Vec<f64> {
        rows.iter()
            .filter(|d| pick(d))
            .filter_map(|d| field(d))
            .collect()
    };

    let trained_day = |d: &DailyMetrics| days.contains_key(&d.date);
    let rested_day = |d: &DailyMetrics| !days.contains_key(&d.date);
    let hard_day = |d: &DailyMetrics| {
        days.get(&d.date)
            .map(|list| list.iter().map(|a| hard_secs(a)).sum::<f64>() > HARD_DAY_SECS)
            .unwrap_or(false)
    };

    let stress = |p: &dyn Fn(&DailyMetrics) -> bool| collect(p, &|d| d.stress_avg);
    let sleep = |p: &dyn Fn(&DailyMetrics) -> bool| collect(p, &|d| d.sleep_score);

    let (ts, rs) = (stress(&trained_day), stress(&rested_day));
    let (tsl, rsl) = (sleep(&trained_day), sleep(&rested_day));

    let stress_gap = stats::mean_difference(&rs, &ts, 25)?;
    let sleep_gap = stats::mean_difference(&tsl, &rsl, 25)?;
    // Both differences have to point the same way *and* be differences the
    // resampling supports.
    if !stress_gap.excludes_zero() || !sleep_gap.excludes_zero() {
        return None;
    }
    if stress_gap.value <= 0.0 || sleep_gap.value <= 0.0 {
        return None;
    }

    let hard_stress = stress(&hard_day);
    let hard_sleep = sleep(&hard_day);

    let mut out_rows = vec![FindingRow {
        label: "Training day".into(),
        value: format!("{:.0} stress", avg(&ts)?),
        note: Some(format!(
            "{} days · sleep score {:.0}",
            ts.len(),
            avg(&tsl).unwrap_or(0.0)
        )),
        accent: false,
    }];
    if hard_stress.len() >= 15 {
        out_rows.push(FindingRow {
            label: "Hard training day".into(),
            value: format!("{:.0} stress", avg(&hard_stress)?),
            note: Some(format!(
                "{} days · sleep score {:.0}",
                hard_stress.len(),
                avg(&hard_sleep).unwrap_or(0.0)
            )),
            accent: false,
        });
    }
    out_rows.push(FindingRow {
        label: "Rest day".into(),
        value: format!("{:.0} stress", avg(&rs)?),
        note: Some(format!(
            "{} days · sleep score {:.0}",
            rs.len(),
            avg(&rsl).unwrap_or(0.0)
        )),
        accent: true,
    });

    Some(Finding {
        kind: "rest-day-contrast".into(),
        section: Section::Patterns,
        tone: Tone::Note,
        claim: "Your rest days are the stressed ones, and you sleep worse on them.".into(),
        detail: format!(
            "On the {} days you trained, average stress was {:.0} and sleep score {:.0}. On the {} \
             days you didn't, stress averaged {:.0} and sleep scored {:.0}. Both gaps survive \
             resampling: stress is {:.1} to {:.1} points higher on rest days, and sleep scores {:.1} \
             to {:.1} points worse. The arrow could still run either way — training may be settling \
             you, or the days you skip may be the busy ones that were always going to score badly. \
             Either reading argues against treating a rest day as free.",
            ts.len(),
            avg(&ts)?,
            avg(&tsl)?,
            rs.len(),
            avg(&rs)?,
            avg(&rsl)?,
            stress_gap.low,
            stress_gap.high,
            sleep_gap.low,
            sleep_gap.high,
        ),
        basis: format!(
            "{} training days against {} rest days · both gaps' 90% intervals exclude zero",
            ts.len(),
            rs.len()
        ),
        estimate: Some(stress_gap),
        series: vec![],
        labels: vec![],
        rows: out_rows,
    })
}

/// The 80/20 line, month by month.
///
/// The single thread this whole account is being coached on. Time above Z2 as a
/// share of run time, per calendar month, so drift is visible as a direction
/// rather than as one bad session.
pub fn easy_share_trend(activities: &[CachedActivity]) -> Option<Finding> {
    let mut months: std::collections::BTreeMap<String, (f64, f64)> = Default::default();
    for a in activities {
        if !crate::query::is_run(a) {
            continue;
        }
        let Some(date) = &a.local_date else { continue };
        let total: f64 = a.zone_secs.iter().sum();
        if total <= 0.0 {
            continue;
        }
        let key = date.get(..7)?.to_string();
        let e = months.entry(key).or_insert((0.0, 0.0));
        e.0 += a.zone_secs[0] + a.zone_secs[1];
        e.1 += total;
    }

    // Ten minutes of run time is the floor for a month to get a point; below
    // that one warm-up decides the month's percentage.
    let ordered: Vec<(String, (f64, f64))> = months
        .into_iter()
        .filter(|(_, (_, total))| *total >= 600.0)
        .collect();
    if ordered.len() < 4 {
        return None;
    }

    let share: Vec<f64> = ordered
        .iter()
        .map(|(_, (easy, total))| easy / total * 100.0)
        .collect();
    let xs: Vec<f64> = (0..share.len()).map(|i| i as f64).collect();
    let fit = stats::linear_fit(&xs, &share, 4);

    let now = *share.last()?;
    let latest = ordered.last()?.0.clone();
    let current = latest == crate::today().format("%Y-%m").to_string();
    let named = if current {
        "your run time this month has been".to_string()
    } else {
        format!("{}'s run time was", month_short(&latest))
    };

    // A direction only gets named when the fit supports one. Otherwise the
    // finding is still worth showing — the level matters even when the trend
    // doesn't — but it says so.
    let directional = fit.as_ref().filter(|f| f.slope.excludes_zero());
    let rising = directional.map(|f| f.slope.value > 0.0);

    Some(Finding {
        kind: "easy-share-trend".into(),
        section: Section::Patterns,
        tone: if now >= 60.0 {
            Tone::Good
        } else if rising == Some(true) {
            Tone::Note
        } else {
            Tone::Watch
        },
        claim: match rising {
            Some(true) => format!(
                "Your easy share is climbing — {:.0}% of {} in Z1–Z2.",
                now, named
            ),
            Some(false) => format!(
                "Your easy share is falling — {:.0}% of {} in Z1–Z2.",
                now, named
            ),
            None => format!("{:.0}% of {} in Z1–Z2, with no clear trend.", now, named),
        },
        detail: format!(
            "Time in Z1–Z2 as a share of all run time, by month: {}. {} The 80/20 model wants 80 \
             here, and {} — which is a statement about where the hours go, not about whether the \
             hard sessions are wrong. Two short hard runs plus one genuinely easy half-hour lands \
             near 60% on its own.",
            ordered
                .iter()
                .zip(&share)
                .map(|((m, _), s)| format!("{} {:.0}%", month_short(m), s))
                .collect::<Vec<_>>()
                .join(", "),
            match directional {
                Some(f) => format!(
                    "The fitted line moves {:.1} points a month and holds its sign through \
                     resampling ({:.1} to {:.1}).",
                    f.slope.value, f.slope.low, f.slope.high
                ),
                None => "Month to month the line has no direction the data will support — the \
                         level is the thing to read, not the slope."
                    .to_string(),
            },
            if now >= 60.0 {
                "you are within reach of it".to_string()
            } else {
                format!("{:.0}% is a long way from it", now)
            },
        ),
        basis: format!(
            "{} months with at least 10 minutes of run time · {} → {}",
            ordered.len(),
            ordered.first()?.0,
            latest
        ),
        estimate: directional.map(|f| f.slope),
        series: vec![FindingSeries {
            name: "Easy share".into(),
            values: share.iter().map(|s| Some(*s)).collect(),
            format: Unit::Pct,
            muted: false,
            invert: false,
        }],
        labels: ordered.iter().map(|(m, _)| month_short(m)).collect(),
        rows: vec![],
    })
}

/// The training block that stopped.
///
/// Nothing else in this app notices this. The coach reasons about the current
/// week against its goals, and `attention` reasons about the last few days —
/// both of which describe a gap as "you haven't run this week", which is what
/// they would say in a normal easy week too. Neither can see that a month of
/// consistent work ended and nothing replaced it.
///
/// It is the most useful thing this data currently has to say and it was
/// missing, so it is new here rather than ported.
pub fn block_ended(activities: &[CachedActivity], today: chrono::NaiveDate) -> Option<Finding> {
    let runs = scored_runs(activities);
    if runs.len() < 4 {
        return None;
    }

    let last = runs.last()?;
    let last_date = last.local_date.clone()?;
    let idle = days_between(&last_date, &today.format("%Y-%m-%d").to_string());
    // Under a fortnight is a quiet spell, not a stopped block. Two weeks is
    // where the aerobic adaptations from a short block start coming off.
    if idle < 14 {
        return None;
    }

    // The block: everything inside the eight weeks before that last run. Eight
    // weeks is long enough to hold a build and short enough that a single run
    // last spring doesn't count as part of it.
    let block_start = parse(&last_date)? - chrono::Duration::days(56);
    let block: Vec<&&CachedActivity> = runs
        .iter()
        .filter(|a| {
            a.local_date
                .as_deref()
                .and_then(parse)
                .map(|d| d >= block_start)
                .unwrap_or(false)
        })
        .collect();
    // Three runs in eight weeks is not a block that ended, it is a habit that
    // never started, and saying "your block stopped" about it would be unkind
    // and wrong.
    if block.len() < 4 {
        return None;
    }

    let first_date = block.first()?.local_date.clone()?;
    let span_days = days_between(&first_date, &last_date).max(1) as f64;
    let weeks = (span_days / 7.0).max(1.0);
    let per_week = block.len() as f64 / weeks;
    let mins: f64 = block
        .iter()
        .map(|a| a.duration_s.unwrap_or(0.0) / 60.0)
        .sum();

    Some(Finding {
        kind: "block-ended".into(),
        section: Section::Patterns,
        tone: Tone::Watch,
        claim: format!("Your running block stopped {} days ago.", idle),
        detail: format!(
            "Between {} and {} you ran {} times — about {:.1} a week, {:.0} minutes in total. \
             Since then, nothing: {} days. This is not a bad week, it is the end of a block, and \
             the difference matters because the fitness those {} runs bought comes off faster than \
             it went on. Nothing here says why, and the honest options include a good one — a \
             deliberate break is a training decision. But the app had no way to tell you this was \
             happening, and every other number on these screens is still describing the block as \
             though it were current. Coming back, start under where you left off; two weeks idle \
             costs more than it feels like it should.",
            first_date,
            last_date,
            block.len(),
            per_week,
            mins,
            idle,
            block.len(),
        ),
        basis: format!(
            "{} runs over {:.0} days, then {} days idle",
            block.len(),
            span_days,
            idle
        ),
        estimate: None,
        series: vec![],
        labels: vec![],
        rows: vec![
            FindingRow {
                label: "Block".into(),
                value: format!("{} runs", block.len()),
                note: Some(format!("{first_date} → {last_date}")),
                accent: false,
            },
            FindingRow {
                label: "Since".into(),
                value: format!("{idle} days"),
                note: Some("no run recorded".into()),
                accent: true,
            },
        ],
    })
}

/// Everything worth saying, in the order it should be read.
pub fn all(
    daily: &[DailyMetrics],
    activities: &[CachedActivity],
    today: chrono::NaiveDate,
) -> Vec<Finding> {
    [
        block_ended(activities, today),
        fitness_at_fixed_hr(activities),
        cadence_lever(activities),
        recovery_drivers(daily, activities),
        easy_share_trend(activities),
        week_shape(daily, activities),
        rest_day_contrast(daily, activities),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(n: i64) -> String {
        (chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(n))
            .format("%Y-%m-%d")
            .to_string()
    }

    /// A treadmill run on day `n`, `mins` long, covering `km`, at `hr`.
    fn run(n: i64, mins: f64, km: f64, hr: f64) -> CachedActivity {
        CachedActivity {
            activity_id: 1000 + n,
            name: Some("Run".into()),
            type_key: Some("treadmill_running".into()),
            start_time_local: Some(format!("{} 10:00:00", day(n))),
            local_date: Some(day(n)),
            distance_m: Some(km * 1000.0),
            duration_s: Some(mins * 60.0),
            moving_duration_s: None,
            avg_hr: Some(hr),
            max_hr: Some(hr + 15.0),
            avg_cadence: Some(150.0),
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: [0.0, mins * 60.0 * 0.6, mins * 60.0 * 0.4, 0.0, 0.0],
        }
    }

    /// The regression this port exists to fix.
    ///
    /// Twelve runs at a constant heart rate whose paces wander with no trend.
    /// The TypeScript original compared the first third against the last third
    /// and fired whenever that gap cleared 15 s/km — which this sample does,
    /// by luck. The interval is what refuses it.
    #[test]
    fn wandering_pace_at_a_fixed_heart_rate_is_not_a_fitness_finding() {
        let paces = [9.5, 8.6, 9.9, 9.1, 8.4, 9.7, 8.9, 9.3, 8.5, 9.8, 8.7, 9.0];
        let runs: Vec<CachedActivity> = paces
            .iter()
            .enumerate()
            .map(|(i, p)| {
                // 30 minutes at pace p covers 30/p kilometres.
                run(i as i64 * 9, 30.0, 30.0 / p, 150.0)
            })
            .collect();
        assert!(
            fitness_at_fixed_hr(&runs).is_none(),
            "noise must not produce a fitness claim"
        );
    }

    /// The other half: a real, monotone improvement still has to come through,
    /// or the gate is just a mute button.
    #[test]
    fn a_steady_improvement_at_a_fixed_heart_rate_is_reported() {
        let runs: Vec<CachedActivity> = (0..12)
            .map(|i| {
                // Pace improving from 10:00/km to about 8:10/km over 99 days.
                let pace = 10.0 - i as f64 * 0.17;
                run(i * 9, 30.0, 30.0 / pace, 150.0)
            })
            .collect();
        let f = fitness_at_fixed_hr(&runs).expect("a monotone gain is a finding");
        assert_eq!(f.tone, Tone::Good);
        let e = f.estimate.expect("the claim carries its interval");
        assert!(e.excludes_zero());
        assert!(e.value > 0.0, "positive means faster now");
    }

    #[test]
    fn a_band_spanning_under_two_months_is_refused_however_clean() {
        let runs: Vec<CachedActivity> = (0..12)
            .map(|i| run(i, 30.0, 30.0 / (10.0 - i as f64 * 0.17), 150.0))
            .collect();
        assert!(fitness_at_fixed_hr(&runs).is_none());
    }

    /// The finding that did not exist before: a block that stopped.
    #[test]
    fn a_month_of_running_followed_by_three_weeks_of_nothing_is_a_finding() {
        // Eight runs across four weeks, then silence.
        let runs: Vec<CachedActivity> = (0..8).map(|i| run(i * 3, 20.0, 3.0, 150.0)).collect();
        let today =
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(21 + 21);
        let f = block_ended(&runs, today).expect("a stopped block is worth saying");
        assert_eq!(f.tone, Tone::Watch);
        assert!(f.claim.contains("stopped"));
    }

    #[test]
    fn a_normal_gap_between_sessions_is_not_a_stopped_block() {
        let runs: Vec<CachedActivity> = (0..8).map(|i| run(i * 3, 20.0, 3.0, 150.0)).collect();
        // Five days after the last run — an ordinary quiet stretch.
        let today =
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(21 + 5);
        assert!(block_ended(&runs, today).is_none());
    }

    /// Saying "your block stopped" to someone who ran three times in two months
    /// would be both wrong and unkind.
    #[test]
    fn a_habit_that_never_started_is_not_a_block_that_ended() {
        let runs = vec![run(0, 20.0, 3.0, 150.0), run(30, 20.0, 3.0, 150.0)];
        let today =
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(60);
        assert!(block_ended(&runs, today).is_none());
    }

    /// An HR-less session is scored as easy minutes rather than as zero, so a
    /// week of lifting doesn't read as a week of rest.
    #[test]
    fn a_session_without_heart_rate_still_carries_load() {
        let mut lift = run(0, 45.0, 0.0, 150.0);
        lift.type_key = Some("strength_training".into());
        lift.zone_secs = [0.0; 5];
        lift.avg_hr = None;
        assert_eq!(edwards_trimp(&lift), 90.0);
    }

    #[test]
    fn zone_time_is_weighted_by_zone() {
        let mut a = run(0, 10.0, 2.0, 150.0);
        // Ten minutes, all of it in Z5.
        a.zone_secs = [0.0, 0.0, 0.0, 0.0, 600.0];
        assert_eq!(edwards_trimp(&a), 50.0);
    }

    /// `all` must never panic on an empty history — every screen calls it
    /// before the first sync has finished.
    #[test]
    fn an_empty_history_produces_no_findings_rather_than_a_panic() {
        let out = all(
            &[],
            &[],
            chrono::NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
        );
        assert!(out.is_empty());
    }
}
