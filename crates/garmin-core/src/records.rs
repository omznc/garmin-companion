//! Personal records, and Garmin's own verdict on the training.
//!
//! Two things Garmin computes that this app previously ignored: the PR list,
//! and the training-status block that carries acute/chronic load, the
//! acute:chronic ratio, the aerobic/anaerobic load balance and VO2 max.
//!
//! The load balance is the interesting one here. Garmin scores a month's work
//! into aerobic-low, aerobic-high and anaerobic buckets and sets a target range
//! for each — so "am I doing too much hard running" has an answer from Garmin's
//! own numbers, not only from this app's zone arithmetic.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a PR row's `typeId` means.
///
/// Garmin publishes no list, so this is what the ids have been observed to be.
/// Deliberately partial: an unrecognised id keeps its number and gets no label,
/// rather than being guessed at and shown to the athlete as a fact.
///
/// `unit` tells the caller how to read `value` — a time record's value is
/// seconds, a distance record's is metres, and a step record's is a count.
pub fn record_label(type_id: i64) -> Option<(&'static str, RecordUnit)> {
    Some(match type_id {
        1 => ("Fastest 1 km", RecordUnit::Seconds),
        2 => ("Fastest 1 mile", RecordUnit::Seconds),
        3 => ("Fastest 5 km", RecordUnit::Seconds),
        4 => ("Fastest 10 km", RecordUnit::Seconds),
        5 => ("Fastest half marathon", RecordUnit::Seconds),
        6 => ("Fastest marathon", RecordUnit::Seconds),
        7 => ("Longest run", RecordUnit::Metres),
        8 => ("Longest ride", RecordUnit::Metres),
        9 => ("Biggest climb on a ride", RecordUnit::Metres),
        12 => ("Most steps in a day", RecordUnit::Count),
        13 => ("Most steps in a week", RecordUnit::Count),
        14 => ("Most steps in a month", RecordUnit::Count),
        15 => ("Longest step-goal streak", RecordUnit::Days),
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordUnit {
    Seconds,
    Metres,
    Count,
    Days,
}

/// One personal record, as the cache keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalRecord {
    /// Garmin's own row id, which is what makes a re-sync idempotent.
    pub record_id: i64,
    pub type_id: i64,
    /// `None` for a `type_id` this build doesn't recognise. The row is still
    /// stored — a label can be added later without re-syncing.
    pub label: Option<String>,
    pub unit: Option<RecordUnit>,
    pub value: f64,
    /// The session that set it. Zero for records Garmin computes from daily
    /// totals rather than one activity, such as the step records.
    pub activity_id: Option<i64>,
    pub activity_name: Option<String>,
    pub activity_type: Option<String>,
    /// Local date the record was set, `YYYY-MM-DD`.
    pub set_on: Option<String>,
}

/// Parse the PR list endpoint. Rows without a value or an id are dropped.
pub fn parse_records(rows: &[Value]) -> Vec<PersonalRecord> {
    rows.iter()
        .filter_map(|r| {
            let labelled = record_label(r["typeId"].as_i64()?);
            Some(PersonalRecord {
                record_id: r["id"].as_i64()?,
                type_id: r["typeId"].as_i64()?,
                label: labelled.map(|(l, _)| l.to_string()),
                unit: labelled.map(|(_, u)| u),
                value: r["value"].as_f64()?,
                // Garmin sends 0 rather than null for a record no single
                // activity owns; null is the honest shape for that.
                activity_id: r["activityId"].as_i64().filter(|id| *id > 0),
                activity_name: r["activityName"].as_str().map(str::to_owned),
                activity_type: r["activityType"].as_str().map(str::to_owned),
                set_on: r["prStartTimeLocalFormatted"]
                    .as_str()
                    .or_else(|| r["activityStartDateTimeLocalFormatted"].as_str())
                    .and_then(|s| s.get(..10))
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// Garmin's training verdict for one day.
///
/// Every field is optional because the whole block is: it comes from the watch,
/// nests under a device id, and an account that hasn't synced recently has none
/// of it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingStatus {
    pub date: Option<String>,
    /// Garmin's status code, and the phrase it pairs with — `RECOVERY_1`,
    /// `PRODUCTIVE_1`, `UNPRODUCTIVE_2` and so on. Kept raw; the frontend
    /// decides how to word them.
    pub status: Option<i64>,
    pub status_phrase: Option<String>,
    /// Load over roughly the last week.
    pub acute_load: Option<f64>,
    /// Load over roughly the last month. The baseline `acute_load` is judged
    /// against.
    pub chronic_load: Option<f64>,
    /// Acute divided by chronic. Below ~0.8 is detraining, 0.8–1.3 is the
    /// productive band, above ~1.5 is where injury risk climbs.
    pub acwr: Option<f64>,
    pub acwr_status: Option<String>,
    /// A month of load split into buckets, each against the range Garmin wants
    /// it in. This is the 80/20 argument in Garmin's own terms.
    pub aerobic_low: Option<f64>,
    pub aerobic_low_target_min: Option<f64>,
    pub aerobic_low_target_max: Option<f64>,
    pub aerobic_high: Option<f64>,
    pub aerobic_high_target_min: Option<f64>,
    pub aerobic_high_target_max: Option<f64>,
    pub anaerobic: Option<f64>,
    pub anaerobic_target_min: Option<f64>,
    pub anaerobic_target_max: Option<f64>,
    /// `BALANCED`, `ANAEROBIC_FOCUS`, `LOW_AEROBIC_SHORTAGE`, …
    pub balance_phrase: Option<String>,
    /// Running VO2 max. Stays null until an outdoor GPS run exists; a treadmill
    /// never populates it.
    pub vo2max: Option<f64>,
}

impl TrainingStatus {
    /// Whether anything at all came back. A block of pure nulls is not worth
    /// writing to the cache.
    pub fn has_data(&self) -> bool {
        self.status.is_some()
            || self.acute_load.is_some()
            || self.chronic_load.is_some()
            || self.anaerobic.is_some()
            || self.vo2max.is_some()
    }

    /// Whether the month's anaerobic load has gone past the top of Garmin's
    /// target range — the signal that hard running has crowded out easy.
    pub fn anaerobic_over_target(&self) -> bool {
        matches!(
            (self.anaerobic, self.anaerobic_target_max),
            (Some(a), Some(max)) if a > max
        )
    }
}

/// Pull the fields worth keeping out of the training-status payload.
///
/// The nesting is Garmin's: status and load balance each sit in a map keyed by
/// device id, so this takes the entry flagged `primaryTrainingDevice` and falls
/// back to whatever the first one is.
pub fn parse_training_status(v: &Value) -> TrainingStatus {
    let primary = |block: &Value| -> Option<Value> {
        let map = block.as_object()?;
        map.values()
            .find(|d| d["primaryTrainingDevice"].as_bool() == Some(true))
            .or_else(|| map.values().next())
            .cloned()
    };

    let status =
        primary(&v["mostRecentTrainingStatus"]["latestTrainingStatusData"]).unwrap_or(Value::Null);
    let balance = primary(&v["mostRecentTrainingLoadBalance"]["metricsTrainingLoadBalanceDTOMap"])
        .unwrap_or(Value::Null);
    let acute = &status["acuteTrainingLoadDTO"];

    TrainingStatus {
        date: status["calendarDate"]
            .as_str()
            .or_else(|| balance["calendarDate"].as_str())
            .map(str::to_owned),
        status: status["trainingStatus"].as_i64(),
        status_phrase: status["trainingStatusFeedbackPhrase"]
            .as_str()
            .map(str::to_owned),
        acute_load: acute["dailyTrainingLoadAcute"].as_f64(),
        chronic_load: acute["dailyTrainingLoadChronic"].as_f64(),
        acwr: acute["dailyAcuteChronicWorkloadRatio"].as_f64(),
        acwr_status: acute["acwrStatus"].as_str().map(str::to_owned),
        aerobic_low: balance["monthlyLoadAerobicLow"].as_f64(),
        aerobic_low_target_min: balance["monthlyLoadAerobicLowTargetMin"].as_f64(),
        aerobic_low_target_max: balance["monthlyLoadAerobicLowTargetMax"].as_f64(),
        aerobic_high: balance["monthlyLoadAerobicHigh"].as_f64(),
        aerobic_high_target_min: balance["monthlyLoadAerobicHighTargetMin"].as_f64(),
        aerobic_high_target_max: balance["monthlyLoadAerobicHighTargetMax"].as_f64(),
        anaerobic: balance["monthlyLoadAnaerobic"].as_f64(),
        anaerobic_target_min: balance["monthlyLoadAnaerobicTargetMin"].as_f64(),
        anaerobic_target_max: balance["monthlyLoadAnaerobicTargetMax"].as_f64(),
        balance_phrase: balance["trainingBalanceFeedbackPhrase"]
            .as_str()
            .map(str::to_owned),
        // Running VO2 max, past two layers of Garmin's naming.
        vo2max: v["mostRecentVO2Max"]["generic"]["vo2MaxPreciseValue"]
            .as_f64()
            .or_else(|| v["mostRecentVO2Max"]["generic"]["vo2MaxValue"].as_f64()),
    }
}

/// Predicted finishing times, in seconds.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RacePredictions {
    pub date: Option<String>,
    pub time_5k_s: Option<f64>,
    pub time_10k_s: Option<f64>,
    pub time_half_s: Option<f64>,
    pub time_marathon_s: Option<f64>,
}

impl RacePredictions {
    pub fn has_data(&self) -> bool {
        self.time_5k_s.is_some()
            || self.time_10k_s.is_some()
            || self.time_half_s.is_some()
            || self.time_marathon_s.is_some()
    }
}

pub fn parse_race_predictions(v: &Value) -> RacePredictions {
    RacePredictions {
        date: v["calendarDate"].as_str().map(str::to_owned),
        time_5k_s: v["time5K"].as_f64(),
        time_10k_s: v["time10K"].as_f64(),
        time_half_s: v["timeHalfMarathon"].as_f64(),
        time_marathon_s: v["timeMarathon"].as_f64(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_drop_the_zero_activity_id() {
        let rows = vec![json!({
            "id": 2749249783_i64, "typeId": 12, "value": 19166.0,
            "activityId": 0, "activityName": null, "activityType": null,
            "prStartTimeLocalFormatted": "2025-11-16T00:00:00.0"
        })];
        let parsed = parse_records(&rows);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].activity_id, None);
        assert_eq!(parsed[0].label.as_deref(), Some("Most steps in a day"));
        assert_eq!(parsed[0].set_on.as_deref(), Some("2025-11-16"));
    }

    #[test]
    fn an_unknown_type_id_survives_without_a_label() {
        let rows = vec![json!({ "id": 1, "typeId": 28, "value": 92000.0 })];
        let parsed = parse_records(&rows);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].label, None);
        assert_eq!(parsed[0].type_id, 28);
    }

    #[test]
    fn training_status_reads_through_the_device_map() {
        let v = json!({
            "mostRecentTrainingStatus": { "latestTrainingStatusData": { "3605474816": {
                "calendarDate": "2026-08-08",
                "primaryTrainingDevice": true,
                "trainingStatus": 5,
                "trainingStatusFeedbackPhrase": "RECOVERY_1",
                "acuteTrainingLoadDTO": {
                    "dailyTrainingLoadAcute": 151,
                    "dailyTrainingLoadChronic": 318,
                    "dailyAcuteChronicWorkloadRatio": 0.4,
                    "acwrStatus": "LOW"
                }
            }}},
            "mostRecentTrainingLoadBalance": { "metricsTrainingLoadBalanceDTOMap": { "3605474816": {
                "primaryTrainingDevice": true,
                "monthlyLoadAnaerobic": 473.2355,
                "monthlyLoadAnaerobicTargetMin": 133,
                "monthlyLoadAnaerobicTargetMax": 400,
                "trainingBalanceFeedbackPhrase": "ANAEROBIC_FOCUS"
            }}},
            "mostRecentVO2Max": { "generic": null }
        });
        let s = parse_training_status(&v);
        assert!(s.has_data());
        assert_eq!(s.acwr, Some(0.4));
        assert_eq!(s.status_phrase.as_deref(), Some("RECOVERY_1"));
        assert!(s.anaerobic_over_target());
        // Null VO2 max is the expected state for a treadmill-only account.
        assert_eq!(s.vo2max, None);
    }

    #[test]
    fn an_empty_payload_reports_no_data() {
        assert!(!parse_training_status(&json!({})).has_data());
        assert!(!parse_race_predictions(&json!({})).has_data());
    }
}
