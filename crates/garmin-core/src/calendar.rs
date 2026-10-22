//! The plan: planned workouts and races on the Garmin calendar, the training
//! plans of the account, and the steps of a planned workout.
//!
//! The shape of an event, a training plan and a repeat group is not confirmed
//! on a real account with data. Every field here is read as optional, and one
//! odd item never drops its list.

use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;

pub(crate) const SCHEMA: &str = r#"
    -- A planned workout or a race from the calendar. Replaced by month: the
    -- sync rewrites the days it fetched, so a removed item leaves the cache.
    CREATE TABLE IF NOT EXISTS plan_items (
        item_key        TEXT PRIMARY KEY,
        date            TEXT NOT NULL,
        kind            TEXT NOT NULL,
        title           TEXT,
        sport           TEXT,
        workout_id      INTEGER,
        is_race         INTEGER NOT NULL DEFAULT 0,
        event_time      TEXT,
        training_plan_id INTEGER
    );

    CREATE TABLE IF NOT EXISTS training_plans (
        plan_id INTEGER PRIMARY KEY,
        name    TEXT,
        active  INTEGER NOT NULL DEFAULT 1
    );

    CREATE TABLE IF NOT EXISTS race_events (
        event_key   TEXT PRIMARY KEY,
        date        TEXT NOT NULL,
        name        TEXT,
        target_secs REAL,
        location    TEXT
    );

    -- The steps of a planned workout as a flat JSON list, and when they were
    -- fetched, so a workout edited on Garmin is read again after a day.
    CREATE TABLE IF NOT EXISTS workout_steps (
        workout_id INTEGER PRIMARY KEY,
        name       TEXT,
        sport      TEXT,
        steps      TEXT NOT NULL DEFAULT '[]',
        fetched_at INTEGER NOT NULL
    );
"#;

/// A planned workout (`workout`) or a race (`event`) on one day.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// `workout` or `event`.
    pub kind: String,
    pub title: Option<String>,
    pub sport: Option<String>,
    pub workout_id: Option<i64>,
    pub is_race: bool,
    /// Start time of day as Garmin sends it, if any.
    pub event_time_local: Option<String>,
    pub training_plan_id: Option<i64>,
    /// Garmin's id of the item. Part of the key in the cache.
    pub item_id: Option<i64>,
}

impl PlanItem {
    fn key(&self) -> String {
        match self.item_id {
            Some(id) => format!("{}:{id}", self.kind),
            None => format!(
                "{}:{}:{}",
                self.kind,
                self.date,
                self.title.as_deref().unwrap_or("")
            ),
        }
    }
}

fn text(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// The planned workouts and races in one calendar month reply. An item of any
/// other type (an activity, a weigh-in, a nap) is not part of the plan.
pub fn parse_plan_items(v: &Value) -> Vec<PlanItem> {
    let Some(list) = v.get("calendarItems").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter().filter_map(parse_plan_item).collect()
}

fn parse_plan_item(it: &Value) -> Option<PlanItem> {
    let kind_type = it.get("itemType")?.as_str()?;
    let is_race = it.get("isRace").and_then(Value::as_bool).unwrap_or(false);
    let kind = match kind_type {
        "workout" => "workout",
        "event" | "race" => "event",
        _ => return None,
    };
    let date = text(it, "date")?;
    // A day string only. Anything else cannot sit in a day window.
    date.get(..10)
        .filter(|d| d.as_bytes().get(4) == Some(&b'-'))?;
    Some(PlanItem {
        date: date[..10].to_owned(),
        kind: kind.to_owned(),
        title: text(it, "title"),
        sport: text(it, "sportTypeKey"),
        workout_id: it.get("workoutId").and_then(Value::as_i64),
        is_race: is_race || kind == "event",
        event_time_local: text(it, "eventTimeLocal"),
        training_plan_id: it
            .get("trainingPlanId")
            .and_then(Value::as_i64)
            .filter(|id| *id > 0),
        item_id: it.get("id").and_then(Value::as_i64),
    })
}

/// A training plan of the account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TrainingPlan {
    pub plan_id: i64,
    pub name: Option<String>,
    pub active: bool,
}

/// The plans in a plan list reply. The list is `trainingPlanList`.
pub fn parse_training_plans(v: &Value) -> Vec<TrainingPlan> {
    let Some(list) = v.get("trainingPlanList").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|p| {
            let plan_id = ["trainingPlanId", "planId", "id"]
                .iter()
                .find_map(|k| p.get(*k).and_then(Value::as_i64))?;
            let status = ["trainingStatus", "status"]
                .iter()
                .find_map(|k| text(p, k))
                .map(|s| s.to_lowercase());
            Some(TrainingPlan {
                plan_id,
                name: ["name", "trainingPlanName", "planName"]
                    .iter()
                    .find_map(|k| text(p, k)),
                // A plan with no status is listed as one that the athlete follows.
                active: status.is_none_or(|s| {
                    matches!(
                        s.as_str(),
                        "active" | "in_progress" | "inprogress" | "current"
                    )
                }),
            })
        })
        .collect()
}

/// A race that the athlete entered on the calendar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RaceEvent {
    pub event_key: String,
    pub date: String,
    pub name: Option<String>,
    /// The target time in seconds, if the athlete set one.
    pub target_secs: Option<f64>,
    pub location: Option<String>,
}

/// The events in an events reply: a list, or an object that holds `events`.
pub fn parse_race_events(v: &Value) -> Vec<RaceEvent> {
    let Some(list) = v.as_array().or_else(|| v.get("events")?.as_array()) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|e| {
            let date = ["date", "eventDate", "startDate"]
                .iter()
                .find_map(|k| text(e, k))?;
            let date = date.get(..10)?.to_owned();
            let name = ["eventName", "name", "title"]
                .iter()
                .find_map(|k| text(e, k));
            let key = e.get("id").and_then(Value::as_i64).map_or_else(
                || format!("{date}:{}", name.as_deref().unwrap_or("")),
                |i| i.to_string(),
            );
            Some(RaceEvent {
                event_key: key,
                date,
                name,
                target_secs: event_target_secs(e),
                location: text(e, "location"),
            })
        })
        .collect()
}

/// A completion target counts as a time only when its unit says time.
fn event_target_secs(e: &Value) -> Option<f64> {
    let t = e.get("completionTarget")?;
    let unit = t.get("unit").or_else(|| t.get("unitType"))?.as_str()?;
    let value = t.get("value")?.as_f64()?;
    (unit.eq_ignore_ascii_case("time") && value > 0.0).then_some(value)
}

/// One step of a workout, flattened. A step inside a repeat group has the
/// group number and the repeat count of the group.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    /// Position in the flat list, from 1.
    pub order: u32,
    /// `warmup`, `interval`, `recovery`, `cooldown`, `rest`, `other` or a key
    /// that Garmin sent.
    pub kind: String,
    /// `time`, `distance`, `lap.button`, `iterations` or what Garmin sent.
    pub end_type: Option<String>,
    /// Seconds for `time`, metres for `distance`.
    pub end_value: Option<f64>,
    /// `heart.rate.zone`, `pace.zone`, `no.target` or what Garmin sent.
    pub target_type: Option<String>,
    pub target_low: Option<f64>,
    pub target_high: Option<f64>,
    pub zone: Option<i64>,
    pub description: Option<String>,
    /// How many times the step runs. 1 outside a repeat group.
    pub repeat: u32,
    /// The repeat group that holds the step, from 1.
    pub group: Option<u32>,
}

/// The steps of a workout, and its name and sport.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WorkoutSteps {
    pub workout_id: i64,
    pub name: Option<String>,
    pub sport: Option<String>,
    pub steps: Vec<PlanStep>,
}

/// The deepest repeat group that is read. Garmin allows one level. A deeper
/// reply is cut and not followed.
const MAX_DEPTH: usize = 3;

/// The steps in a workout detail reply, flat and in order.
pub fn parse_workout_steps(workout_id: i64, v: &Value) -> WorkoutSteps {
    let mut steps = Vec::new();
    let mut groups = 0;
    let top = v
        .pointer("/workoutSegments/0/workoutSteps")
        .and_then(Value::as_array);
    if let Some(list) = top {
        flatten(list, 1, None, &mut groups, 0, &mut steps);
    }
    WorkoutSteps {
        workout_id,
        name: text(v, "workoutName"),
        sport: v
            .pointer("/sportType/sportTypeKey")
            .and_then(Value::as_str)
            .map(str::to_owned),
        steps,
    }
}

fn flatten(
    list: &[Value],
    repeat: u32,
    group: Option<u32>,
    groups: &mut u32,
    depth: usize,
    out: &mut Vec<PlanStep>,
) {
    for s in list {
        let children = s.get("workoutSteps").and_then(Value::as_array);
        let is_group =
            s.get("type").and_then(Value::as_str) == Some("RepeatGroupDTO") || children.is_some();
        if is_group {
            if depth >= MAX_DEPTH {
                continue;
            }
            *groups += 1;
            let n = s
                .get("numberOfIterations")
                .or_else(|| s.get("endConditionValue"))
                .and_then(Value::as_f64)
                .map_or(1, |n| n.clamp(1.0, 99.0) as u32);
            let id = *groups;
            flatten(
                children.map_or(&[][..], Vec::as_slice),
                repeat * n,
                Some(id),
                groups,
                depth + 1,
                out,
            );
            continue;
        }
        let key = |path: &str| s.pointer(path).and_then(Value::as_str).map(str::to_owned);
        let low = s.get("targetValueOne").and_then(Value::as_f64);
        let high = s.get("targetValueTwo").and_then(Value::as_f64);
        out.push(PlanStep {
            order: out.len() as u32 + 1,
            kind: key("/stepType/stepTypeKey").unwrap_or_else(|| "other".into()),
            end_type: key("/endCondition/conditionTypeKey"),
            end_value: s.get("endConditionValue").and_then(Value::as_f64),
            target_type: key("/targetType/workoutTargetTypeKey"),
            target_low: low,
            target_high: high,
            zone: s.get("zoneNumber").and_then(Value::as_i64),
            description: text(s, "description"),
            repeat,
            group,
        });
    }
}

impl Db {
    /// Replace the plan items from `from` to `to` (both included).
    pub fn replace_plan_items(&self, from: &str, to: &str, items: &[PlanItem]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM plan_items WHERE date >= ?1 AND date <= ?2",
            params![from, to],
        )?;
        for i in items
            .iter()
            .filter(|i| i.date.as_str() >= from && i.date.as_str() <= to)
        {
            tx.execute(
                "INSERT OR REPLACE INTO plan_items (
                    item_key, date, kind, title, sport, workout_id, is_race,
                    event_time, training_plan_id
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    i.key(),
                    i.date,
                    i.kind,
                    i.title,
                    i.sport,
                    i.workout_id,
                    i.is_race,
                    i.event_time_local,
                    i.training_plan_id,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The plan items from `from` to `to` (both included), by day and time.
    pub fn plan_items(&self, from: &str, to: &str) -> Result<Vec<PlanItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT date, kind, title, sport, workout_id, is_race, event_time,
                    training_plan_id
             FROM plan_items WHERE date >= ?1 AND date <= ?2
             ORDER BY date, event_time, item_key",
        )?;
        let rows = stmt
            .query_map(params![from, to], |r| {
                Ok(PlanItem {
                    date: r.get(0)?,
                    kind: r.get(1)?,
                    title: r.get(2)?,
                    sport: r.get(3)?,
                    workout_id: r.get(4)?,
                    is_race: r.get(5)?,
                    event_time_local: r.get(6)?,
                    training_plan_id: r.get(7)?,
                    item_id: None,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Replace the training plans.
    pub fn replace_training_plans(&self, plans: &[TrainingPlan]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM training_plans", [])?;
        for p in plans {
            tx.execute(
                "INSERT OR REPLACE INTO training_plans (plan_id, name, active) VALUES (?1,?2,?3)",
                params![p.plan_id, p.name, p.active],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The training plans, the active ones first.
    pub fn training_plans(&self) -> Result<Vec<TrainingPlan>> {
        let mut stmt = self.conn.prepare(
            "SELECT plan_id, name, active FROM training_plans ORDER BY active DESC, plan_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(TrainingPlan {
                    plan_id: r.get(0)?,
                    name: r.get(1)?,
                    active: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Replace the race events from `from` to `to` (both included).
    pub fn replace_race_events(&self, from: &str, to: &str, events: &[RaceEvent]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM race_events WHERE date >= ?1 AND date <= ?2",
            params![from, to],
        )?;
        for e in events
            .iter()
            .filter(|e| e.date.as_str() >= from && e.date.as_str() <= to)
        {
            tx.execute(
                "INSERT OR REPLACE INTO race_events (event_key, date, name, target_secs, location)
                 VALUES (?1,?2,?3,?4,?5)",
                params![e.event_key, e.date, e.name, e.target_secs, e.location],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The race events from `from` to `to` (both included), soonest first.
    pub fn race_events(&self, from: &str, to: &str) -> Result<Vec<RaceEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT event_key, date, name, target_secs, location FROM race_events
             WHERE date >= ?1 AND date <= ?2 ORDER BY date, event_key",
        )?;
        let rows = stmt
            .query_map(params![from, to], |r| {
                Ok(RaceEvent {
                    event_key: r.get(0)?,
                    date: r.get(1)?,
                    name: r.get(2)?,
                    target_secs: r.get(3)?,
                    location: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Store the steps of a workout, read at `now` (epoch seconds).
    pub fn save_workout_steps(&self, w: &WorkoutSteps, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO workout_steps (workout_id, name, sport, steps, fetched_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![
                w.workout_id,
                w.name,
                w.sport,
                serde_json::to_string(&w.steps)?,
                now
            ],
        )?;
        Ok(())
    }

    /// The stored steps of a workout.
    pub fn workout_steps(&self, workout_id: i64) -> Result<Option<WorkoutSteps>> {
        use rusqlite::OptionalExtension;
        let row = self
            .conn
            .query_row(
                "SELECT name, sport, steps FROM workout_steps WHERE workout_id = ?1",
                params![workout_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, String>(2)?)),
            )
            .optional()?;
        Ok(row.map(|(name, sport, steps)| WorkoutSteps {
            workout_id,
            name,
            sport,
            steps: serde_json::from_str(&steps).unwrap_or_default(),
        }))
    }

    /// Which of `ids` have no steps stored, or steps older than `max_age_secs`.
    pub fn workouts_to_fetch(&self, ids: &[i64], now: i64, max_age_secs: i64) -> Result<Vec<i64>> {
        let mut out = Vec::new();
        for id in ids {
            let at: Option<i64> = self
                .conn
                .query_row(
                    "SELECT fetched_at FROM workout_steps WHERE workout_id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .ok();
            if at.is_none_or(|t| now - t >= max_age_secs) && !out.contains(id) {
                out.push(*id);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn month() -> Value {
        json!({ "calendarItems": [
            { "id": 1, "itemType": "workout", "date": "2026-03-12", "title": "Easy", "sportTypeKey": "running",
              "workoutId": 7, "trainingPlanId": 0 },
            { "id": 2, "itemType": "event", "date": "2026-03-20", "title": "Race", "isRace": true,
              "eventTimeLocal": "08:00", "trainingPlanId": 5 },
            { "id": 3, "itemType": "activity", "date": "2026-03-12" },
            { "id": 4, "itemType": "weight", "date": "2026-03-12" },
            { "itemType": "workout", "title": "No date" },
            { "itemType": "workout", "date": "soon" },
            "junk",
        ]})
    }

    #[test]
    fn only_workouts_and_races_are_kept_and_an_odd_item_is_skipped() {
        let got = parse_plan_items(&month());
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].training_plan_id, None, "plan id 0 means no plan");
        assert_eq!(got[0].workout_id, Some(7));
        assert!(got[1].is_race && got[1].kind == "event");
        assert_eq!(got[1].event_time_local.as_deref(), Some("08:00"));
        assert!(parse_plan_items(&json!(null)).is_empty());
    }

    #[test]
    fn a_month_is_replaced_whole_and_other_days_stay() {
        let db = crate::db::scratch_db("plan-items");
        let items = parse_plan_items(&month());
        db.replace_plan_items("2026-03-01", "2026-03-31", &items)
            .unwrap();
        db.replace_plan_items("2026-04-01", "2026-04-30", &[])
            .unwrap();
        assert_eq!(db.plan_items("2026-03-01", "2026-04-30").unwrap().len(), 2);
        db.replace_plan_items("2026-03-01", "2026-03-31", &items[..1])
            .unwrap();
        let left = db.plan_items("2026-03-01", "2026-03-31").unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].title.as_deref(), Some("Easy"));
        // An item outside the window that was fetched is not written.
        db.replace_plan_items("2026-05-01", "2026-05-31", &items)
            .unwrap();
        assert!(db
            .plan_items("2026-05-01", "2026-05-31")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn plans_and_events_read_every_field_as_optional() {
        let plans = parse_training_plans(&json!({ "trainingPlanList": [
            { "trainingPlanId": 3, "name": "Plan A" },
            { "id": 4, "trainingStatus": "COMPLETED" },
            { "name": "No id" },
        ]}));
        assert_eq!(plans.len(), 2);
        assert!(plans[0].active && !plans[1].active);
        assert_eq!(plans[1].name, None);

        let events = parse_race_events(&json!([
            { "id": 9, "eventName": "Spring race", "date": "2026-05-03",
              "completionTarget": { "value": 12000.0, "unit": "time" } },
            { "name": "Far run", "startDate": "2026-06-01T00:00:00",
              "completionTarget": { "value": 42000.0, "unit": "distance" } },
            { "eventName": "No date" },
        ]));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].target_secs, Some(12000.0));
        assert_eq!(
            events[1].target_secs, None,
            "a distance target is not a time"
        );
        assert_eq!(events[1].date, "2026-06-01");
        assert!(!parse_race_events(&json!({ "events": [{ "date": "2026-01-01" }] })).is_empty());
    }

    #[test]
    fn a_repeat_group_is_flattened_with_its_count() {
        let w = json!({
            "workoutName": "Test", "sportType": { "sportTypeKey": "running" },
            "workoutSegments": [{ "workoutSteps": [
                { "type": "ExecutableStepDTO", "stepType": { "stepTypeKey": "warmup" },
                  "endCondition": { "conditionTypeKey": "time" }, "endConditionValue": 600.0 },
                { "type": "RepeatGroupDTO", "numberOfIterations": 4,
                  "stepType": { "stepTypeKey": "repeat" }, "workoutSteps": [
                    { "type": "ExecutableStepDTO", "stepType": { "stepTypeKey": "interval" },
                      "endCondition": { "conditionTypeKey": "distance" }, "endConditionValue": 400.0,
                      "targetType": { "workoutTargetTypeKey": "pace.zone" },
                      "targetValueOne": 3.5, "targetValueTwo": 3.0 },
                    { "stepType": { "stepTypeKey": "recovery" } },
                ]},
                { "type": "RepeatGroupDTO" },
                { "type": "ExecutableStepDTO" },
            ]}],
        });
        let got = parse_workout_steps(5, &w);
        assert_eq!(got.sport.as_deref(), Some("running"));
        let shape: Vec<_> = got
            .steps
            .iter()
            .map(|s| (s.order, s.kind.as_str(), s.repeat, s.group))
            .collect();
        assert_eq!(
            shape,
            [
                (1, "warmup", 1, None),
                (2, "interval", 4, Some(1)),
                (3, "recovery", 4, Some(1)),
                (4, "other", 1, None)
            ]
        );
        assert_eq!(got.steps[1].target_low, Some(3.5));
        assert!(parse_workout_steps(5, &json!(null)).steps.is_empty());
    }

    #[test]
    fn steps_are_stored_and_fetched_again_after_a_day() {
        let db = crate::db::scratch_db("workout-steps");
        let w = WorkoutSteps {
            workout_id: 7,
            steps: vec![PlanStep {
                order: 1,
                repeat: 1,
                ..Default::default()
            }],
            ..Default::default()
        };
        db.save_workout_steps(&w, 1_000).unwrap();
        assert_eq!(db.workout_steps(7).unwrap(), Some(w));
        assert_eq!(db.workout_steps(8).unwrap(), None);
        assert_eq!(
            db.workouts_to_fetch(&[7, 8, 8], 1_500, 86_400).unwrap(),
            vec![8]
        );
        assert_eq!(db.workouts_to_fetch(&[7], 90_000, 86_400).unwrap(), vec![7]);
    }
}
