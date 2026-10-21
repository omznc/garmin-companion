//! The Insights, Fitness and Reports views of the made-up users.
//!
//! One file for each persona holds the three views. A UI in sample mode reads
//! the file. The extra rows here (a longer history, Garmin's training status
//! and the records) are made up, like every other persona row.

use crate::fitness::{fitness_view, FitnessView};
use crate::insights::{insights_view_seen, InsightsView};
use crate::personas::Persona;
use crate::reports::{report_view, ReportView};
use crate::ApiError;
use chrono::{Duration, NaiveDate};
use garmin_core::{Db, PersonalRecord, RacePredictions, TrainingStatus};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingViews {
    pub insights: InsightsView,
    pub fitness: FitnessView,
    /// One view for each week, newest first. A UI picks one by its offset.
    pub reports: Vec<ReportView>,
}

impl Persona {
    pub fn training_file_name(self) -> String {
        format!("training.{}.json", self.name())
    }
}

/// Older runs with a pace that improves at a steady heart rate.
fn older_runs(db: &Db, today: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
    for n in 0..24i64 {
        let date = today - Duration::days(24 + n * 6);
        let pace_min = 5.9 + n as f64 * 0.06 + (n * 7 % 3) as f64 * 0.04;
        let km = 6.0 + (n % 3) as f64;
        let secs = km * pace_min * 60.0;
        db.upsert_activity(&serde_json::from_value(json!({
            "activityId": 9_000_100 + n,
            "activityName": "Easy Run",
            "startTimeLocal": format!("{} 07:30:00", date.format("%Y-%m-%d")),
            "distance": km * 1000.0,
            "duration": secs,
            "movingDuration": secs,
            "averageHR": 147.0 + (n % 3) as f64,
            "maxHR": 168.0,
            "averageRunningCadenceInStepsPerMinute": 160.0,
            "activityType": { "typeKey": "running" },
            "hrTimeInZone_1": 200.0,
            "hrTimeInZone_2": secs * 0.6,
            "hrTimeInZone_3": secs * 0.3,
            "hrTimeInZone_4": secs * 0.05,
            "hrTimeInZone_5": 0.0,
        }))?)?;
    }
    Ok(())
}

fn record(id: i64, type_id: i64, value: f64, date: &str) -> PersonalRecord {
    let (label, unit) = garmin_core::records::record_label(type_id).unzip();
    PersonalRecord {
        record_id: id,
        type_id,
        label: label.map(String::from),
        unit,
        value,
        activity_id: None,
        activity_name: None,
        activity_type: None,
        set_on: Some(date.into()),
    }
}

fn status(vo2: Option<f64>) -> TrainingStatus {
    TrainingStatus {
        status_phrase: Some("PRODUCTIVE_1".into()),
        acute_load: Some(412.0),
        chronic_load: Some(368.0),
        acwr: Some(1.12),
        acwr_status: Some("OPTIMAL".into()),
        aerobic_low: Some(210.0),
        aerobic_low_target_min: Some(150.0),
        aerobic_low_target_max: Some(280.0),
        aerobic_high: Some(190.0),
        aerobic_high_target_min: Some(120.0),
        aerobic_high_target_max: Some(250.0),
        anaerobic: Some(95.0),
        anaerobic_target_min: Some(20.0),
        anaerobic_target_max: Some(80.0),
        balance_phrase: Some("BALANCED".into()),
        vo2max: vo2,
        ..TrainingStatus::default()
    }
}

/// Add the rows that the three views need, on top of `personas::seed`.
pub fn seed_training(
    db: &Db,
    persona: Persona,
    today: NaiveDate,
) -> Result<(), Box<dyn std::error::Error>> {
    let date = today.format("%Y-%m-%d").to_string();
    let steps = [(12, 17_840.0), (13, 71_200.0), (15, 9.0)];
    let mut records: Vec<PersonalRecord> = steps
        .iter()
        .enumerate()
        .map(|(i, (t, v))| record(500 + i as i64, *t, *v, "2026-01-18"))
        .collect();
    match persona {
        Persona::Sample => {
            older_runs(db, today)?;
            let predictions = RacePredictions {
                time_5k_s: Some(1_475.0),
                time_10k_s: Some(3_080.0),
                time_half_s: Some(6_900.0),
                time_marathon_s: Some(14_700.0),
                ..RacePredictions::default()
            };
            db.upsert_fitness_day(&date, &status(Some(51.0)), &predictions)?;
            records.push(record(600, 3, 1_452.0, "2026-02-02"));
            records.push(record(601, 7, 14_200.0, "2026-01-11"));
        }
        Persona::Golfer | Persona::Gym => {
            db.upsert_fitness_day(&date, &status(None), &RacePredictions::default())?;
        }
        Persona::HealthOnly => {}
        Persona::NewUser => records.clear(),
    }
    for r in &records {
        db.upsert_personal_record(r)?;
    }
    Ok(())
}

/// The three views for `today`, and every week of the report.
pub fn training_views(db: &Db, today: NaiveDate) -> Result<TrainingViews, ApiError> {
    let first = report_view(db, today, 0)?;
    let mut reports = vec![first.clone()];
    for offset in 1..first.weeks {
        reports.push(report_view(db, today, offset)?);
    }
    Ok(TrainingViews {
        insights: insights_view_seen(db, today, today)?,
        fitness: fitness_view(db, today)?,
        reports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personas::{sample_day, seed};

    fn views(persona: Persona) -> TrainingViews {
        let dir = std::env::temp_dir().join(format!(
            "garmin-api-training-{}-{}",
            persona.name(),
            std::process::id()
        ));
        let db = crate::open_db(&dir.join("cache.sqlite3")).unwrap();
        seed(&db, persona, sample_day()).unwrap();
        seed_training(&db, persona, sample_day()).unwrap();
        let out = training_views(&db, sample_day()).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        out
    }

    fn text(v: &TrainingViews) -> String {
        serde_json::to_string(v).unwrap()
    }

    #[test]
    fn committed_fixtures_are_current() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        for p in Persona::ALL {
            let want = serde_json::to_string_pretty(&views(p)).unwrap() + "\n";
            let have =
                std::fs::read_to_string(dir.join(p.training_file_name())).unwrap_or_default();
            assert_eq!(
                have,
                want,
                "stale training file for {}: run the fixture bin with --training",
                p.name()
            );
        }
    }

    #[test]
    fn the_runner_sees_running_analysis() {
        let v = views(Persona::Sample);
        assert!(v.fitness.predictions.is_some());
        assert!(v.fitness.records.iter().any(|r| r.label == "Fastest 5 km"));
        assert!(v.reports[0].figures.iter().any(|f| f.label == "Distance"));
        assert!(v.reports.len() > 1 && v.reports[0].has_earlier && !v.reports[0].has_later);
    }

    #[test]
    fn nobody_else_sees_running_analysis() {
        for persona in [
            Persona::Golfer,
            Persona::Gym,
            Persona::HealthOnly,
            Persona::NewUser,
        ] {
            let v = views(persona);
            let all = text(&v);
            assert!(v.fitness.predictions.is_none(), "{persona:?}");
            assert!(!all.contains("Fastest 5 km"), "{persona:?}");
            assert!(!all.contains("Z2"), "{persona:?}");
            assert!(!all.contains("VO2 max"), "{persona:?}");
            assert!(!all.contains("cadence"), "{persona:?}");
            assert!(!all.contains("80/20"), "{persona:?}");
        }
    }

    #[test]
    fn a_new_user_gets_empty_states_that_say_why() {
        let v = views(Persona::NewUser);
        assert!(v.insights.empty.is_some() && v.insights.sections.is_empty());
        assert!(v.fitness.empty.is_some());
        assert!(v.reports[0].empty.is_some());
    }

    #[test]
    fn health_only_users_still_get_their_step_records() {
        let v = views(Persona::HealthOnly);
        assert!(v.fitness.empty.is_none());
        assert!(v
            .fitness
            .records
            .iter()
            .any(|r| r.label.starts_with("Most steps")));
        assert!(v.reports[0].empty.is_some());
    }
}
