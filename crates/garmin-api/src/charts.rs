//! Totals for the chart blocks: distance, time, strength sets and the month
//! against the last month.
//!
//! Every function takes rows and a day, so a test needs no database. A sport
//! word appears only when the rows hold that sport: the families come from the
//! activities themselves.

use crate::derive::{above_z2_pct, date_key, has_zone_data, mean};
use crate::ApiError;
use chrono::{Datelike, Duration, NaiveDate};
use garmin_core::sport::{family, is_exercise, sport_label};
use garmin_core::{CachedActivity, Db, ExerciseSet, Family};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Months in the distance chart.
const MONTHS: u32 = 12;
/// Days in the time by sport block and in the muscle block.
const WINDOW_DAYS: i64 = 30;
/// Weeks in the sets per week chart.
const WEEKS: i64 = 12;
/// A session counts as easy when at most this share of its time is above Z2.
const EASY_ABOVE_Z2_PCT: f64 = 20.0;
/// How many strength sessions the view reads sets for.
const SESSION_LIMIT: u32 = 400;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FamilyKm {
    pub family: Family,
    pub label: String,
    pub km: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct MonthDistance {
    /// `YYYY-MM`.
    pub month: String,
    pub total_km: f64,
    /// Only the families with distance in the month, largest first.
    pub families: Vec<FamilyKm>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SportTime {
    pub family: Family,
    /// The sport with the most time in the family, for example "Running".
    pub label: String,
    pub secs: f64,
    pub share_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WeekSets {
    /// The Monday of the week, `YYYY-MM-DD`.
    pub week_start: String,
    /// `2026-W11`.
    pub iso_week: String,
    pub sets: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct MuscleSets {
    pub group: String,
    pub sets: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct MuscleReport {
    /// Largest first. Only the groups with a set.
    pub groups: Vec<MuscleSets>,
    /// Sets in the window that the watch could not name, or that no group
    /// takes. They are left out of `groups`.
    pub unassigned_sets: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct MonthStats {
    pub sessions: usize,
    pub distance_km: f64,
    pub secs: f64,
    /// Mean of the average heart rate of the easy sessions. `None` with no
    /// easy session that has a heart rate.
    pub easy_avg_hr: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct MonthCompare {
    /// The day of the month that both sides run to.
    pub through_day: u32,
    pub this_month: MonthStats,
    pub last_month: MonthStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ChartsView {
    pub date: String,
    pub distance_by_month: Vec<MonthDistance>,
    pub time_by_sport: Vec<SportTime>,
    pub sets_per_week: Vec<WeekSets>,
    pub sets_by_muscle: MuscleReport,
    pub month_compare: MonthCompare,
}

/// A readable name for a family, for a legend.
pub fn family_label(f: Family) -> &'static str {
    match f {
        Family::Endurance => "Endurance",
        Family::Gym => "Gym",
        Family::Golf => "Golf",
        Family::Water => "Water",
        Family::MindBody => "Mind and body",
        Family::Mixed => "Other",
        Family::NonExercise => "Not exercise",
    }
}

fn day_of(a: &CachedActivity) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(a.local_date.as_deref()?.get(..10)?, "%Y-%m-%d").ok()
}

fn month_start(today: NaiveDate, back: u32) -> NaiveDate {
    let index = today.year() * 12 + today.month0() as i32 - back as i32;
    NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1)
        .unwrap_or(NaiveDate::MIN)
}

/// Distance for each of the last 12 months, by family, oldest month first.
/// A month with no distance stays in the list with a zero total.
pub fn distance_by_month(activities: &[CachedActivity], today: NaiveDate) -> Vec<MonthDistance> {
    let mut totals: HashMap<(i32, u32), HashMap<Family, f64>> = HashMap::new();
    for a in activities
        .iter()
        .filter(|a| is_exercise(a.type_key.as_deref()))
    {
        let (Some(d), Some(m)) = (day_of(a), a.distance_m.filter(|m| *m > 0.0)) else {
            continue;
        };
        *totals
            .entry((d.year(), d.month()))
            .or_default()
            .entry(family(a.type_key.as_deref()))
            .or_default() += m / 1000.0;
    }
    (0..MONTHS)
        .rev()
        .map(|back| {
            let start = month_start(today, back);
            let mut families: Vec<FamilyKm> = totals
                .remove(&(start.year(), start.month()))
                .unwrap_or_default()
                .into_iter()
                .map(|(family, km)| FamilyKm {
                    family,
                    label: family_label(family).into(),
                    km,
                })
                .collect();
            families.sort_by(|a, b| b.km.total_cmp(&a.km).then(a.family.cmp(&b.family)));
            MonthDistance {
                month: start.format("%Y-%m").to_string(),
                total_km: families.iter().map(|f| f.km).sum(),
                families,
            }
        })
        .collect()
}

/// Time for each family over the last 30 days, largest first. The label is the
/// sport with the most time in the family.
pub fn time_by_sport(activities: &[CachedActivity], today: NaiveDate) -> Vec<SportTime> {
    let cutoff = date_key(today - Duration::days(WINDOW_DAYS - 1));
    let mut by_family: HashMap<Family, HashMap<String, f64>> = HashMap::new();
    for a in activities {
        let key = a.type_key.as_deref();
        let secs = a.duration_s.unwrap_or(0.0);
        let recent = a
            .local_date
            .as_deref()
            .is_some_and(|d| d >= cutoff.as_str());
        if recent && secs > 0.0 && is_exercise(key) {
            *by_family
                .entry(family(key))
                .or_default()
                .entry(sport_label(key))
                .or_default() += secs;
        }
    }
    let total: f64 = by_family.values().flat_map(|m| m.values()).sum();
    let mut out: Vec<SportTime> = by_family
        .into_iter()
        .map(|(family, sports)| {
            let secs = sports.values().sum();
            let label = sports
                .into_iter()
                .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
                .map(|(l, _)| l)
                .unwrap_or_default();
            SportTime {
                family,
                label,
                secs,
                share_pct: secs / total * 100.0,
            }
        })
        .collect();
    out.sort_by(|a, b| b.secs.total_cmp(&a.secs).then(a.family.cmp(&b.family)));
    out
}

/// The work sets of a session, by the day it happened.
pub struct SessionSets {
    pub date: NaiveDate,
    pub sets: Vec<ExerciseSet>,
}

fn monday_of(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

/// Work sets for each of the last 12 ISO weeks, oldest first. A week with no
/// session stays in the list with zero.
pub fn sets_per_week(sessions: &[SessionSets], today: NaiveDate) -> Vec<WeekSets> {
    let this_week = monday_of(today);
    (0..WEEKS)
        .rev()
        .map(|back| {
            let start = this_week - Duration::weeks(back);
            let end = start + Duration::days(6);
            let sets = sessions
                .iter()
                .filter(|s| s.date >= start && s.date <= end)
                .flat_map(|s| &s.sets)
                .filter(|set| set.active)
                .count() as u32;
            let iso = start.iso_week();
            WeekSets {
                week_start: date_key(start),
                iso_week: format!("{}-W{:02}", iso.year(), iso.week()),
                sets,
            }
        })
        .collect()
}

/// The muscle group of a Garmin exercise category. The first row that the
/// category contains wins, so a narrow name goes above a wide one: LEG_CURL
/// is a leg exercise and not a CURL, SHOULDER_PRESS is not a PRESS.
const MUSCLE_MAP: &[(&str, &str)] = &[
    ("LEG_", "Legs"),
    ("SQUAT", "Legs"),
    ("LUNGE", "Legs"),
    ("DEADLIFT", "Legs"),
    ("CALF", "Legs"),
    ("HIP_", "Legs"),
    ("GLUTE", "Legs"),
    ("STEP_UP", "Legs"),
    ("SHOULDER", "Shoulders"),
    ("LATERAL_RAISE", "Shoulders"),
    ("FRONT_RAISE", "Shoulders"),
    ("SHRUG", "Shoulders"),
    ("ROW", "Back"),
    ("PULL_UP", "Back"),
    ("CHIN_UP", "Back"),
    ("PULLDOWN", "Back"),
    ("PULL_DOWN", "Back"),
    ("BACK_EXTENSION", "Back"),
    ("BENCH_PRESS", "Chest"),
    ("PUSH_UP", "Chest"),
    ("FLYE", "Chest"),
    ("CHEST", "Chest"),
    ("CURL", "Arms"),
    ("TRICEPS", "Arms"),
    ("BICEPS", "Arms"),
    ("DIP", "Arms"),
    ("CRUNCH", "Core"),
    ("SIT_UP", "Core"),
    ("PLANK", "Core"),
    ("CORE", "Core"),
    ("TWIST", "Core"),
];

/// The group for a Garmin exercise category, or `None` for one that the map
/// does not hold.
pub fn muscle_group(category: &str) -> Option<&'static str> {
    let c = category.to_ascii_uppercase();
    MUSCLE_MAP
        .iter()
        .find(|(part, _)| c.contains(part))
        .map(|(_, group)| *group)
}

/// Work sets by muscle group over the last 30 days. A set with no named
/// exercise, or with a category outside the map, counts as unassigned.
pub fn sets_by_muscle(sessions: &[SessionSets], today: NaiveDate) -> MuscleReport {
    let cutoff = today - Duration::days(WINDOW_DAYS - 1);
    let mut counts: HashMap<&'static str, u32> = HashMap::new();
    let mut unassigned = 0;
    let recent = sessions
        .iter()
        .filter(|s| s.date >= cutoff && s.date <= today);
    for set in recent.flat_map(|s| &s.sets).filter(|s| s.active) {
        match set.exercise.as_deref().and_then(muscle_group) {
            Some(g) => *counts.entry(g).or_default() += 1,
            None => unassigned += 1,
        }
    }
    let mut groups: Vec<MuscleSets> = counts
        .into_iter()
        .map(|(group, sets)| MuscleSets {
            group: group.into(),
            sets,
        })
        .collect();
    groups.sort_by(|a, b| b.sets.cmp(&a.sets).then(a.group.cmp(&b.group)));
    MuscleReport {
        groups,
        unassigned_sets: unassigned,
    }
}

fn month_stats(activities: &[&CachedActivity]) -> MonthStats {
    let easy_hr = activities
        .iter()
        .filter(|a| has_zone_data(a) && above_z2_pct(a) <= EASY_ABOVE_Z2_PCT)
        .map(|a| a.avg_hr.filter(|hr| *hr > 0.0));
    MonthStats {
        sessions: activities.len(),
        distance_km: activities.iter().filter_map(|a| a.distance_m).sum::<f64>() / 1000.0,
        secs: activities.iter().filter_map(|a| a.duration_s).sum(),
        easy_avg_hr: mean(easy_hr),
    }
}

/// This month through today against the same days of the last month. When the
/// last month is shorter than today's day number, it runs to its last day.
pub fn month_compare(activities: &[CachedActivity], today: NaiveDate) -> MonthCompare {
    let (this_start, last_start) = (month_start(today, 0), month_start(today, 1));
    let last_len = (this_start - last_start).num_days() as u32;
    let through = today.day();
    let pick = |start: NaiveDate, days: u32| -> Vec<&CachedActivity> {
        let end = start + Duration::days(days as i64 - 1);
        activities
            .iter()
            .filter(|a| is_exercise(a.type_key.as_deref()))
            .filter(|a| day_of(a).is_some_and(|d| d >= start && d <= end))
            .collect()
    };
    MonthCompare {
        through_day: through,
        this_month: month_stats(&pick(this_start, through)),
        last_month: month_stats(&pick(last_start, through.min(last_len))),
    }
}

/// The work sets of the strength sessions in the cache, with their days.
fn load_sessions(db: &Db) -> Result<Vec<SessionSets>, ApiError> {
    let mut out = Vec::new();
    for a in db.strength_activities(SESSION_LIMIT)? {
        let Some(date) = day_of(&a) else { continue };
        out.push(SessionSets {
            date,
            sets: db.exercise_sets(a.activity_id)?,
        });
    }
    Ok(out)
}

/// Build the chart totals for `today`.
pub fn charts_view(db: &Db, today: NaiveDate) -> Result<ChartsView, ApiError> {
    let from = date_key(month_start(today, MONTHS - 1));
    let activities = db.activities_since(&from)?;
    let sessions = load_sessions(db)?;
    Ok(ChartsView {
        date: date_key(today),
        distance_by_month: distance_by_month(&activities, today),
        time_by_sport: time_by_sport(&activities, today),
        sets_per_week: sets_per_week(&sessions, today),
        sets_by_muscle: sets_by_muscle(&sessions, today),
        month_compare: month_compare(&activities, today),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn act(date: &str, key: &str, km: f64, mins: f64) -> CachedActivity {
        CachedActivity {
            activity_id: 1,
            name: None,
            type_key: Some(key.into()),
            start_time_local: None,
            local_date: Some(date.into()),
            distance_m: Some(km * 1000.0),
            duration_s: Some(mins * 60.0),
            moving_duration_s: None,
            avg_hr: None,
            max_hr: None,
            avg_cadence: None,
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: [0.0; 5],
            training_load: None,
        }
    }

    fn easy(date: &str, hr: f64) -> CachedActivity {
        CachedActivity {
            avg_hr: Some(hr),
            zone_secs: [600.0, 2000.0, 100.0, 0.0, 0.0],
            ..act(date, "running", 8.0, 45.0)
        }
    }

    fn set(exercise: Option<&str>, active: bool) -> ExerciseSet {
        ExerciseSet {
            activity_id: 1,
            set_index: 0,
            active,
            duration_s: None,
            reps: None,
            exercise: exercise.map(str::to_owned),
            exercise_confidence: None,
            weight_kg: None,
            start_time: None,
        }
    }

    #[test]
    fn distance_by_month_keeps_twelve_months_and_splits_the_families() {
        let acts = [
            act("2026-03-02", "running", 10.0, 50.0),
            act("2026-03-09", "cycling", 30.0, 90.0),
            act("2026-03-10", "strength_training", 0.0, 40.0),
            act("2026-03-10", "lap_swimming", 1.5, 40.0),
            act("2026-03-10", "motorcycling", 80.0, 60.0),
            act("2025-04-30", "running", 5.0, 30.0),
            act("2025-03-31", "running", 99.0, 30.0),
        ];
        let got = distance_by_month(&acts, d(2026, 3, 11));
        assert_eq!(got.len(), 12);
        assert_eq!(got[0].month, "2025-04");
        assert_eq!(got[0].total_km, 5.0);
        assert_eq!(got[11].month, "2026-03");
        assert_eq!(got[11].total_km, 41.5, "no motor, no zero distance");
        let shape: Vec<_> = got[11].families.iter().map(|f| (f.family, f.km)).collect();
        assert_eq!(shape, [(Family::Endurance, 40.0), (Family::Water, 1.5)]);
        assert_eq!(got[5].total_km, 0.0);
        assert!(got[5].families.is_empty());
    }

    #[test]
    fn time_by_sport_sums_a_family_and_names_the_main_sport() {
        let acts = [
            act("2026-03-10", "running", 5.0, 30.0),
            act("2026-03-09", "walking", 3.0, 10.0),
            act("2026-03-08", "strength_training", 0.0, 60.0),
            act("2026-02-01", "running", 5.0, 600.0),
        ];
        let got = time_by_sport(&acts, d(2026, 3, 11));
        assert_eq!(got.len(), 2);
        assert_eq!(
            (got[0].family, got[0].label.as_str()),
            (Family::Gym, "Strength training")
        );
        assert_eq!((got[1].label.as_str(), got[1].secs), ("Running", 2400.0));
        assert!((got.iter().map(|g| g.share_pct).sum::<f64>() - 100.0).abs() < 1e-9);
        assert!(time_by_sport(&[], d(2026, 3, 11)).is_empty());
    }

    #[test]
    fn sets_per_week_counts_work_sets_in_iso_weeks() {
        // 2026-03-11 is a Wednesday. Its week starts on Monday 2026-03-09.
        let s = |date, sets| SessionSets { date, sets };
        let sessions = [
            s(
                d(2026, 3, 9),
                vec![set(None, true), set(None, false), set(None, true)],
            ),
            s(d(2026, 3, 1), vec![set(None, true)]),
            s(d(2025, 12, 1), vec![set(None, true)]),
        ];
        let got = sets_per_week(&sessions, d(2026, 3, 11));
        assert_eq!(got.len(), 12);
        assert_eq!(
            (
                got[11].week_start.as_str(),
                got[11].iso_week.as_str(),
                got[11].sets
            ),
            ("2026-03-09", "2026-W11", 2)
        );
        assert_eq!(got[10].sets, 0);
        assert_eq!(got[9].sets, 1, "Sunday the 1st is in the week before last");
        assert_eq!(got[0].week_start, "2025-12-22");
        assert_eq!(
            got.iter().map(|w| w.sets).sum::<u32>(),
            3,
            "December is outside 12 weeks"
        );
    }

    #[test]
    fn muscle_groups_use_the_first_match_and_unnamed_sets_are_counted_apart() {
        for (cat, group) in [
            ("LEG_CURL", "Legs"),
            ("CURL", "Arms"),
            ("SHOULDER_PRESS", "Shoulders"),
            ("BENCH_PRESS", "Chest"),
            ("bent_over_row", "Back"),
            ("CRUNCH", "Core"),
        ] {
            assert_eq!(muscle_group(cat), Some(group), "{cat}");
        }
        assert_eq!(muscle_group("TOTAL_BODY"), None);

        let sessions = [SessionSets {
            date: d(2026, 3, 10),
            sets: vec![
                set(Some("SQUAT"), true),
                set(Some("SQUAT"), true),
                set(Some("CURL"), true),
                set(Some("TOTAL_BODY"), true),
                set(None, true),
                set(Some("SQUAT"), false),
            ],
        }];
        let got = sets_by_muscle(&sessions, d(2026, 3, 11));
        assert_eq!(
            got.groups[0],
            MuscleSets {
                group: "Legs".into(),
                sets: 2
            }
        );
        assert_eq!(got.groups.len(), 2);
        assert_eq!(got.unassigned_sets, 2);
        let old = sets_by_muscle(&sessions, d(2026, 5, 1));
        assert!(old.groups.is_empty() && old.unassigned_sets == 0);
    }

    #[test]
    fn the_month_is_set_against_the_same_days_of_the_last_month() {
        let acts = [
            easy("2026-03-02", 140.0),
            easy("2026-03-10", 144.0),
            act("2026-03-12", "running", 50.0, 10.0),
            easy("2026-02-03", 150.0),
            act("2026-02-10", "cycling", 20.0, 60.0),
            act("2026-02-11", "cycling", 99.0, 60.0),
        ];
        let got = month_compare(&acts, d(2026, 3, 10));
        assert_eq!(got.through_day, 10);
        assert_eq!(got.this_month.sessions, 2);
        assert_eq!(got.this_month.distance_km, 16.0);
        assert_eq!(got.this_month.easy_avg_hr, Some(142.0));
        assert_eq!(got.last_month.sessions, 2);
        assert_eq!(got.last_month.distance_km, 28.0);
        assert_eq!(got.last_month.easy_avg_hr, Some(150.0));
        // The 31st against February: the last month runs to its last day.
        assert_eq!(month_compare(&acts, d(2026, 3, 31)).last_month.sessions, 3);
        assert_eq!(
            month_compare(&[], d(2026, 3, 10)).this_month,
            MonthStats::default()
        );
    }

    #[test]
    fn the_view_builds_from_an_empty_cache() {
        let db = Db::open(&std::env::temp_dir().join(format!(
            "garmin-api-charts-{}/cache.sqlite3",
            std::process::id()
        )))
        .unwrap();
        let v = charts_view(&db, d(2026, 3, 11)).unwrap();
        assert_eq!(v.distance_by_month.len(), 12);
        assert_eq!(v.sets_per_week.len(), 12);
        assert!(v.time_by_sport.is_empty() && v.sets_by_muscle.groups.is_empty());
    }
}
