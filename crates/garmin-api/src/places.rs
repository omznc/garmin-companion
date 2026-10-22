//! Plan, Routes and Gear together, plus the made-up rows for their fixtures.
//!
//! The three screens share a tile type and a distance format. The personas here
//! add workouts, GPS traces and gear to the rows of [`crate::personas`].

use crate::gear::{gear_view, GearEntry, GearView};
use crate::personas::Persona;
use crate::plan::{plan_view, PlanView};
use crate::routes::{routes_view, RouteSort, RoutesView};
use crate::ApiError;
use chrono::NaiveDate;
use garmin_core::db::{ActivityTrack, Workout};
use garmin_core::Db;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A figure and its caption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
pub struct Tile {
    pub label: String,
    pub value: String,
}

impl Tile {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

/// `12.4 km`. Gear and routes use kilometres, as the rest of the app does.
pub fn km_text(metres: f64) -> String {
    let km = metres / 1000.0;
    if km >= 100.0 {
        format!("{} km", crate::text::num(km))
    } else {
        format!("{km:.1} km")
    }
}

/// The three views of one user. This is the content of a `places.*.json` fixture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacesViews {
    pub plan: PlanView,
    pub routes: RoutesView,
    pub gear: GearView,
}

pub fn places_views(
    db: &Db,
    today: NaiveDate,
    gear: &[GearEntry],
) -> Result<PlacesViews, ApiError> {
    Ok(PlacesViews {
        plan: plan_view(db, today)?,
        routes: routes_view(db, RouteSort::Recent)?,
        gear: gear_view(gear),
    })
}

impl Persona {
    pub fn places_file_name(self) -> String {
        format!("places.{}.json", self.name())
    }
}

fn workout(id: i64, name: &str, sport: &str, secs: f64, note: Option<&str>) -> Workout {
    Workout {
        workout_id: id,
        name: Some(name.into()),
        sport_type: Some(sport.into()),
        description: note.map(Into::into),
        est_duration_s: Some(secs),
        est_distance_m: None,
        updated_at: Some("2026-03-01T08:00:00".into()),
        raw: None,
    }
}

/// A closed loop of 60 points near an invented place. `bend` changes the shape.
fn loop_track(activity_id: i64, bend: f64) -> ActivityTrack {
    let points: Vec<[f64; 2]> = (0..60)
        .map(|i| {
            let a = f64::from(i) / 59.0 * std::f64::consts::TAU;
            let r = 1.0 + bend * (2.0 * a).cos();
            [
                50.0 + 0.004 * r * a.sin() + 0.0005 * f64::from(i % 3),
                10.0 + 0.007 * r * a.cos() - 0.007,
            ]
        })
        .collect();
    let (first, last) = (points[0], points[59]);
    let lat = |f: fn(f64, f64) -> f64| points.iter().map(|p| p[0]).fold(first[0], f);
    let lon = |f: fn(f64, f64) -> f64| points.iter().map(|p| p[1]).fold(first[1], f);
    ActivityTrack {
        activity_id,
        point_count: 60,
        start_lat: Some(first[0]),
        start_lon: Some(first[1]),
        end_lat: Some(last[0]),
        end_lon: Some(last[1]),
        min_lat: Some(lat(f64::min)),
        max_lat: Some(lat(f64::max)),
        min_lon: Some(lon(f64::min)),
        max_lon: Some(lon(f64::max)),
        points,
        ..Default::default()
    }
}

/// Add workouts and GPS traces for the persona, on top of [`crate::personas::seed`].
pub fn seed_places(db: &Db, persona: Persona) -> Result<(), Box<dyn std::error::Error>> {
    let workouts = match persona {
        Persona::Sample => vec![
            workout(
                1,
                "Long easy run",
                "running",
                4_500.0,
                Some("Keep it in zone 2."),
            ),
            workout(2, "Tempo 3 x 10", "running", 3_300.0, None),
            workout(3, "Core and mobility", "strength_training", 0.0, None),
        ],
        Persona::Gym => vec![
            workout(1, "Full body A", "strength_training", 0.0, None),
            workout(
                2,
                "Full body B",
                "strength_training",
                0.0,
                Some("Heavy day."),
            ),
            workout(3, "Easy walk", "walking", 2_400.0, None),
        ],
        _ => Vec::new(),
    };
    for w in &workouts {
        db.upsert_workout(w)?;
    }
    // Runs 0 to 5 and rounds 0 to 7 each get a trace. Two shapes make two routes.
    let (base, count) = match persona {
        Persona::Sample => (9_000_000, 6),
        Persona::Golfer => (9_100_000, 8),
        _ => return Ok(()),
    };
    for n in 0..count {
        db.upsert_track(&loop_track(base + n, if n < 4 { 0.0 } else { 0.35 }))?;
    }
    Ok(())
}

/// The gear of a persona. A runner and a golfer both own something. A new user owns nothing.
pub fn persona_gear(persona: Persona) -> Vec<GearEntry> {
    let entry = |name: &str, kind: &str, limit: Option<f64>, km: f64, n: u64| GearEntry {
        item: serde_json::from_value(json!({
            "uuid": name, "displayName": name, "gearTypeName": kind,
            "maximumMeters": limit, "gearStatusName": "active",
        }))
        .expect("the gear item is valid"),
        distance_m: Some(km * 1000.0),
        activities: Some(n),
    };
    match persona {
        Persona::Sample => vec![
            entry("Road shoes", "SHOES", Some(800_000.0), 655.0, 91),
            entry("Trail shoes", "SHOES", Some(700_000.0), 210.0, 24),
            entry("Commuter bike", "BIKE", None, 1_840.0, 160),
        ],
        Persona::Golfer => vec![
            entry("Walking shoes", "SHOES", Some(1_000_000.0), 380.0, 55),
            entry("Golf clubs", "OTHER", None, 0.0, 33),
        ],
        Persona::Gym => vec![entry(
            "Training shoes",
            "SHOES",
            Some(600_000.0),
            590.0,
            120,
        )],
        Persona::HealthOnly | Persona::NewUser => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_db;
    use crate::personas::{sample_day, seed};

    fn views(p: Persona) -> PlacesViews {
        let dir = std::env::temp_dir().join(format!(
            "garmin-api-places-{}-{}-{:?}",
            p.name(),
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let db = open_db(&dir.join("cache.sqlite3")).unwrap();
        seed(&db, p, sample_day()).unwrap();
        seed_places(&db, p).unwrap();
        let v = places_views(&db, sample_day(), &persona_gear(p)).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        v
    }

    #[test]
    fn the_runner_has_a_gap_between_the_longest_workout_and_the_longest_run() {
        let v = views(Persona::Sample);
        assert!(v.plan.empty.is_none());
        assert_eq!(v.plan.workouts.len(), 3);
        let s = v.plan.sentence.unwrap();
        assert!(s.contains("saved running workout asks for 1h 15m"), "{s}");
        assert!(s.contains("short of it"), "{s}");
        // Two shapes at one start make two routes. The first holds four outings.
        assert_eq!(v.routes.routes.len(), 2);
        assert_eq!(v.routes.routes.iter().map(|r| r.times).max(), Some(4));
        assert!(v.routes.routes.iter().all(|r| r.trace.len() > 10));
        assert_eq!(v.gear.items.len(), 3);
    }

    #[test]
    fn the_strength_persona_gets_a_sentence_about_a_sport_it_did_not_do() {
        let v = views(Persona::Gym);
        let s = v.plan.sentence.unwrap();
        assert!(s.contains("no walking session"), "{s}");
        assert!(v.routes.empty.is_some());
    }

    #[test]
    fn the_golfer_has_routes_and_no_plan() {
        let v = views(Persona::Golfer);
        assert!(v.plan.empty.is_some());
        assert!(!v.routes.routes.is_empty());
        assert!(v.routes.notes[0].contains("golf"), "{:?}", v.routes.notes);
        assert!(!v.plan.gap_note.to_lowercase().contains("run"));
    }

    #[test]
    fn a_user_with_nothing_sees_three_empty_states() {
        for p in [Persona::HealthOnly, Persona::NewUser] {
            let v = views(p);
            assert!(v.plan.empty.is_some(), "{p:?}");
            assert!(v.routes.empty.is_some(), "{p:?}");
            assert!(v.gear.empty.is_some(), "{p:?}");
        }
    }

    /// The committed files must match what the code writes now.
    #[test]
    fn committed_fixtures_are_current() {
        for p in Persona::ALL {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(p.places_file_name());
            let want = serde_json::to_string_pretty(&views(p)).unwrap() + "\n";
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
