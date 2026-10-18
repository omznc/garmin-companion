//! Made-up users for the fixture files and the tests.
//!
//! Every number here is invented. The files that these personas produce are
//! safe to publish. The date is fixed, so the output is stable.

use chrono::{Duration, NaiveDate};
use garmin_core::{db::Db, DailyMetrics};
use serde_json::{json, Value};

/// The day that every persona describes.
pub const SAMPLE_DAY: (i32, u32, u32) = (2026, 3, 11);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persona {
    /// A runner, with a food log and a rising resting HR.
    Sample,
    Golfer,
    Gym,
    HealthOnly,
    NewUser,
}

impl Persona {
    pub const ALL: [Persona; 5] = [
        Persona::Sample,
        Persona::Golfer,
        Persona::Gym,
        Persona::HealthOnly,
        Persona::NewUser,
    ];

    /// The persona for a command line name, such as `golfer`.
    pub fn from_name(name: &str) -> Option<Persona> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Persona::Sample => "sample",
            Persona::Golfer => "golfer",
            Persona::Gym => "gym",
            Persona::HealthOnly => "health_only",
            Persona::NewUser => "new_user",
        }
    }

    pub fn file_name(self) -> String {
        format!("today.{}.json", self.name())
    }
}

pub fn sample_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(SAMPLE_DAY.0, SAMPLE_DAY.1, SAMPLE_DAY.2).unwrap()
}

/// Fill `db` with the rows of one persona.
pub fn seed(db: &Db, persona: Persona, today: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
    let (days, rise, food) = match persona {
        Persona::Sample => (40, true, true),
        Persona::Golfer | Persona::Gym => (40, false, persona == Persona::Gym),
        Persona::HealthOnly => (40, false, false),
        Persona::NewUser => (3, false, false),
    };
    seed_daily(db, today, days, rise, food)?;
    let activities = match persona {
        Persona::Sample => runs(today),
        Persona::Golfer => golf_and_rides(today),
        Persona::Gym => strength(today),
        Persona::HealthOnly | Persona::NewUser => Vec::new(),
    };
    for a in activities {
        db.upsert_activity(&serde_json::from_value(a)?)?;
    }
    Ok(())
}

/// Recovery data for `days` days. A food log on four days when `food` is set.
/// Resting HR rises in the last three mornings when `rise` is set.
fn seed_daily(
    db: &Db,
    today: NaiveDate,
    days: i64,
    rise: bool,
    food: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    for age in 0..days {
        let date = today - Duration::days(age);
        let wobble = (age * 7 % 5) as f64;
        let mut d = DailyMetrics {
            date: date.format("%Y-%m-%d").to_string(),
            resting_hr: Some(if rise && age < 3 {
                56.0
            } else {
                50.0 + wobble / 2.0
            }),
            hrv_last_night: Some(if rise && age < 5 { 41.0 } else { 52.0 + wobble }),
            training_readiness: Some(60.0 + wobble * 4.0),
            sleep_secs: Some(25_200.0 + wobble * 900.0),
            sleep_score: Some(70.0 + wobble * 3.0),
            steps: Some(7_000 + (age * 311 % 4_000)),
            stress_avg: Some(28.0 + wobble),
            body_battery_high: Some(80.0 - wobble * 3.0),
            body_battery_low: Some(20.0 + wobble),
            total_burn_kcal: Some(2_600.0 + wobble * 40.0),
            active_kcal: Some(500.0),
            bmr_kcal: Some(2_100.0 + wobble * 40.0),
            ..DailyMetrics::default()
        };
        if food && age % 3 == 1 && age < 12 {
            d.consumed_kcal = Some(1_700.0 + wobble * 50.0);
        }
        db.upsert_daily(&d)?;
    }
    Ok(())
}

fn at(date: NaiveDate, time: &str) -> String {
    format!("{} {time}", date.format("%Y-%m-%d"))
}

/// A run every second day.
fn runs(today: NaiveDate) -> Vec<Value> {
    (0..10i64)
        .map(|n| {
            let date = today - Duration::days(n * 2);
            let hard = 300.0 + (n * 130 % 700) as f64;
            json!({
                "activityId": 9_000_000 + n,
                "activityName": "Morning Run",
                "startTimeLocal": at(date, "07:15:00"),
                "distance": 6_000.0 + (n * 450 % 3_000) as f64,
                "duration": 2_100.0 + (n * 90 % 600) as f64,
                "movingDuration": 2_050.0,
                "averageHR": 148.0,
                "maxHR": 171.0,
                "averageRunningCadenceInStepsPerMinute": 158.0 + (n % 4) as f64,
                "aerobicTrainingEffect": 3.1,
                "activityType": { "typeKey": "running" },
                "hrTimeInZone_1": 200.0,
                "hrTimeInZone_2": 1_100.0 - hard / 2.0,
                "hrTimeInZone_3": hard,
                "hrTimeInZone_4": hard / 3.0,
                "hrTimeInZone_5": 0.0,
            })
        })
        .collect()
}

/// A round of golf every fourth day, and a motorcycle ride today and
/// yesterday. The rides carry distance and heart rate, like real ones.
fn golf_and_rides(today: NaiveDate) -> Vec<Value> {
    let rounds = (0..8i64).map(|n| {
        let date = today - Duration::days(2 + n * 4);
        json!({
            "activityId": 9_100_000 + n,
            "activityName": "Golf",
            "startTimeLocal": at(date, "09:00:00"),
            "distance": 6_500.0 + (n * 300 % 1_500) as f64,
            "duration": 15_000.0 + (n * 240 % 1_800) as f64,
            "averageHR": 104.0,
            "maxHR": 138.0,
            "steps": 10_500 + (n * 410 % 2_000),
            "activityType": { "typeKey": "golf" },
            "hrTimeInZone_1": 9_000.0,
            "hrTimeInZone_2": 5_000.0,
            "hrTimeInZone_3": 800.0,
            "hrTimeInZone_4": 0.0,
            "hrTimeInZone_5": 0.0,
        })
    });
    let rides = (0..2i64).map(|n| {
        let date = today - Duration::days(n);
        json!({
            "activityId": 9_200_000 + n,
            "activityName": "Motorcycling",
            "startTimeLocal": at(date, "17:30:00"),
            "distance": 58_000.0,
            "duration": 3_600.0,
            "averageHR": 96.0,
            "maxHR": 120.0,
            "activityType": { "typeKey": "motorcycling_v2" },
            "hrTimeInZone_1": 3_000.0,
            "hrTimeInZone_2": 600.0,
            "hrTimeInZone_3": 0.0,
            "hrTimeInZone_4": 0.0,
            "hrTimeInZone_5": 0.0,
        })
    });
    rounds.chain(rides).collect()
}

/// Strength training three times a week. No distance.
fn strength(today: NaiveDate) -> Vec<Value> {
    (0..12i64)
        .map(|n| {
            let date = today - Duration::days(n * 2 + n / 3);
            json!({
                "activityId": 9_300_000 + n,
                "activityName": "Strength",
                "startTimeLocal": at(date, "18:00:00"),
                "duration": 3_300.0 + (n * 120 % 600) as f64,
                "averageHR": 118.0,
                "maxHR": 162.0,
                "aerobicTrainingEffect": 2.2,
                "anaerobicTrainingEffect": 1.4,
                "activityType": { "typeKey": "strength_training" },
                "hrTimeInZone_1": 900.0,
                "hrTimeInZone_2": 1_300.0,
                "hrTimeInZone_3": 800.0,
                "hrTimeInZone_4": 300.0,
                "hrTimeInZone_5": 0.0,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_db, today_view, TodayView};
    use std::path::Path;

    fn view(p: Persona) -> TodayView {
        let dir = std::env::temp_dir().join(format!(
            "garmin-api-persona-{}-{}-{:?}",
            std::process::id(),
            p.name(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let db = open_db(&dir.join("cache.sqlite3")).unwrap();
        seed(&db, p, sample_day()).unwrap();
        let v = today_view(&db, sample_day()).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        v
    }

    fn sections(p: Persona) -> Vec<String> {
        view(p).sections
    }

    /// Every sentence that the view writes, in one lowercase string.
    fn copy(v: &TodayView) -> String {
        let mut all = vec![
            v.lede.clone(),
            v.fuel_sentence.clone(),
            v.week.sentence.clone(),
        ];
        all.extend(v.attention.iter().map(|o| o.text.clone()));
        all.join(" ").to_lowercase()
    }

    fn assert_no_running_copy(p: Persona) {
        let text = copy(&view(p));
        for word in ["run", "z2", "pace", "80/20", "cadence", "vo2"] {
            assert!(!text.contains(word), "{} copy has {word}: {text}", p.name());
        }
    }

    #[test]
    fn the_runner_gets_the_zone_drift_strip_last() {
        assert_eq!(
            sections(Persona::Sample),
            [
                "health_tiles",
                "attention",
                "fuel",
                "last_session",
                "week",
                "zone_drift"
            ]
        );
        let v = view(Persona::Sample);
        assert!(v.last_session.unwrap().show_zone_verdict);
        assert!(v.week.total_minutes > 0.0 && v.week.total_km > 0.0);
    }

    #[test]
    fn the_golfer_sees_no_run_copy_and_no_motorcycle() {
        assert_eq!(
            sections(Persona::Golfer),
            ["health_tiles", "last_session", "week"]
        );
        assert_no_running_copy(Persona::Golfer);
        let v = view(Persona::Golfer);
        let last = v.last_session.unwrap();
        // The newest rows are rides. The newest session must be golf.
        assert_eq!(last.activity.type_key.as_deref(), Some("golf"));
        assert_eq!(last.sport_label, "Golf");
        assert!(!last.show_zone_verdict);
        assert_eq!(v.week.sessions, 2);
        assert_eq!(v.profile.active, [garmin_core::Family::Golf]);
    }

    #[test]
    fn the_gym_user_has_time_and_no_distance() {
        assert_eq!(
            sections(Persona::Gym),
            ["health_tiles", "fuel", "last_session", "week"]
        );
        assert_no_running_copy(Persona::Gym);
        let v = view(Persona::Gym);
        assert!(!v.last_session.unwrap().show_zone_verdict);
        assert!(v.week.total_minutes > 0.0);
        assert_eq!(v.week.total_km, 0.0);
        assert!(!v.week.sentence.contains("km"));
        assert_eq!(v.profile.primary, Some(garmin_core::Family::Gym));
    }

    #[test]
    fn the_health_only_user_gets_health_sections_alone() {
        assert_eq!(sections(Persona::HealthOnly), ["health_tiles"]);
        assert_no_running_copy(Persona::HealthOnly);
        let v = view(Persona::HealthOnly);
        assert!(!v.profile.tracks_activity && v.steps.is_some() && v.stress.is_some());
        assert_eq!(v.week.sentence, "No activity in the last seven days.");
    }

    #[test]
    fn the_new_user_is_flagged_and_gets_health_tiles() {
        assert_eq!(sections(Persona::NewUser), ["health_tiles"]);
        assert_no_running_copy(Persona::NewUser);
        assert!(view(Persona::NewUser).profile.new_user);
    }

    /// The committed files must match what the code writes now.
    #[test]
    fn committed_fixtures_are_current() {
        for p in Persona::ALL {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(p.file_name());
            let want = serde_json::to_string_pretty(&view(p)).unwrap() + "\n";
            let have = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("missing {}", path.display()));
            assert_eq!(
                have,
                want,
                "stale {}: run the fixture bin with --personas",
                p.name()
            );
        }
    }
}
