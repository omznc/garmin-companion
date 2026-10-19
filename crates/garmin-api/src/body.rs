//! The Weight, Food and Strength screens as view models.
//!
//! The core builds the numbers and the sentences. A UI only draws them.
//! Every function here reads the cache. Nothing writes, and nothing calls Garmin.

use crate::derive::{daily_series, date_key, hydration};
use crate::text::{num, short_date, weight_sentence};
use crate::{activity_profile, ApiError};
use chrono::NaiveDate;
use garmin_core::query;
use garmin_core::sport::is_exercise;
use garmin_core::{Db, Family};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The title and the text of a screen that has no data. It is a state, not an error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyNote {
    pub title: String,
    pub body: String,
}

fn empty(title: &str, body: &str) -> EmptyNote {
    EmptyNote {
        title: title.into(),
        body: body.into(),
    }
}

fn label(date: &str) -> String {
    short_date(date).unwrap_or_else(|| date.to_string())
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn moved(v: f64) -> String {
    format!("{} {:.1} kg", if v < 0.0 { "down" } else { "up" }, v.abs())
}

/* ------------------------------------------------------------------ weight --- */

const WEIGHT_DAYS: u32 = 180;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightWindowView {
    pub days: u32,
    pub sentence: String,
}

/// The latest reading that carries a body composition figure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Composition {
    pub date: String,
    pub body_fat_pct: Option<f64>,
    pub body_water_pct: Option<f64>,
    pub muscle_kg: Option<f64>,
    pub bone_kg: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightPointView {
    pub date: String,
    /// The short date, such as `06 Aug`.
    pub label: String,
    pub kg: f64,
    pub trend_kg: Option<f64>,
    pub outlier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightView {
    pub empty: Option<EmptyNote>,
    /// Weigh-ins in the window, outliers included.
    pub count: usize,
    /// The core weight sentence. The same text as the Today line.
    pub sentence: Option<String>,
    pub trend_kg: Option<f64>,
    pub latest_kg: Option<f64>,
    pub change_kg: Option<f64>,
    pub bmi: Option<f64>,
    pub windows: Vec<WeightWindowView>,
    /// Oldest first, for the chart. An outlier stays in the list and is marked.
    pub points: Vec<WeightPointView>,
    pub composition: Option<Composition>,
    pub goal_sentence: Option<String>,
}

fn window_sentence(w: &query::WeightWindow) -> String {
    let span = format!("the last {} days", w.days);
    match (w.count, w.change_kg) {
        (0, _) => format!("No weigh-ins in {span}."),
        (n, Some(c)) if c.abs() >= 0.05 => format!(
            "In {span} the trend moved {} over {}.",
            moved(c),
            plural(n, "weigh-in", "weigh-ins")
        ),
        (n, Some(_)) => format!(
            "In {span} the trend held steady over {}.",
            plural(n, "weigh-in", "weigh-ins")
        ),
        (_, None) => format!("One weigh-in in {span}, so there is no change to report for it."),
    }
}

fn goal_sentence(g: &query::WeightGoal) -> String {
    let head = format!(
        "{:.1} kg {} your goal of {:.1} kg",
        g.delta_kg.abs(),
        if g.delta_kg < 0.0 { "above" } else { "below" },
        g.target_kg
    );
    match &g.eta_date {
        Some(d) => format!("{head}. At this rate you reach it around {}.", label(d)),
        None => format!("{head}. The trend does not point at it yet."),
    }
}

fn composition(db: &Db, today: NaiveDate) -> Result<Option<Composition>, ApiError> {
    let from = date_key(today - chrono::Duration::days(WEIGHT_DAYS as i64 - 1));
    let found = db
        .weigh_ins_since(&from)?
        .into_iter()
        .rev()
        .find(|w| w.body_fat.is_some() || w.body_water.is_some() || w.muscle_mass.is_some());
    // Garmin stores body mass figures in grams.
    Ok(found.map(|w| Composition {
        date: w.calendar_date,
        body_fat_pct: w.body_fat,
        body_water_pct: w.body_water,
        muscle_kg: w.muscle_mass.map(|g| (g / 100.0).round() / 10.0),
        bone_kg: w.bone_mass.map(|g| (g / 100.0).round() / 10.0),
    }))
}

fn weight_empty(latest_date: Option<&str>) -> EmptyNote {
    let title = if latest_date.is_some() {
        "No weigh-ins in the last six months."
    } else {
        "No weigh-ins on this account."
    };
    empty(
        title,
        "This screen reads what Garmin holds. Weigh-ins reach Garmin from a connected scale, from the Connect app, or from an integration. A new weigh-in appears here after the next sync.",
    )
}

pub fn weight_view(db: &Db, today: NaiveDate) -> Result<WeightView, ApiError> {
    let r = query::weight(db, WEIGHT_DAYS)?;
    let note = (r.count == 0).then(|| weight_empty(r.latest_date.as_deref()));
    let has = note.is_none();
    Ok(WeightView {
        empty: note,
        count: r.count,
        sentence: has.then(|| weight_sentence(r.trend_kg, r.days_since_latest, r.rate_kg_per_week)),
        trend_kg: r.trend_kg,
        latest_kg: r.latest_kg,
        change_kg: r.change_kg,
        bmi: r.bmi,
        windows: if has {
            r.windows
                .iter()
                .map(|w| WeightWindowView {
                    days: w.days,
                    sentence: window_sentence(w),
                })
                .collect()
        } else {
            Vec::new()
        },
        composition: if has { composition(db, today)? } else { None },
        // Without a weight the core gives a delta of 0.0. That is no distance to a goal.
        goal_sentence: r
            .goal
            .as_ref()
            .filter(|_| r.trend_kg.or(r.latest_kg).is_some())
            .map(goal_sentence),
        points: r
            .points
            .into_iter()
            .map(|p| WeightPointView {
                label: label(&p.date),
                date: p.date,
                kg: p.kg,
                trend_kg: p.trend_kg,
                outlier: p.outlier,
            })
            .collect(),
    })
}

/* -------------------------------------------------------------------- food --- */

const FOOD_DAYS: u32 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoodDay {
    pub date: String,
    pub label: String,
    pub logged: bool,
    pub eaten_kcal: Option<f64>,
    pub burned_kcal: Option<f64>,
    pub balance_kcal: Option<f64>,
    /// `None` when the day has no water log. Garmin writes zero for no log.
    pub water_ml: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaterView {
    pub avg_ml: f64,
    /// `None` until the user sets a goal in Garmin. A UI then draws no goal line.
    pub goal_ml: Option<f64>,
    pub sentence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoodView {
    /// Set when no day in the window has a food log.
    pub empty: Option<EmptyNote>,
    pub days_logged: usize,
    pub avg_eaten_kcal: Option<f64>,
    pub avg_burned_kcal: Option<f64>,
    pub avg_balance_kcal: Option<f64>,
    /// The net calorie goal of the newest logged day. `None` when unset.
    pub goal_kcal: Option<f64>,
    /// How the averages were made. It is empty when no day has a log.
    pub sentence: String,
    pub water: Option<WaterView>,
    /// Newest first.
    pub days: Vec<FoodDay>,
}

fn food_empty() -> EmptyNote {
    empty(
        "No food logged in the last 30 days.",
        "Garmin returns the eaten side only when something writes it, such as the Connect app log or an integration. Burn comes from your device. Log a day in Garmin Connect and it appears here after the next sync.",
    )
}

fn food_sentence(logged: usize, total: usize, goal: Option<f64>) -> String {
    let base = format!(
        "Averaged over the {} with a food log, not over all {total}. Balance is eaten minus burned. A negative balance is a deficit.",
        plural(logged, "day", "days")
    );
    match goal {
        Some(g) => format!("{base} Your Garmin net calorie goal is {}.", num(g)),
        None => base,
    }
}

fn water_view(db: &Db, today: NaiveDate) -> Result<Option<WaterView>, ApiError> {
    let rows = daily_series(
        db.daily_since(&garmin_core::days_ago(FOOD_DAYS))?,
        FOOD_DAYS,
        today,
    );
    Ok(hydration(&rows, FOOD_DAYS as usize).map(|h| WaterView {
        avg_ml: h.avg_ml.round(),
        goal_ml: h.goal_ml,
        sentence: crate::text::hydration_sentence(&h),
    }))
}

pub fn food_view(db: &Db, today: NaiveDate) -> Result<FoodView, ApiError> {
    let r = query::nutrition(db, FOOD_DAYS)?;
    let goal_kcal = r
        .days
        .iter()
        .find(|d| d.logged)
        .and_then(|d| d.net_calorie_goal);
    let none = r.days_logged == 0;
    Ok(FoodView {
        empty: none.then(food_empty),
        days_logged: r.days_logged,
        avg_eaten_kcal: r.avg_consumed_kcal,
        avg_burned_kcal: r.avg_burn_kcal,
        avg_balance_kcal: r.avg_balance_kcal,
        goal_kcal,
        sentence: if none {
            String::new()
        } else {
            food_sentence(r.days_logged, r.days.len(), goal_kcal)
        },
        water: water_view(db, today)?,
        days: r
            .days
            .into_iter()
            .map(|d| FoodDay {
                label: label(&d.date),
                logged: d.logged,
                eaten_kcal: d.consumed_kcal,
                burned_kcal: d.total_burn_kcal,
                balance_kcal: d.balance_kcal,
                water_ml: d.hydration_ml.filter(|v| *v > 0.0),
                date: d.date,
            })
            .collect(),
    })
}

/* ---------------------------------------------------------------- strength --- */

const STRENGTH_SESSIONS: u32 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrengthRow {
    pub activity_id: i64,
    pub date: String,
    pub label: String,
    /// For example `12 sets, 96 reps, 1:30 rest`.
    pub summary: String,
    /// The movements the watch guessed. `None` when it named none.
    pub movements: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExerciseDay {
    pub date: String,
    pub label: String,
    pub sets: usize,
    pub reps: i64,
}

/// One guessed movement across the sessions. Volume is sets and reps, because the watch records no load.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExerciseHistory {
    pub name: String,
    pub sessions: usize,
    pub sets: usize,
    pub reps: i64,
    /// For example `1 session, 3 sets`.
    #[serde(default)]
    pub detail: String,
    /// Oldest first.
    pub history: Vec<ExerciseDay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrengthView {
    /// True when the profile shows gym sessions or the cache holds sets. A UI hides the screen when false.
    pub shown: bool,
    pub empty: Option<EmptyNote>,
    pub sessions_examined: usize,
    pub avg_sets: Option<f64>,
    pub avg_reps: Option<f64>,
    pub median_rest_s: Option<f64>,
    /// Share of work sets that the watch named.
    pub identified_pct: Option<u32>,
    pub sentence: String,
    /// Says that the watch records no load, and that names are guesses.
    pub caveat: String,
    /// Newest first.
    pub sessions: Vec<StrengthRow>,
    /// Most sets first.
    pub exercises: Vec<ExerciseHistory>,
}

pub fn mmss(secs: f64) -> String {
    let total = secs.round() as i64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn nice(key: &str) -> String {
    let words = key.to_lowercase().replace('_', " ");
    let mut chars = words.chars();
    chars
        .next()
        .map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

fn strength_row(s: &garmin_core::StrengthSession) -> StrengthRow {
    let date = s.date.clone().unwrap_or_default();
    let rest = s
        .median_rest_s
        .filter(|r| *r > 0.0)
        .map_or(String::new(), |r| format!(", {} rest", mmss(r)));
    let names: Vec<String> = s
        .guessed_exercises
        .iter()
        .map(|e| nice(&e.exercise))
        .collect();
    let movements = (!names.is_empty()).then(|| {
        let more = if s.unlabelled_sets > 0 {
            format!(", {} unidentified", s.unlabelled_sets)
        } else {
            String::new()
        };
        format!("Probably {}{more}", names.join(", "))
    });
    StrengthRow {
        activity_id: s.activity_id,
        label: if date.is_empty() {
            "—".into()
        } else {
            label(&date)
        },
        summary: format!(
            "{}, {}{rest}",
            plural(s.work_sets, "set", "sets"),
            plural(s.total_reps.max(0) as usize, "rep", "reps")
        ),
        movements,
        date,
    }
}

/// Group the guessed sets by movement, oldest session first inside each group.
fn exercise_histories(sessions: &[garmin_core::StrengthSession]) -> Vec<ExerciseHistory> {
    let mut by_name: BTreeMap<String, ExerciseHistory> = BTreeMap::new();
    for s in sessions.iter().rev() {
        let date = s.date.clone().unwrap_or_default();
        for e in &s.guessed_exercises {
            let h = by_name
                .entry(e.exercise.clone())
                .or_insert_with(|| ExerciseHistory {
                    name: nice(&e.exercise),
                    sessions: 0,
                    sets: 0,
                    reps: 0,
                    detail: String::new(),
                    history: Vec::new(),
                });
            h.sessions += 1;
            h.sets += e.sets;
            h.reps += e.reps;
            h.history.push(ExerciseDay {
                label: label(&date),
                date: date.clone(),
                sets: e.sets,
                reps: e.reps,
            });
        }
    }
    let mut out: Vec<ExerciseHistory> = by_name.into_values().collect();
    for h in &mut out {
        h.detail = format!(
            "{}, {}",
            plural(h.sessions, "session", "sessions"),
            plural(h.sets, "set", "sets")
        );
    }
    out.sort_by(|a, b| b.sets.cmp(&a.sets).then_with(|| a.name.cmp(&b.name)));
    out
}

fn strength_empty() -> EmptyNote {
    empty(
        "No strength sessions with set data yet.",
        "Sets arrive with a sync, and only for sessions that the watch recorded as strength training. If you lifted recently, the sync has not reached those sessions yet.",
    )
}

fn gym_profile(db: &Db, today: NaiveDate) -> Result<bool, ApiError> {
    let exercise: Vec<_> = db
        .recent_activities(300, None)?
        .into_iter()
        .filter(|a| is_exercise(a.type_key.as_deref()))
        .collect();
    Ok(activity_profile(&exercise, 0, today)
        .active
        .contains(&Family::Gym))
}

pub fn strength_view(db: &Db, today: NaiveDate) -> Result<StrengthView, ApiError> {
    let r = query::strength_trend(db, STRENGTH_SESSIONS)?;
    let gym = gym_profile(db, today)?;
    let none = r.sessions.is_empty();
    let named = r.labelled_sets;
    let all = named + r.unlabelled_sets;
    Ok(StrengthView {
        shown: gym || !none,
        empty: none.then(strength_empty),
        sessions_examined: r.sessions_examined,
        avg_sets: r.avg_work_sets,
        avg_reps: r.avg_reps,
        median_rest_s: r.median_rest_s,
        identified_pct: (all > 0).then(|| (named as f64 / all as f64 * 100.0).round() as u32),
        sentence: if none {
            String::new()
        } else {
            format!(
                "Reps, work time and rest across {}.",
                plural(r.sessions_examined, "session", "sessions")
            )
        },
        caveat: "The watch does not record the weight on the bar, so volume here means sets and reps. Exercise names are the watch's guess from wrist motion. The app shows a name only where the watch was confident.".into(),
        sessions: r.sessions.iter().map(strength_row).collect(),
        exercises: exercise_histories(&r.sessions),
    })
}

/* ---------------------------------------------------------------- fixtures --- */

/// The three views together. The fixture files and the desktop app use this shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyViews {
    pub weight: WeightView,
    pub food: FoodView,
    pub strength: StrengthView,
}

pub fn body_views(db: &Db, today: NaiveDate) -> Result<BodyViews, ApiError> {
    Ok(BodyViews {
        weight: weight_view(db, today)?,
        food: food_view(db, today)?,
        strength: strength_view(db, today)?,
    })
}

/// Fill `db` with made-up weigh-ins, food, water and lifting for the fixture and the tests.
///
/// Rows are relative to `today`, and `today` must be the real day, because the core reads the clock.
pub fn seed_body(db: &Db, today: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
    use chrono::Duration;
    use garmin_core::db::WeighIn;
    for n in 0..12i64 {
        let date = today - Duration::days(n * 5);
        db.upsert_weigh_in(&WeighIn {
            sample_pk: 1_000 + n,
            calendar_date: date_key(date),
            weight_g: 81_500.0 - (11 - n) as f64 * 90.0 + ((n * 7) % 4) as f64 * 120.0,
            bmi: None,
            body_fat: (n % 2 == 0).then_some(19.5 + (n % 3) as f64 * 0.3),
            body_water: (n % 2 == 0).then_some(57.0),
            bone_mass: (n % 2 == 0).then_some(3_200.0),
            muscle_mass: (n % 2 == 0).then_some(37_400.0),
            source_type: Some("INDEX_SCALE".into()),
        })?;
    }
    for age in 0..30i64 {
        let date = date_key(today - Duration::days(age));
        let wobble = (age * 7 % 5) as f64;
        let mut d = garmin_core::DailyMetrics {
            date,
            total_burn_kcal: Some(2_500.0 + wobble * 40.0),
            hydration_ml: Some(1_600.0 + wobble * 150.0),
            ..Default::default()
        };
        if age % 3 != 2 {
            d.consumed_kcal = Some(2_100.0 + wobble * 60.0);
        }
        db.upsert_daily(&d)?;
    }
    seed_lifting(db, today)
}

/// One work set and the rest after it. The fifth set has no guessed movement.
fn lifting_set(id: i64, session: i64, i: i64) -> [garmin_core::ExerciseSet; 2] {
    const MOVES: [&str; 3] = ["BENCH_PRESS", "SQUAT", "CURL"];
    let exercise = (i < 5).then(|| MOVES[(i / 2) as usize % 3].to_string());
    let set = |active: bool| garmin_core::ExerciseSet {
        activity_id: id,
        set_index: i * 2 + i64::from(!active),
        active,
        duration_s: Some(if active { 40.0 } else { 90.0 }),
        reps: active.then_some(8 + (session + i) % 4),
        exercise: exercise.clone().filter(|_| active),
        exercise_confidence: exercise.as_ref().filter(|_| active).map(|_| 88.0),
        weight_kg: None,
        start_time: None,
    };
    [set(true), set(false)]
}

fn seed_lifting(db: &Db, today: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
    use chrono::Duration;
    use garmin_core::ExerciseSet;
    for n in 0..6i64 {
        let date = today - Duration::days(n * 3 + 1);
        let id = 9_400_000 + n;
        db.upsert_activity(&serde_json::from_value(serde_json::json!({
            "activityId": id,
            "activityName": "Strength",
            "startTimeLocal": format!("{} 18:00:00", date_key(date)),
            "duration": 3_000.0,
            "activityType": { "typeKey": "strength_training" },
        }))?)?;
        let sets: Vec<ExerciseSet> = (0..6).flat_map(|i| lifting_set(id, n, i)).collect();
        db.save_exercise_sets(id, &sets)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> (Db, NaiveDate, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "garmin-api-body-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let db = crate::open_db(&dir.join("cache.sqlite3")).unwrap();
        let today = garmin_core::today();
        seed_body(&db, today).unwrap();
        (db, today, dir)
    }

    #[test]
    fn an_empty_cache_gives_three_intentional_empty_states() {
        let dir =
            std::env::temp_dir().join(format!("garmin-api-body-empty-{}", std::process::id()));
        let db = crate::open_db(&dir.join("cache.sqlite3")).unwrap();
        let v = body_views(&db, garmin_core::today()).unwrap();
        assert!(v.weight.empty.is_some() && v.weight.points.is_empty());
        assert!(v.food.empty.is_some() && v.food.water.is_none() && v.food.goal_kcal.is_none());
        assert!(v.strength.empty.is_some() && !v.strength.shown);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn weight_uses_the_core_sentence_and_reads_composition() {
        let (db, today, dir) = seeded();
        let v = weight_view(&db, today).unwrap();
        assert!(v.empty.is_none() && v.count == 12);
        assert!(v.sentence.unwrap().starts_with("Weight trend"));
        assert_eq!(v.windows.len(), 2);
        let c = v.composition.unwrap();
        assert_eq!(c.muscle_kg, Some(37.4));
        assert_eq!(c.bone_kg, Some(3.2));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn food_averages_logged_days_only_and_has_no_goal_when_unset() {
        let (db, today, dir) = seeded();
        let v = food_view(&db, today).unwrap();
        assert!(v.empty.is_none() && v.days_logged > 0 && v.days_logged < v.days.len());
        assert_eq!(v.goal_kcal, None);
        assert_eq!(v.water.as_ref().unwrap().goal_ml, None);
        assert!(v.days.iter().any(|d| !d.logged && d.water_ml.is_some()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_goal_without_a_weight_has_no_sentence() {
        let dir = std::env::temp_dir().join(format!("garmin-api-goal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = crate::open_db(&dir.join("cache.sqlite3")).unwrap();
        db.set_sync_state("weight_goal_kg", "75").unwrap();
        let today = garmin_core::today();
        assert!(weight_view(&db, today).unwrap().goal_sentence.is_none());
        let _ = std::fs::remove_dir_all(dir);
        let (db, today, dir) = seeded();
        db.set_sync_state("weight_goal_kg", "75").unwrap();
        let text = weight_view(&db, today).unwrap().goal_sentence.unwrap();
        assert!(text.contains("your goal of 75.0 kg") && !text.starts_with("0.0 kg"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn strength_groups_sets_by_movement() {
        let (db, today, dir) = seeded();
        let v = strength_view(&db, today).unwrap();
        assert!(v.shown && v.empty.is_none());
        let bench = v
            .exercises
            .iter()
            .find(|e| e.name == "Bench press")
            .unwrap();
        assert_eq!(bench.sessions, 6);
        assert_eq!(bench.history.len(), 6);
        assert!(bench.history[0].date < bench.history[5].date);
        assert!(v.sessions[0]
            .movements
            .as_deref()
            .unwrap()
            .starts_with("Probably "));
        assert_eq!(mmss(90.0), "1:30");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn window_sentences_say_what_they_know() {
        let w = |count, change| query::WeightWindow {
            days: 7,
            count,
            trend_start_kg: None,
            trend_end_kg: None,
            change_kg: change,
            low_kg: None,
            high_kg: None,
        };
        assert_eq!(
            window_sentence(&w(0, None)),
            "No weigh-ins in the last 7 days."
        );
        assert!(window_sentence(&w(1, None)).contains("no change"));
        assert!(window_sentence(&w(3, Some(-0.4))).contains("down 0.4 kg"));
        assert!(window_sentence(&w(3, Some(0.0))).contains("steady"));
    }
}
