//! Sample mode: recorded view models for an app with no Garmin account.
//!
//! The commands here return the files in `fixtures/` and never open the cache,
//! because opening it creates the file and ends sample mode.

use serde_json::Value;

use crate::ApiError;

const TODAY: &str = include_str!("../fixtures/today.sample.json");
const ACTIVITIES: &str = include_str!("../fixtures/activities.sample.json");
const ACTIVITY: &str = include_str!("../fixtures/activity.sample.json");
const HEALTH: &str = include_str!("../fixtures/health.sample.json");
const SLEEP: &str = include_str!("../fixtures/sleep.sample.json");
const BODY: &str = include_str!("../fixtures/body.sample.json");
const PLACES: &str = include_str!("../fixtures/places.sample.json");
const TRAINING: &str = include_str!("../fixtures/training.sample.json");

/// Commands that read the cache and have no recorded result. Sample mode
/// refuses them, so a call cannot create the cache.
const NEEDS_CACHE: &[&str] = &[
    "cached_daily",
    "cached_activities",
    "cached_activities_since",
    "cached_activity",
    "nutrition",
    "weight",
    "tiles_view",
    "extras_view",
    "charts_view",
];

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_default()
}

/// The recorded result of a command, `None` when the command has no sample.
/// A command with no sample and no cache need goes to the live path.
pub(crate) fn call(cmd: &str, args: &Value) -> Option<Result<String, ApiError>> {
    let out = match cmd {
        "today" => TODAY.to_owned(),
        "activities" => ACTIVITIES.to_owned(),
        "activity_detail" => detail(args),
        "weight_view" | "food_view" | "strength_view" => part(BODY, cmd),
        "health_view" => HEALTH.to_owned(),
        "sleep_view" => SLEEP.to_owned(),
        "insights_view" | "fitness_view" | "report_view" => training(cmd, args),
        "plan_view" | "routes_view" | "gear_view" => part(PLACES, cmd),
        c if NEEDS_CACHE.contains(&c) => {
            return Some(Err(ApiError::BadArguments(
                "sample mode has no cache".into(),
            )))
        }
        _ => return None,
    };
    Some(Ok(out))
}

/// The sample detail with the requested id, so the list and the detail agree.
fn detail(args: &Value) -> String {
    let mut detail = json(ACTIVITY);
    if let Some(id) = args.get("activityId").filter(|v| v.is_i64()) {
        detail["activityId"] = id.clone();
    }
    detail.to_string()
}

/// One part of a file that holds several views: `weight`, `plan` and so on.
fn part(file: &str, cmd: &str) -> String {
    json(file)[cmd.trim_end_matches("_view")].to_string()
}

/// One part of `training.sample.json`. The report picks its week by `offset`.
fn training(cmd: &str, args: &Value) -> String {
    let all = json(TRAINING);
    match cmd {
        "insights_view" => all["insights"].to_string(),
        "fitness_view" => all["fitness"].to_string(),
        _ => {
            let weeks = all["reports"].as_array().map_or(0, Vec::len);
            let offset = args["offset"].as_u64().unwrap_or(0) as usize;
            all["reports"][offset.min(weeks.saturating_sub(1))].to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sample_is_json_and_the_detail_takes_the_asked_id() {
        let none = serde_json::json!({});
        for cmd in [
            "today",
            "activities",
            "activity_detail",
            "health_view",
            "sleep_view",
            "weight_view",
            "food_view",
            "strength_view",
            "insights_view",
            "fitness_view",
            "report_view",
            "plan_view",
            "routes_view",
            "gear_view",
        ] {
            let out = call(cmd, &serde_json::json!({ "offset": 99 }))
                .unwrap()
                .unwrap();
            assert!(
                serde_json::from_str::<Value>(&out).unwrap().is_object(),
                "{cmd}"
            );
        }
        let d = call("activity_detail", &serde_json::json!({ "activityId": 42 }))
            .unwrap()
            .unwrap();
        assert_eq!(json(&d)["activityId"], 42);
        assert!(call("nope", &none).is_none());
    }

    #[test]
    fn a_command_that_needs_the_cache_is_refused() {
        for cmd in NEEDS_CACHE {
            let out = call(cmd, &serde_json::json!({})).unwrap();
            assert!(matches!(out, Err(ApiError::BadArguments(_))), "{cmd}");
        }
    }
}
