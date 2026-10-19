//! The Plan screen as a view model.
//!
//! Garmin holds no training plan and no goal event for most accounts. The honest
//! plan is the set of structured workouts that the user built. The screen sets
//! those against the sessions of the same sport in the last 28 days.
//! The code reads the cache. It never calls Garmin.

use crate::body::EmptyNote;
use crate::derive::date_key;
use crate::places::{km_text, Tile};
use crate::text::hours_minutes;
use crate::ApiError;
use chrono::NaiveDate;
use garmin_core::db::Workout;
use garmin_core::sport::{is_exercise, sport_label};
use garmin_core::{CachedActivity, Db};
use serde::{Deserialize, Serialize};

/// The window that the follow-through sentence looks at.
pub const WINDOW_DAYS: u32 = 28;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkoutRow {
    pub workout_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub sport: String,
    /// The planned time, or the planned distance. A strength workout has neither.
    pub size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub empty: Option<EmptyNote>,
    pub window_days: u32,
    pub tiles: Vec<Tile>,
    /// How the longest saved workout compares with what the user did.
    pub sentence: Option<String>,
    pub workouts: Vec<WorkoutRow>,
    /// What Garmin does not store, so the app does not invent it.
    pub gap_note: String,
}

/// The sport of a type key, reduced to a stem, so `treadmill_running` and
/// `running` match and `cycling` does not match `running`.
fn stem(key: &str) -> String {
    let stems = [
        ("run", "run"),
        ("walk", "walk"),
        ("hik", "hike"),
        ("cycl", "ride"),
        ("bik", "ride"),
        ("ride", "ride"),
        ("swim", "swim"),
        ("strength", "strength"),
        ("golf", "golf"),
        ("yoga", "yoga"),
        ("cardio", "cardio"),
    ];
    stems
        .iter()
        .find(|(part, _)| key.contains(part))
        .map_or_else(|| key.to_string(), |(_, s)| s.to_string())
}

fn same_sport(workout: &Workout, a: &CachedActivity) -> bool {
    match (&workout.sport_type, &a.type_key) {
        (Some(w), Some(k)) => stem(w) == stem(k),
        _ => false,
    }
}

fn size_of(w: &Workout) -> Option<String> {
    // A strength workout carries a zero estimate, not a real one.
    match (w.est_duration_s, w.est_distance_m) {
        (Some(s), _) if s > 0.0 => Some(hours_minutes(s)),
        (_, Some(m)) if m > 0.0 => Some(km_text(m)),
        _ => None,
    }
}

fn row_of(w: &Workout) -> WorkoutRow {
    WorkoutRow {
        workout_id: w.workout_id,
        name: w.name.clone().unwrap_or_else(|| "Untitled workout".into()),
        description: w.description.clone().filter(|d| !d.trim().is_empty()),
        sport: w
            .sport_type
            .as_deref()
            .map_or_else(|| "Workout".into(), |k| sport_label(Some(k))),
        size: size_of(w),
    }
}

fn empty_note() -> EmptyNote {
    EmptyNote {
        title: "No workouts saved on your Garmin account.".into(),
        body: "Garmin holds no training plan for you. A plan here comes from the structured \
               workouts that you build in Garmin Connect, for any sport. You have none yet. \
               Create one, and it appears here after the next sync."
            .into(),
    }
}

/// The sentence about the saved workout with the longest planned time.
fn follow_through(saved: &[Workout], recent: &[&CachedActivity]) -> Option<String> {
    let longest = saved
        .iter()
        .filter(|w| w.est_duration_s.unwrap_or(0.0) > 0.0)
        .max_by(|a, b| a.est_duration_s.partial_cmp(&b.est_duration_s).unwrap())?;
    let ask = longest.est_duration_s?;
    let sport = sport_label(longest.sport_type.as_deref()).to_lowercase();
    let best = recent
        .iter()
        .filter(|a| same_sport(longest, a))
        .filter_map(|a| a.duration_s)
        .fold(0.0, f64::max);
    let head = format!(
        "Your longest saved {sport} workout asks for {}.",
        hours_minutes(ask)
    );
    let tail = if best <= 0.0 {
        format!("You have no {sport} session in the last {WINDOW_DAYS} days.")
    } else if best >= ask {
        format!(
            "Your longest {sport} session in the last {WINDOW_DAYS} days was {}. You cover it.",
            hours_minutes(best)
        )
    } else {
        format!(
            "Your longest {sport} session in the last {WINDOW_DAYS} days was {}, which is {}% short of it. That gap is the plan.",
            hours_minutes(best),
            ((1.0 - best / ask) * 100.0).round()
        )
    };
    Some(format!("{head} {tail}"))
}

pub fn plan_view(db: &Db, today: NaiveDate) -> Result<PlanView, ApiError> {
    let saved = db.workouts()?;
    let from = date_key(today - chrono::Duration::days(i64::from(WINDOW_DAYS) - 1));
    let all = db.activities_since(&from)?;
    let recent: Vec<&CachedActivity> = all
        .iter()
        .filter(|a| is_exercise(a.type_key.as_deref()))
        .collect();
    let gap_note = "Garmin stores no goal event and no weekly plan for your account. \
                    This app does not invent them. This screen shows the workouts that Garmin holds."
        .to_string();
    if saved.is_empty() {
        return Ok(PlanView {
            empty: Some(empty_note()),
            window_days: WINDOW_DAYS,
            tiles: Vec::new(),
            sentence: None,
            workouts: Vec::new(),
            gap_note,
        });
    }
    let longest = recent
        .iter()
        .filter_map(|a| a.duration_s)
        .fold(0.0, f64::max);
    let mut tiles = vec![
        Tile::new("Saved workouts", saved.len().to_string()),
        Tile::new(
            format!("Sessions in {WINDOW_DAYS} days"),
            recent.len().to_string(),
        ),
    ];
    if longest > 0.0 {
        tiles.push(Tile::new("Longest session", hours_minutes(longest)));
    }
    Ok(PlanView {
        empty: None,
        window_days: WINDOW_DAYS,
        tiles,
        sentence: follow_through(&saved, &recent),
        workouts: saved.iter().map(row_of).collect(),
        gap_note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workout(sport: &str, secs: f64) -> Workout {
        Workout {
            workout_id: 1,
            name: Some("W".into()),
            sport_type: Some(sport.into()),
            description: None,
            est_duration_s: Some(secs),
            est_distance_m: None,
            updated_at: None,
            raw: None,
        }
    }

    fn activity(key: &str, secs: f64) -> CachedActivity {
        serde_json::from_value(serde_json::json!({
            "activityId": 1, "typeKey": key, "durationS": secs, "zoneSecs": [0, 0, 0, 0, 0],
        }))
        .unwrap()
    }

    #[test]
    fn sports_match_by_stem_not_by_family() {
        assert_eq!(stem("treadmill_running"), stem("running"));
        assert_ne!(stem("cycling"), stem("running"));
        assert_eq!(stem("indoor_cycling"), stem("road_biking"));
    }

    #[test]
    fn the_sentence_says_how_far_short_the_user_is() {
        let saved = [workout("cycling", 7_200.0), workout("running", 1_800.0)];
        let ride = activity("road_biking", 3_600.0);
        let run = activity("running", 9_000.0);
        let s = follow_through(&saved, &[&ride, &run]).unwrap();
        assert!(s.contains("saved cycling workout asks for 2h 00m"), "{s}");
        assert!(s.contains("50% short"), "{s}");
    }

    #[test]
    fn the_sentence_handles_no_session_and_a_covered_workout() {
        let saved = [workout("walking", 3_600.0)];
        let none = follow_through(&saved, &[]).unwrap();
        assert!(none.contains("no walking session"), "{none}");
        let walk = activity("walking", 4_000.0);
        assert!(follow_through(&saved, &[&walk])
            .unwrap()
            .contains("You cover it"));
        // A strength workout has no planned time, so it makes no sentence.
        assert!(follow_through(&[workout("strength_training", 0.0)], &[]).is_none());
    }

    #[test]
    fn a_strength_workout_has_no_size() {
        assert_eq!(row_of(&workout("strength_training", 0.0)).size, None);
        assert_eq!(row_of(&workout("running", 2_700.0)).size.unwrap(), "0h 45m");
    }
}
