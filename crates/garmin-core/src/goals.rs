//! Training goals, and how the current week is going against them.
//!
//! These are the app's own, not Garmin's — the account exposes no readable
//! training goal, so there is nothing to sync against and nothing here is ever
//! written back. Stored as one JSON blob in `sync_state` rather than as a table
//! because they are a handful of scalars read together and written whole.
//!
//! The shipped defaults are the 80/20 compromise: keep the short hard sessions,
//! make one run a week a long easy one, and pick the step rate up. They are
//! defaults rather than rules — every one can be cleared, and a cleared goal
//! produces no ring and no nudge.

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::db::{CachedActivity, Db};

const KEY: &str = "training_goals";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Goals {
    /// Total training minutes across the week, all sports.
    pub weekly_minutes: Option<f64>,
    /// How many sessions of any kind.
    pub weekly_sessions: Option<u32>,
    /// The long one: minutes of running in a single session. The whole point is
    /// duration, not distance or pace.
    pub long_run_minutes: Option<f64>,
    /// Share of tracked HR time that should be Z1+Z2, across the week's runs.
    pub easy_share_pct: Option<f64>,
    /// Steps per minute to aim at on runs.
    pub cadence_spm: Option<f64>,
}

impl Default for Goals {
    fn default() -> Self {
        Self {
            weekly_minutes: None,
            weekly_sessions: None,
            long_run_minutes: Some(30.0),
            easy_share_pct: Some(80.0),
            cadence_spm: Some(170.0),
        }
    }
}

impl Goals {
    pub fn load(db: &Db) -> Result<Self> {
        Ok(db
            .sync_state(KEY)?
            .filter(|s| !s.is_empty())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default())
    }

    pub fn save(&self, db: &Db) -> Result<()> {
        db.set_sync_state(KEY, &serde_json::to_string(self)?)
    }
}

/// Monday of the week `date` falls in. Monday because that is where a training
/// week starts everywhere this app shows one, including the load chart.
pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64)
}

/// One goal and how far through it the week is.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRing {
    /// Stable identifier: `weeklyMinutes`, `longRun`, `easyShare`, `cadence`,
    /// `weeklySessions`.
    pub id: String,
    pub label: String,
    pub target: f64,
    pub actual: f64,
    /// `actual / target`, clamped to 1 for drawing. The raw ratio is
    /// recoverable from the two figures above, so nothing is lost.
    pub fraction: f64,
    pub met: bool,
    /// The unit `target` and `actual` are in, for labelling: `minutes`,
    /// `sessions`, `percent`, `spm`.
    pub unit: String,
    /// Set when the figure rests on too little to mean anything — a cadence
    /// average over one run, an easy share with no HR recorded.
    pub thin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekProgress {
    pub week_start: String,
    pub rings: Vec<GoalRing>,
    pub sessions: usize,
    pub minutes: f64,
    /// The longest single run this week, which is what the long-run goal is
    /// measured against.
    pub longest_run_minutes: f64,
    /// Z1+Z2 share across the week's runs, time-weighted, counting only runs
    /// that recorded HR. `None` when none did.
    pub easy_share_pct: Option<f64>,
    pub avg_cadence: Option<f64>,
}

/// Where the current week stands against the goals.
pub fn week_progress(
    goals: &Goals,
    activities: &[CachedActivity],
    today: NaiveDate,
) -> WeekProgress {
    let start = week_start(today);
    let start_str = start.format("%Y-%m-%d").to_string();

    let this_week: Vec<&CachedActivity> = activities
        .iter()
        .filter(|a| {
            a.local_date
                .as_deref()
                .is_some_and(|d| d >= start_str.as_str())
        })
        .collect();

    let minutes: f64 = this_week.iter().filter_map(|a| a.duration_s).sum::<f64>() / 60.0;

    let runs: Vec<&&CachedActivity> = this_week
        .iter()
        .filter(|a| crate::query::is_run(a))
        .collect();

    let longest_run_minutes = runs
        .iter()
        .filter_map(|a| a.duration_s)
        .fold(0.0f64, f64::max)
        / 60.0;

    // Time-weighted rather than an average of per-run percentages: a two-minute
    // sprint and a forty-minute jog do not get an equal vote.
    let (mut easy, mut tracked) = (0.0, 0.0);
    for a in &runs {
        let z = a.zone_secs;
        easy += z[0] + z[1];
        tracked += z.iter().sum::<f64>();
    }
    let easy_share_pct = (tracked > 0.0).then(|| easy / tracked * 100.0);

    let cadences: Vec<f64> = runs.iter().filter_map(|a| a.avg_cadence).collect();
    let avg_cadence =
        (!cadences.is_empty()).then(|| cadences.iter().sum::<f64>() / cadences.len() as f64);

    let round1 = crate::query::round1;
    let mut rings = Vec::new();
    let mut push = |id: &str,
                    label: &str,
                    unit: &str,
                    target: Option<f64>,
                    actual: Option<f64>,
                    thin: bool| {
        let (Some(target), Some(actual)) = (target, actual) else {
            return;
        };
        if target <= 0.0 {
            return;
        }
        rings.push(GoalRing {
            id: id.into(),
            label: label.into(),
            target: round1(target),
            actual: round1(actual),
            fraction: (actual / target).clamp(0.0, 1.0),
            met: actual >= target,
            unit: unit.into(),
            thin,
        });
    };

    push(
        "weeklyMinutes",
        "Training time",
        "minutes",
        goals.weekly_minutes,
        Some(minutes),
        false,
    );
    push(
        "weeklySessions",
        "Sessions",
        "sessions",
        goals.weekly_sessions.map(f64::from),
        Some(this_week.len() as f64),
        false,
    );
    push(
        "longRun",
        "Long easy run",
        "minutes",
        goals.long_run_minutes,
        Some(longest_run_minutes),
        false,
    );
    push(
        "easyShare",
        "Easy share",
        "percent",
        goals.easy_share_pct,
        easy_share_pct,
        // One short run is not a week's easy/hard balance.
        runs.len() < 2,
    );
    push(
        "cadence",
        "Cadence",
        "spm",
        goals.cadence_spm,
        avg_cadence,
        cadences.len() < 2,
    );

    WeekProgress {
        week_start: start_str,
        rings,
        sessions: this_week.len(),
        minutes: round1(minutes),
        longest_run_minutes: round1(longest_run_minutes),
        easy_share_pct: easy_share_pct.map(round1),
        avg_cadence: avg_cadence.map(round1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(
        date: &str,
        kind: &str,
        mins: f64,
        zones: [f64; 5],
        cadence: Option<f64>,
    ) -> CachedActivity {
        CachedActivity {
            activity_id: date.len() as i64 + mins as i64,
            name: None,
            type_key: Some(kind.into()),
            start_time_local: Some(format!("{date} 10:00:00")),
            local_date: Some(date.into()),
            distance_m: Some(3000.0),
            duration_s: Some(mins * 60.0),
            moving_duration_s: None,
            avg_hr: Some(140.0),
            max_hr: Some(160.0),
            avg_cadence: cadence,
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: zones,
        }
    }

    /// 2026-08-08 is a Saturday; its week starts on Monday the 3rd.
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 8).unwrap()
    }

    #[test]
    fn last_weeks_work_does_not_count_towards_this_week() {
        let acts = vec![
            act(
                "2026-08-02",
                "running",
                40.0,
                [0.0, 2400.0, 0.0, 0.0, 0.0],
                Some(170.0),
            ),
            act(
                "2026-08-05",
                "running",
                12.0,
                [0.0, 720.0, 0.0, 0.0, 0.0],
                Some(150.0),
            ),
        ];
        let p = week_progress(&Goals::default(), &acts, today());
        assert_eq!(p.week_start, "2026-08-03");
        assert_eq!(p.sessions, 1);
        assert_eq!(p.longest_run_minutes, 12.0);
    }

    #[test]
    fn the_easy_share_is_weighted_by_time_not_by_run() {
        // Two minutes entirely hard, forty minutes entirely easy. Averaging the
        // per-run percentages would call this a 50% easy week.
        let acts = vec![
            act(
                "2026-08-04",
                "running",
                2.0,
                [0.0, 0.0, 0.0, 0.0, 120.0],
                None,
            ),
            act(
                "2026-08-06",
                "running",
                40.0,
                [0.0, 2400.0, 0.0, 0.0, 0.0],
                None,
            ),
        ];
        let p = week_progress(&Goals::default(), &acts, today());
        assert!(
            p.easy_share_pct.unwrap() > 94.0,
            "got {:?}",
            p.easy_share_pct
        );
    }

    #[test]
    fn a_run_with_no_hr_leaves_the_easy_share_unknown_rather_than_zero() {
        let acts = vec![act("2026-08-06", "running", 20.0, [0.0; 5], None)];
        let p = week_progress(&Goals::default(), &acts, today());
        assert_eq!(p.easy_share_pct, None);
        assert!(!p.rings.iter().any(|r| r.id == "easyShare"));
    }

    #[test]
    fn a_cleared_goal_produces_no_ring() {
        let goals = Goals {
            long_run_minutes: None,
            ..Goals::default()
        };
        let acts = vec![act(
            "2026-08-06",
            "running",
            20.0,
            [0.0, 1200.0, 0.0, 0.0, 0.0],
            Some(165.0),
        )];
        let p = week_progress(&goals, &acts, today());
        assert!(!p.rings.iter().any(|r| r.id == "longRun"));
        assert!(p.rings.iter().any(|r| r.id == "easyShare"));
    }

    #[test]
    fn a_met_goal_is_flagged_and_the_fraction_stops_at_one() {
        let acts = vec![act(
            "2026-08-06",
            "running",
            45.0,
            [0.0, 2700.0, 0.0, 0.0, 0.0],
            None,
        )];
        let p = week_progress(&Goals::default(), &acts, today());
        let ring = p.rings.iter().find(|r| r.id == "longRun").unwrap();
        assert!(ring.met);
        assert_eq!(ring.fraction, 1.0);
        assert_eq!(ring.actual, 45.0);
    }

    #[test]
    fn strength_counts_as_a_session_but_not_as_a_run() {
        let acts = vec![act(
            "2026-08-06",
            "strength_training",
            40.0,
            [0.0, 2400.0, 0.0, 0.0, 0.0],
            None,
        )];
        let p = week_progress(&Goals::default(), &acts, today());
        assert_eq!(p.sessions, 1);
        assert_eq!(p.longest_run_minutes, 0.0);
        assert_eq!(p.easy_share_pct, None);
    }
}
