//! The tools that the model can call. Each one reads a bounded slice of the
//! local cache through `garmin_core::query`, the same code as the MCP server.
//!
//! Left out of this first version: drafting a workout, asking the person a
//! question, themes, and the close read of one session (it can call Garmin).
//! `zone_drift` and `cadence_trend` are also out. Both are about running, and
//! the coach must not lean on one sport. They return when the data gates them.

use anyhow::Result;
use chrono::NaiveDate;
use garmin_core::{db::Db, plural, query};
use serde_json::{json, Value};

const MAX_ACTIVITIES: u64 = 50;
/// For `sleep` and `findings`: their rows are small or are summaries.
const MAX_DAYS: u64 = 365;
/// For `recovery`, `nutrition` and `fitness`: one row a day, and a row is large.
const MAX_ROW_DAYS: u64 = 120;
const MAX_WEIGHT_DAYS: u64 = 730;
/// The largest tool result that goes to the model. The hosted coach refuses a
/// request body over 100,000 bytes, and the results of a turn share it.
const MAX_RESULT_BYTES: usize = 25_000;

/// A slice of a table, held between 1 and `max`. The model picks the number,
/// and the result goes back on every later round, so a large guess is clamped
/// and not rejected.
pub(crate) fn window(args: &Value, key: &str, default: u64, max: u64) -> u32 {
    args[key].as_u64().unwrap_or(default).clamp(1, max) as u32
}

fn day_count(n: u32) -> String {
    plural(n as usize, "day", "days")
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

fn days(default: u64, max: u64) -> Value {
    json!({ "days": { "type": "integer", "description": format!("How many days back from today. Default {default}, capped at {max}.") } })
}

/// The tool that proposes a workout. It does not read the cache and it saves
/// nothing: the host shows the draft and the person saves it.
pub(crate) const DRAFT_TOOL: &str = "draft_workout";

/// The tool surface offered to the model. `drafts` adds the tool that proposes
/// a workout, for a host that can show one.
pub(crate) fn schemas(drafts: bool) -> Value {
    let none = json!({});
    let mut tools = vec![
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
            days(14, MAX_ROW_DAYS),
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
            days(30, MAX_ROW_DAYS),
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
            days(30, MAX_ROW_DAYS),
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
    ];
    if drafts {
        tools.push(tool(
            DRAFT_TOOL,
            "Propose a structured workout for the person to review. This SAVES NOTHING: it shows the session as a card, and the person saves it to Garmin themselves. Call it when they ask for a session or a workout. Read their recent activities and recovery first.",
            garmin_core::workout::schema()["properties"].clone(),
            &["name", "sport", "steps"],
        ));
    }
    Value::Array(tools)
}

/// Run `draft_workout`: the model gets a thin result, and the same parse gives
/// the host the draft. The steps are not echoed back, so the model does not
/// list them in prose next to the card.
fn draft_result(args: &Value) -> Value {
    match garmin_core::workout::parse_draft(args) {
        Ok(d) => json!({
            "ok": true,
            "name": d.name,
            "steps": d.flat_count(),
            "estimatedMinutes": d.est_duration_secs().map(|s| (s / 60.0).round()),
            "status": "Shown to the person for review. Nothing is saved to Garmin, and nothing will be unless they press the button on it.",
        }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

/// The label of a tool call, shown to the person while the model works. It
/// says which data a question touched, with the window that was read.
pub(crate) fn describe(name: &str, args: &Value) -> String {
    match name {
        "recent_activities" => {
            let n = window(args, "limit", 10, MAX_ACTIVITIES);
            match args["sport"].as_str() {
                Some(s) if n == 1 => format!("Reading your last {s} session"),
                Some(s) => format!("Reading your last {n} {s} sessions"),
                None if n == 1 => "Reading your last activity".into(),
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
            "Reading {} of recovery signals",
            day_count(window(args, "days", 14, MAX_ROW_DAYS))
        ),
        "nutrition" => format!(
            "Reading {} of food and hydration",
            day_count(window(args, "days", 30, MAX_ROW_DAYS))
        ),
        "weight" => format!(
            "Reading {} of weigh-ins",
            day_count(window(args, "days", 180, MAX_WEIGHT_DAYS))
        ),
        "sleep" => format!(
            "Reading last night and {} behind it",
            plural(
                window(args, "days", 30, MAX_DAYS) as usize,
                "night",
                "nights"
            )
        ),
        "workouts" => "Reading your saved Garmin workouts".into(),
        "routes" => "Reading your cached GPS routes".into(),
        "strength_sessions" => "Reading your strength sessions set by set".into(),
        "personal_records" => "Reading your personal records".into(),
        "fitness" => "Reading Garmin's training status and load balance".into(),
        "coach" => "Checking what the coach already says".into(),
        "findings" => "Reading a year of history".into(),
        "cache_status" => "Checking what is cached".into(),
        DRAFT_TOOL => "Putting a session together".into(),
        other => format!("Running {other}"),
    }
}

/// Run one tool against the cache. An error goes back as JSON and does not end
/// the turn: the model can explain "that failed" better than a stack trace.
pub(crate) fn run(
    name: &str,
    args: &Value,
    open_db: &dyn Fn() -> Result<Db>,
    period: Option<Period>,
) -> Value {
    let db = match open_db() {
        Ok(db) => db,
        Err(e) => return json!({ "error": format!("could not open the cache: {e}") }),
    };
    let result = match period {
        None => read(&db, name, args),
        Some(p) => {
            // The window ends on the last day of the period. `days` never
            // goes past the period, and a call without `days` reads all of it.
            let mut args = args.clone();
            let asked = args["days"].as_u64().unwrap_or(u64::MAX);
            args["days"] = json!(asked.min(p.days()));
            garmin_core::with_as_of(p.end, || read(&db, name, &args))
        }
    };
    capped(result.unwrap_or_else(|e| json!({ "error": e.to_string() })))
}

/// A span of days that bounds what the tools read. With a period, "today" for
/// every tool is `end`, and no row after `end` is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl Period {
    /// The days from `start` to `end`, both ends counted. At least 1.
    pub fn days(&self) -> u64 {
        ((self.end - self.start).num_days() + 1).max(1) as u64
    }
}

/// Room for the note that says rows were dropped.
const NOTE_BYTES: usize = 200;

/// The largest list in a result: the rows that can be dropped.
fn rows_of(out: &mut Value) -> Option<&mut Vec<Value>> {
    out.as_object_mut()?
        .values_mut()
        .filter_map(Value::as_array_mut)
        .max_by_key(|rows| rows.iter().map(|r| r.to_string().len()).sum::<usize>())
}

/// A result over `MAX_RESULT_BYTES` loses its oldest rows (every list is newest
/// first) and gets a note. When no list can shrink enough, it becomes an error
/// that tells the model to ask for less. The result goes back on every later
/// round, so one large result would push the request over the body limit of the
/// hosted coach.
pub(crate) fn capped(mut out: Value) -> Value {
    let mut size = out.to_string().len();
    if size <= MAX_RESULT_BYTES {
        return out;
    }
    if out.is_array() {
        out = json!({ "rows": out });
    }
    let total = rows_of(&mut out).map_or(0, |rows| rows.len());
    let mut kept = total;
    while size > MAX_RESULT_BYTES - NOTE_BYTES {
        let Some(rows) = rows_of(&mut out) else { break };
        if rows.len() <= 1 {
            break;
        }
        kept = (rows.len() * (MAX_RESULT_BYTES - NOTE_BYTES) / size).clamp(1, rows.len() - 1);
        rows.truncate(kept);
        size = out.to_string().len();
    }
    if size > MAX_RESULT_BYTES - NOTE_BYTES {
        return json!({ "error": format!(
            "The result is too large ({} KB) to read. Call this tool again with fewer days or a smaller limit.",
            size / 1000
        ) });
    }
    out["truncated"] = json!(format!(
        "Only the newest {kept} of {total} rows fit. Ask for fewer days to see them all."
    ));
    out
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
        "recovery" => to(query::recovery(db, window(args, "days", 14, MAX_ROW_DAYS))?)?,
        // `brief` drops the hypnogram and the heart-rate curve: they are for charts.
        "sleep" => to(query::sleep(db, window(args, "days", 30, MAX_DAYS))?.brief(30))?,
        "nutrition" => to(query::nutrition(
            db,
            window(args, "days", 30, MAX_ROW_DAYS),
        )?)?,
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
        "fitness" => to(query::fitness(db, window(args, "days", 30, MAX_ROW_DAYS))?)?,
        "coach" => to(garmin_core::coach::for_today(
            db,
            chrono::Local::now().date_naive(),
        )?)?,
        "findings" => {
            let from = garmin_core::days_ago(window(args, "days", 365, MAX_DAYS));
            to(garmin_core::findings::all(
                &db.daily_since(&from)?,
                &db.activities_since(&from)?,
                garmin_core::today(),
            ))?
        }
        "cache_status" => to(cache_status(db)?)?,
        DRAFT_TOOL => draft_result(args),
        other => json!({ "error": format!("unknown tool {other}") }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_use_the_singular_for_one() {
        assert_eq!(
            describe("recent_activities", &json!({ "limit": 1 })),
            "Reading your last activity"
        );
        assert_eq!(
            describe("recent_activities", &json!({ "limit": 1, "sport": "run" })),
            "Reading your last run session"
        );
        assert_eq!(
            describe("recovery", &json!({ "days": 1 })),
            "Reading 1 day of recovery signals"
        );
        assert_eq!(
            describe("weight", &json!({ "days": 1 })),
            "Reading 1 day of weigh-ins"
        );
        assert_eq!(
            describe("sleep", &json!({ "days": 1 })),
            "Reading last night and 1 night behind it"
        );
        assert_eq!(
            describe("sleep", &json!({ "days": 5 })),
            "Reading last night and 5 nights behind it"
        );
    }

    #[test]
    fn every_schema_has_a_label_and_a_runner() {
        let db = Db::open(std::path::Path::new(":memory:")).unwrap();
        for t in schemas(true).as_array().unwrap() {
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
    fn a_large_result_becomes_an_error_that_asks_for_less() {
        let big = json!({ "rows": "x".repeat(MAX_RESULT_BYTES) });
        let out = capped(big);
        assert!(out["error"].as_str().unwrap().contains("fewer days"));
        assert!(out.to_string().len() < 300);
        let small = json!({ "rows": [1, 2, 3] });
        assert_eq!(capped(small.clone()), small);
    }

    #[test]
    fn a_large_list_loses_its_oldest_rows() {
        let rows: Vec<Value> = (0..100)
            .map(|i| json!({ "day": i, "pad": "é".repeat(500) }))
            .collect();
        let out = capped(json!({ "nights": rows, "averages": { "a": 1 } }));
        assert!(out.to_string().len() <= MAX_RESULT_BYTES);
        let kept = out["nights"].as_array().unwrap();
        assert!(kept.len() < 100 && kept.len() > 10);
        // Newest first: the head stays.
        assert_eq!(kept[0]["day"], 0);
        assert_eq!(out["averages"]["a"], 1);
        assert!(out["truncated"].as_str().unwrap().contains("of 100 rows"));
        // A bare list is wrapped so that the note has a place.
        let bare = capped(json!(vec![json!({ "pad": "x".repeat(10_000) }); 5]));
        assert!(bare["rows"].as_array().unwrap().len() < 5);
    }

    #[test]
    fn the_row_tools_stop_at_120_days() {
        let text = schemas(true).to_string();
        assert_eq!(text.matches("capped at 120").count(), 3);
        assert_eq!(
            window(&json!({ "days": 365 }), "days", 14, MAX_ROW_DAYS),
            120
        );
    }

    #[test]
    fn no_tool_text_leans_on_running() {
        let text = schemas(true).to_string().to_lowercase();
        for word in ["runner", "cadence", "pace", "per kilometre", "run/walk"] {
            assert!(!text.contains(word), "{word}");
        }
    }

    #[test]
    fn the_draft_tool_is_offered_only_when_drafts_are_on() {
        let has = |on| schemas(on).to_string().contains(DRAFT_TOOL);
        assert!(has(true));
        assert!(!has(false));
        assert_eq!(
            describe(DRAFT_TOOL, &json!({})),
            "Putting a session together"
        );
    }

    #[test]
    fn the_draft_result_is_thin_and_an_error_names_the_step() {
        let ok = draft_result(&json!({
            "name": "Easy", "sport": "running",
            "steps": [{ "kind": "interval", "end": { "seconds": 600 } }]
        }));
        assert_eq!(ok["ok"], true);
        assert_eq!(ok["steps"], 1);
        assert_eq!(ok["estimatedMinutes"], 10.0);
        assert!(ok.get("draft").is_none());
        let bad = draft_result(&json!({
            "name": "Long", "sport": "running",
            "steps": [{ "kind": "interval", "end": { "seconds": 90000 } }]
        }));
        assert!(bad["error"].as_str().unwrap().contains("step 1"));
    }

    /// Ten days in March 2026, one a day, `resting_hr` 1 to 10.
    fn march_db(tag: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("coach-period-{tag}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).unwrap();
        for d in 1..=10 {
            db.upsert_daily(&garmin_core::db::DailyMetrics {
                date: format!("2026-03-{d:02}"),
                resting_hr: Some(d as f64),
                ..Default::default()
            })
            .unwrap();
        }
        path
    }

    fn dates(out: &Value) -> Vec<String> {
        out.as_array()
            .unwrap()
            .iter()
            .map(|r| r["date"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn a_period_ends_and_starts_the_window() {
        let path = march_db("window");
        let open = || Db::open(&path);
        let day = |d| NaiveDate::from_ymd_opt(2026, 3, d).unwrap();
        let period = Period {
            start: day(4),
            end: day(7),
        };
        // No `days`: the whole period, and nothing after its end.
        let out = run("recovery", &json!({}), &open, Some(period));
        assert_eq!(
            dates(&out),
            ["2026-03-07", "2026-03-06", "2026-03-05", "2026-03-04"]
        );
        // A smaller `days` is honoured and still ends at `end`.
        let out = run("recovery", &json!({ "days": 2 }), &open, Some(period));
        assert_eq!(dates(&out), ["2026-03-07", "2026-03-06"]);
        // A larger `days` is cut to the period.
        let out = run("recovery", &json!({ "days": 6 }), &open, Some(period));
        assert_eq!(dates(&out).first().unwrap(), "2026-03-07");
        assert_eq!(dates(&out).last().unwrap(), "2026-03-04");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn no_period_keeps_the_window_on_the_real_today() {
        let path = march_db("none");
        let open = || Db::open(&path);
        // March 2026 is far behind the real today: the old window is empty.
        let out = run("recovery", &json!({ "days": 14 }), &open, None);
        let db = open().unwrap();
        let direct = read(&db, "recovery", &json!({ "days": 14 })).unwrap();
        assert_eq!(out, direct);
        assert!(out.as_array().unwrap().is_empty());
        assert_eq!(garmin_core::today(), chrono::Local::now().date_naive());
        assert_eq!(garmin_core::until(), "9999-12-31");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_unopenable_cache_is_an_error_value() {
        let out = run("recovery", &json!({}), &|| Err(anyhow::anyhow!("no")), None);
        assert!(out["error"].as_str().unwrap().contains("could not open"));
    }
}
