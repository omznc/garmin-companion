//! The tools that the model can call. Each one reads a bounded slice of the
//! local cache through `garmin_core::query`, the same code as the MCP server.
//!
//! Left out of this first version: drafting a workout, asking the person a
//! question, themes, and the close read of one session (it can call Garmin).
//! `zone_drift` and `cadence_trend` are also out. Both are about running, and
//! the coach must not lean on one sport. They return when the data gates them.

use anyhow::Result;
use garmin_core::{db::Db, query};
use serde_json::{json, Value};

const MAX_ACTIVITIES: u64 = 50;
const MAX_DAYS: u64 = 365;
const MAX_WEIGHT_DAYS: u64 = 730;

/// A slice of a table, held between 1 and `max`. The model picks the number,
/// and the result goes back on every later round, so a large guess is clamped
/// and not rejected.
pub(crate) fn window(args: &Value, key: &str, default: u64, max: u64) -> u32 {
    args[key].as_u64().unwrap_or(default).clamp(1, max) as u32
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": { "type": "object", "properties": properties, "required": required },
        }
    })
}

fn days(default: u32, max: u32) -> Value {
    json!({ "days": { "type": "integer", "description": format!("How many days back from today. Default {default}, capped at {max}.") } })
}

/// The tool surface offered to the model.
pub(crate) fn schemas() -> Value {
    let none = json!({});
    Value::Array(vec![
        tool(
            "recent_activities",
            "List recent activities of any sport with duration, distance, heart rate, per-zone minutes and training effect. Start here for 'how am I doing'. Each row has an `activity_id` that names one session for `activity_zones`.",
            json!({
                "limit": { "type": "integer", "description": "How many to return. Default 10, capped at 50." },
                "sport": { "type": "string", "description": "Substring of Garmin's type key, for example 'strength', 'golf', 'cycling', 'walking', 'running'." }
            }),
            &[],
        ),
        tool(
            "activity_zones",
            "Heart-rate zone breakdown for one activity: minutes and percent in zones 1-5. Omit activity_id for the most recent. Zones suit steady aerobic work and say little about stop-and-go sessions.",
            json!({ "activity_id": { "type": "integer" } }),
            &[],
        ),
        tool(
            "list_tags",
            "The labels the person put on their own sessions, with how many sessions carry each. Call it before tagged_activities.",
            none.clone(),
            &[],
        ),
        tool(
            "tagged_activities",
            "Activities that carry one of the person's own tags, newest first, with the same fields as recent_activities.",
            json!({
                "tag": { "type": "string", "description": "The tag, exactly as list_tags gave it." },
                "limit": { "type": "integer", "description": "How many to return. Default 20, capped at 50." }
            }),
            &["tag"],
        ),
        tool(
            "recovery",
            "Resting HR, HRV, training readiness, sleep, stress and body battery by day. The window is by date and ends today. A day without a watch is absent: an empty result means nothing was recorded, not that recovery was poor. Check restingHrSource before you trend resting HR.",
            days(14, 365),
            &[],
        ),
        tool(
            "sleep",
            "Sleep in detail: last night's stages (deep, light, REM, awake), bedtime and wake time, efficiency, overnight HRV and resting HR, respiration, SpO2, plus the typical bedtime and how far it varies. `insights` are computed from the rows, so quote them. `needsBackfill: true` means the detail is not synced yet, not that sleep was bad.",
            days(30, 365),
            &[],
        ),
        tool(
            "nutrition",
            "Calories eaten against calories burned by day, plus hydration. A day with no food log has consumed_kcal null and logged:false. That is a missing log, not a day of eating nothing.",
            days(30, 365),
            &[],
        ),
        tool(
            "weight",
            "Body weight over time: weigh-ins, a smoothed trend, the rate of change per week, BMI, and the logged calorie balance against the scale. A point with outlier:true is a mis-entry and is not in the trend.",
            days(180, 730),
            &[],
        ),
        tool(
            "strength_sessions",
            "Strength sessions set by set: work sets, reps, time working, rest between sets and the work:rest ratio. The data has no load, so do not discuss volume in kilograms. Exercise names are guesses from wrist motion.",
            json!({ "limit": { "type": "integer", "description": "How many sessions. Default 10, capped at 50." } }),
            &[],
        ),
        tool(
            "routes",
            "Routes built from cached GPS traces, grouped when outings start and end in the same place over a similar distance. Only outdoor activities have a trace, so an empty result can be normal.",
            none.clone(),
            &[],
        ),
        tool(
            "workouts",
            "The person's saved Garmin workouts, the structured sessions they built. Read them to see what they already have.",
            none.clone(),
            &[],
        ),
        tool(
            "personal_records",
            "Garmin's personal records across every sport. A record with a null label is one the app does not know: skip it.",
            none.clone(),
            &[],
        ),
        tool(
            "fitness",
            "Garmin's own view: training status, acute and chronic load, the load ratio, the load balance against Garmin's target ranges, VO2 max and race predictions. A null VO2 max means Garmin has no data for it, not poor fitness.",
            days(30, 365),
            &[],
        ),
        tool(
            "coach",
            "What the app's coach says today without a question, and the week against the person's goals. Empty means nothing is worth raising. Call it so that your answer builds on what they already heard.",
            none.clone(),
            &[],
        ),
        tool(
            "findings",
            "What a year of history says when asked properly: trends in fitness, which recorded metric moves overnight HRV most, which weekday is skipped, how rest days differ from training days. Each finding has `claim`, `detail`, `basis` and usually an `estimate` with `low`, `high` and `n`. Quote the interval with the number. An empty list means nothing cleared the bar. That is a normal result.",
            days(365, 365),
            &[],
        ),
        tool(
            "cache_status",
            "What is in the local cache and how current it is. The grounding message has these numbers already. Call it only to re-check after a sync.",
            none,
            &[],
        ),
    ])
}

/// The label of a tool call, shown to the person while the model works. It
/// says which data a question touched, with the window that was read.
pub(crate) fn describe(name: &str, args: &Value) -> String {
    match name {
        "recent_activities" => {
            let n = window(args, "limit", 10, MAX_ACTIVITIES);
            match args["sport"].as_str() {
                Some(s) => format!("Reading your last {n} {s} sessions"),
                None => format!("Reading your last {n} activities"),
            }
        }
        "activity_zones" => "Reading one activity's zone breakdown".into(),
        "list_tags" => "Reading the tags you used".into(),
        "tagged_activities" => match args["tag"].as_str() {
            Some(t) => format!("Reading your \"{t}\" sessions"),
            None => "Reading tagged sessions".into(),
        },
        "recovery" => format!(
            "Reading {} days of recovery signals",
            window(args, "days", 14, MAX_DAYS)
        ),
        "nutrition" => format!(
            "Reading {} days of food and hydration",
            window(args, "days", 30, MAX_DAYS)
        ),
        "weight" => format!(
            "Reading {} days of weigh-ins",
            window(args, "days", 180, MAX_WEIGHT_DAYS)
        ),
        "sleep" => format!(
            "Reading last night and {} nights behind it",
            window(args, "days", 30, MAX_DAYS)
        ),
        "workouts" => "Reading your saved Garmin workouts".into(),
        "routes" => "Reading your cached GPS routes".into(),
        "strength_sessions" => "Reading your strength sessions set by set".into(),
        "personal_records" => "Reading your personal records".into(),
        "fitness" => "Reading Garmin's training status and load balance".into(),
        "coach" => "Checking what the coach already says".into(),
        "findings" => "Reading a year of history".into(),
        "cache_status" => "Checking what is cached".into(),
        other => format!("Running {other}"),
    }
}

/// Run one tool against the cache. An error goes back as JSON and does not end
/// the turn: the model can explain "that failed" better than a stack trace.
pub(crate) fn run(name: &str, args: &Value, open_db: &dyn Fn() -> Result<Db>) -> Value {
    let db = match open_db() {
        Ok(db) => db,
        Err(e) => return json!({ "error": format!("could not open the cache: {e}") }),
    };
    read(&db, name, args).unwrap_or_else(|e| json!({ "error": e.to_string() }))
}

fn to<T: serde::Serialize>(v: T) -> Result<Value> {
    Ok(serde_json::to_value(v)?)
}

/// The cache status. Tests must not read the OS keyring, so they report
/// "not connected" without looking.
pub(crate) fn cache_status(db: &Db) -> Result<query::CacheStatus> {
    if cfg!(test) {
        return query::cache_status_with(db, false);
    }
    query::cache_status(db)
}

fn read(db: &Db, name: &str, args: &Value) -> Result<Value> {
    Ok(match name {
        "recent_activities" => to(query::recent_activities(
            db,
            window(args, "limit", 10, MAX_ACTIVITIES),
            args["sport"].as_str(),
        )?)?,
        "activity_zones" => match query::activity_zones(db, args["activity_id"].as_i64())? {
            Some(v) => to(v)?,
            None => json!({ "error": "no such activity in the cache" }),
        },
        "list_tags" => to(db
            .all_tags()?
            .into_iter()
            .map(|(tag, count)| json!({ "tag": tag, "activities": count }))
            .collect::<Vec<_>>())?,
        "tagged_activities" => match args["tag"].as_str() {
            Some(tag) => to(db
                .activities_with_tag(tag, window(args, "limit", 20, MAX_ACTIVITIES))?
                .iter()
                .map(query::ActivityView::from)
                .collect::<Vec<_>>())?,
            None => json!({ "error": "tagged_activities needs a tag" }),
        },
        "recovery" => to(query::recovery(db, window(args, "days", 14, MAX_DAYS))?)?,
        // `brief` drops the hypnogram and the heart-rate curve: they are for charts.
        "sleep" => to(query::sleep(db, window(args, "days", 30, MAX_DAYS))?.brief(30))?,
        "nutrition" => to(query::nutrition(db, window(args, "days", 30, MAX_DAYS))?)?,
        "weight" => to(query::weight(
            db,
            window(args, "days", 180, MAX_WEIGHT_DAYS),
        )?)?,
        "strength_sessions" => to(query::strength_trend(
            db,
            window(args, "limit", 10, MAX_ACTIVITIES),
        )?)?,
        "routes" => to(query::route_summaries(db)?)?,
        "workouts" => to(db.workouts()?)?,
        "personal_records" => to(query::personal_records(db)?)?,
        "fitness" => to(query::fitness(db, window(args, "days", 30, MAX_DAYS))?)?,
        "coach" => to(garmin_core::coach::for_today(
            db,
            chrono::Local::now().date_naive(),
        )?)?,
        "findings" => {
            let from = garmin_core::days_ago(window(args, "days", 365, MAX_DAYS));
            to(garmin_core::findings::all(
                &db.daily_since(&from)?,
                &db.activities_since(&from)?,
                chrono::Local::now().date_naive(),
            ))?
        }
        "cache_status" => to(cache_status(db)?)?,
        other => json!({ "error": format!("unknown tool {other}") }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schema_has_a_label_and_a_runner() {
        let db = Db::open(std::path::Path::new(":memory:")).unwrap();
        for t in schemas().as_array().unwrap() {
            let name = t["function"]["name"].as_str().unwrap();
            assert!(!describe(name, &json!({})).starts_with("Running"), "{name}");
            let out = read(&db, name, &json!({ "tag": "x" })).unwrap();
            let unknown = out["error"]
                .as_str()
                .is_some_and(|e| e.starts_with("unknown tool"));
            assert!(!unknown, "{name} has no runner");
        }
    }

    #[test]
    fn a_window_is_clamped() {
        assert_eq!(window(&json!({ "days": 5000 }), "days", 14, MAX_DAYS), 365);
        assert_eq!(window(&json!({ "days": 0 }), "days", 14, MAX_DAYS), 1);
        assert_eq!(window(&json!({}), "days", 14, MAX_DAYS), 14);
    }

    #[test]
    fn no_tool_text_leans_on_running() {
        let text = schemas().to_string().to_lowercase();
        for word in ["runner", "cadence", "pace", "per kilometre", "run/walk"] {
            assert!(!text.contains(word), "{word}");
        }
    }

    #[test]
    fn an_unopenable_cache_is_an_error_value() {
        let out = run("recovery", &json!({}), &|| Err(anyhow::anyhow!("no")));
        assert!(out["error"].as_str().unwrap().contains("could not open"));
    }
}
