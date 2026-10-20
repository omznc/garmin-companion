//! The newer synced data in one view: devices, the Body Battery curve, the
//! overnight HRV readings, the plan, the food log, VO2 max and the fitness age.
//!
//! Each part is empty or `None` when the cache holds nothing for it, and the
//! UI hides the block. Nothing here reads the network.

use crate::derive::date_key;
use crate::ApiError;
use chrono::{Duration, NaiveDate};
use garmin_core::calendar::{PlanItem, RaceEvent, TrainingPlan, WorkoutSteps};
use garmin_core::devices::DeviceInfo;
use garmin_core::intraday::{BatteryPoint, HrvReading};
use garmin_core::nutrition::{Meal, NutritionGoals};
use garmin_core::vo2::{FitnessAge, Vo2Day};
use garmin_core::Db;
use serde::{Deserialize, Serialize};

/// Days ahead that the plan covers.
const PLAN_DAYS: i64 = 14;
/// Days of VO2 max history.
const VO2_DAYS: i64 = 365;

/// A planned item with the steps of its workout, if they are cached.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedItem {
    #[serde(flatten)]
    pub item: PlanItem,
    pub steps: Option<WorkoutSteps>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NutritionToday {
    pub meals: Vec<Meal>,
    pub goals: Option<NutritionGoals>,
    /// The sum of the calories of the meals that have a figure. `None` when no
    /// meal has one.
    pub logged_kcal: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtrasView {
    pub date: String,
    pub devices: Vec<DeviceInfo>,
    /// Today's Body Battery curve.
    pub body_battery: Vec<BatteryPoint>,
    /// The overnight HRV readings of the night that ended today.
    pub hrv_readings: Vec<HrvReading>,
    /// Planned workouts and races in the next 14 days, soonest first.
    pub planned: Vec<PlannedItem>,
    /// The next race: an event if there is one, else a race on the calendar.
    pub race: Option<RaceEvent>,
    pub active_plan: Option<TrainingPlan>,
    pub nutrition: NutritionToday,
    pub vo2: Vec<Vo2Day>,
    pub fitness_age: Option<FitnessAge>,
}

/// The next race from the events, else from the planned items.
fn next_race(events: Vec<RaceEvent>, planned: &[PlannedItem]) -> Option<RaceEvent> {
    events.into_iter().next().or_else(|| {
        planned.iter().find(|p| p.item.is_race).map(|p| RaceEvent {
            event_key: p.item.date.clone(),
            date: p.item.date.clone(),
            name: p.item.title.clone(),
            target_secs: None,
            location: None,
        })
    })
}

fn nutrition_today(db: &Db, date: &str) -> Result<NutritionToday, ApiError> {
    let meals = db.nutrition_meals(date)?;
    let kcal: Vec<f64> = meals.iter().filter_map(|m| m.kcal).collect();
    Ok(NutritionToday {
        logged_kcal: (!kcal.is_empty()).then(|| kcal.iter().sum()),
        goals: db.nutrition_goals(date)?,
        meals,
    })
}

/// Build the view for `today`.
pub fn extras_view(db: &Db, today: NaiveDate) -> Result<ExtrasView, ApiError> {
    let day = date_key(today);
    let ahead = date_key(today + Duration::days(PLAN_DAYS));
    let mut planned = Vec::new();
    for item in db.plan_items(&day, &ahead)? {
        let steps = match item.workout_id {
            Some(id) => db.workout_steps(id)?,
            None => None,
        };
        planned.push(PlannedItem { item, steps });
    }
    let year_ahead = date_key(today + Duration::days(365));
    Ok(ExtrasView {
        devices: db.devices()?,
        body_battery: db.body_battery_curve(&day)?,
        hrv_readings: db.hrv_readings(&day)?,
        race: next_race(db.race_events(&day, &year_ahead)?, &planned),
        planned,
        active_plan: db.training_plans()?.into_iter().find(|p| p.active),
        nutrition: nutrition_today(db, &day)?,
        vo2: db.vo2_since(&date_key(today - Duration::days(VO2_DAYS)))?,
        fitness_age: db.latest_fitness_age()?,
        date: day,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use garmin_core::calendar::PlanStep;

    fn db(tag: &str) -> Db {
        Db::open(&std::env::temp_dir().join(format!(
            "garmin-api-extras-{tag}-{}/cache.sqlite3",
            std::process::id()
        )))
        .unwrap()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()
    }

    fn item(date: &str, workout_id: Option<i64>, race: bool) -> PlanItem {
        PlanItem {
            date: date.into(),
            kind: if race { "event" } else { "workout" }.into(),
            title: Some("Item".into()),
            workout_id,
            is_race: race,
            item_id: Some(workout_id.unwrap_or(99)),
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_cache_gives_an_empty_view() {
        let v = extras_view(&db("empty"), today()).unwrap();
        assert!(v.devices.is_empty() && v.planned.is_empty() && v.vo2.is_empty());
        assert!(v.race.is_none() && v.active_plan.is_none() && v.fitness_age.is_none());
        assert_eq!(v.nutrition, NutritionToday::default());
    }

    #[test]
    fn the_plan_holds_the_next_two_weeks_with_steps_and_a_race_from_the_calendar() {
        let db = db("plan");
        db.replace_plan_items(
            "2026-03-01",
            "2026-03-31",
            &[
                item("2026-03-12", Some(7), false),
                item("2026-03-30", Some(8), false),
                item("2026-03-20", None, true),
                item("2026-03-05", Some(6), false),
            ],
        )
        .unwrap();
        db.save_workout_steps(
            &WorkoutSteps {
                workout_id: 7,
                steps: vec![PlanStep {
                    order: 1,
                    repeat: 1,
                    ..Default::default()
                }],
                ..Default::default()
            },
            0,
        )
        .unwrap();
        let v = extras_view(&db, today()).unwrap();
        let dates: Vec<_> = v.planned.iter().map(|p| p.item.date.as_str()).collect();
        assert_eq!(dates, ["2026-03-12", "2026-03-20"].map(String::from));
        assert_eq!(v.planned[0].steps.as_ref().map(|s| s.steps.len()), Some(1));
        assert!(v.planned[1].steps.is_none());
        let race = v.race.unwrap();
        assert_eq!((race.date.as_str(), race.target_secs), ("2026-03-20", None));

        // An event with a target time wins over a race on the calendar.
        db.replace_race_events(
            "2026-03-11",
            "2027-03-11",
            &[RaceEvent {
                event_key: "1".into(),
                date: "2026-06-01".into(),
                name: Some("Event".into()),
                target_secs: Some(12_000.0),
                location: None,
            }],
        )
        .unwrap();
        assert_eq!(
            extras_view(&db, today()).unwrap().race.unwrap().target_secs,
            Some(12_000.0)
        );
    }

    #[test]
    fn the_food_log_sums_the_meals_that_have_calories() {
        let db = db("food");
        let meal = |kcal| Meal {
            name: Some("m".into()),
            kcal,
            ..Default::default()
        };
        db.save_nutrition_day(
            "2026-03-11",
            Some(&NutritionGoals {
                calories: Some(2000.0),
                ..Default::default()
            }),
            &[meal(Some(500.0)), meal(None), meal(Some(250.0))],
        )
        .unwrap();
        let n = extras_view(&db, today()).unwrap().nutrition;
        assert_eq!((n.meals.len(), n.logged_kcal), (3, Some(750.0)));
        assert_eq!(n.goals.and_then(|g| g.calories), Some(2000.0));
    }
}
