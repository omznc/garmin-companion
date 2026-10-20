//! Curves inside one day: the overnight HRV readings and, later, Body Battery.
//!
//! A curve is large next to the daily figure, so it has its own table and the
//! sync keeps it for the recent days only. The daily rows stay the long record.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;

pub(crate) const SCHEMA: &str = r#"
    -- The overnight HRV readings of one night, as a JSON list. Kept for the
    -- recent days only: a reading is every five minutes, so a year of them
    -- would be thousands of rows that no screen asks for.
    CREATE TABLE IF NOT EXISTS hrv_readings (
        date     TEXT PRIMARY KEY,
        readings TEXT NOT NULL DEFAULT '[]'
    );

    -- The Body Battery curve of one day, as a JSON list of [minute, level].
    -- Same rule as above: the recent days only.
    CREATE TABLE IF NOT EXISTS body_battery_curve (
        date   TEXT PRIMARY KEY,
        points TEXT NOT NULL DEFAULT '[]'
    );
"#;

/// One Body Battery sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryPoint {
    /// Minutes after local midnight, so a chart needs no time zone.
    pub minute: u32,
    /// 0 to 100.
    pub level: f64,
}

/// The Body Battery samples in a `dailyStress` response.
///
/// Garmin sends each sample as a list and names the places in a descriptor
/// list, so the places are read from it and not assumed. A sample with no
/// level is skipped. The rest of the list stays.
pub fn parse_body_battery(v: &Value) -> Vec<BatteryPoint> {
    let Some(list) = v.get("bodyBatteryValuesArray").and_then(Value::as_array) else {
        return Vec::new();
    };
    let Some(start) = v
        .get("startTimestampGMT")
        .and_then(Value::as_str)
        .and_then(gmt_millis)
    else {
        return Vec::new();
    };
    let place = |key: &str, fallback: usize| {
        v.get("bodyBatteryValueDescriptorsDTOList")
            .and_then(Value::as_array)
            .and_then(|d| {
                d.iter().find(|e| {
                    e.get("bodyBatteryValueDescriptorKey")
                        .and_then(Value::as_str)
                        == Some(key)
                })
            })
            .and_then(|e| e.get("bodyBatteryValueDescriptorIndex")?.as_u64())
            .map_or(fallback, |i| i as usize)
    };
    let (at, level) = (place("timestamp", 0), place("bodyBatteryLevel", 2));
    list.iter()
        .filter_map(|s| {
            let ts = s.get(at)?.as_i64()?;
            let level = s.get(level)?.as_f64()?;
            let minute = u32::try_from((ts - start) / 60_000).ok()?;
            (minute < 24 * 60 + 60).then_some(BatteryPoint { minute, level })
        })
        .collect()
}

/// `2026-10-19T22:00:00.0` as epoch milliseconds.
fn gmt_millis(s: &str) -> Option<i64> {
    let t = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
    Some(t.and_utc().timestamp_millis())
}

/// One overnight HRV reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HrvReading {
    /// Local wall clock, `YYYY-MM-DDTHH:MM`.
    pub time: String,
    /// Milliseconds.
    pub value: f64,
}

/// The readings in an HRV response. An item without a value or a time is
/// skipped. The rest of the list stays.
pub fn parse_hrv_readings(v: &Value) -> Vec<HrvReading> {
    let Some(list) = v.get("hrvReadings").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|r| {
            let value = r.get("hrvValue")?.as_f64()?;
            let time = r.get("readingTimeLocal")?.as_str()?.get(..16)?;
            Some(HrvReading {
                time: time.to_owned(),
                value,
            })
        })
        .collect()
}

impl Db {
    /// Replace the readings of one night. An empty list writes nothing, so a
    /// late empty reply cannot erase a night that was kept.
    pub fn save_hrv_readings(&self, date: &str, readings: &[HrvReading]) -> Result<()> {
        if readings.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO hrv_readings (date, readings) VALUES (?1, ?2)",
            params![date, serde_json::to_string(readings)?],
        )?;
        Ok(())
    }

    /// Replace the Body Battery curve of one day. An empty list writes
    /// nothing, as for the HRV readings.
    pub fn save_body_battery_curve(&self, date: &str, points: &[BatteryPoint]) -> Result<()> {
        if points.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO body_battery_curve (date, points) VALUES (?1, ?2)",
            params![date, serde_json::to_string(points)?],
        )?;
        Ok(())
    }

    /// The Body Battery curve of one day, oldest first. Empty when the cache
    /// holds none: Garmin sends the curve for the recent days only.
    pub fn body_battery_curve(&self, date: &str) -> Result<Vec<BatteryPoint>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT points FROM body_battery_curve WHERE date = ?1",
                params![date],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw
            .and_then(|r| serde_json::from_str(&r).ok())
            .unwrap_or_default())
    }

    /// The overnight readings of one night, oldest first. Empty when the cache
    /// holds none.
    pub fn hrv_readings(&self, date: &str) -> Result<Vec<HrvReading>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT readings FROM hrv_readings WHERE date = ?1",
                params![date],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw
            .and_then(|r| serde_json::from_str(&r).ok())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_odd_reading_does_not_drop_the_others() {
        let v = json!({ "hrvReadings": [
            { "hrvValue": 41, "readingTimeLocal": "2026-03-10T23:35:00.0" },
            { "hrvValue": null, "readingTimeLocal": "2026-03-10T23:40:00.0" },
            { "readingTimeLocal": "2026-03-10T23:45:00.0" },
            { "hrvValue": 47, "readingTimeLocal": "2026-03-10T23:50:00.0" },
            "junk",
        ]});
        let got = parse_hrv_readings(&v);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].time, "2026-03-10T23:35");
        assert_eq!(got[1].value, 47.0);
        assert!(parse_hrv_readings(&json!(null)).is_empty());
    }

    fn day_with(array: Value) -> Value {
        json!({
            "startTimestampGMT": "2026-03-09T23:00:00.0",
            "bodyBatteryValueDescriptorsDTOList": [
                { "bodyBatteryValueDescriptorIndex": 0, "bodyBatteryValueDescriptorKey": "timestamp" },
                { "bodyBatteryValueDescriptorIndex": 1, "bodyBatteryValueDescriptorKey": "bodyBatteryLevel" },
            ],
            "bodyBatteryValuesArray": array,
        })
    }

    #[test]
    fn the_samples_become_minutes_after_midnight_in_the_order_the_descriptors_give() {
        // 23:00 GMT is the local midnight, and the level is in the second place.
        let start = 1_773_097_200_000_i64;
        let v = day_with(json!([
            [start, 30],
            [start + 180_000, null],
            [start + 360_000],
            [start + 3_600_000, 55.5],
            "junk",
        ]));
        assert_eq!(
            parse_body_battery(&v),
            vec![
                BatteryPoint {
                    minute: 0,
                    level: 30.0
                },
                BatteryPoint {
                    minute: 60,
                    level: 55.5
                },
            ]
        );
        assert!(parse_body_battery(&json!({ "bodyBatteryValuesArray": [[1, "M", 5]] })).is_empty());
        assert!(parse_body_battery(&json!(null)).is_empty());
    }

    #[test]
    fn the_curve_of_a_day_round_trips_and_an_empty_list_keeps_it() {
        let db = crate::db::scratch_db("battery-curve");
        let one = [BatteryPoint {
            minute: 5,
            level: 40.0,
        }];
        db.save_body_battery_curve("2026-03-10", &one).unwrap();
        db.save_body_battery_curve("2026-03-10", &[]).unwrap();
        assert_eq!(db.body_battery_curve("2026-03-10").unwrap(), one);
        assert!(db.body_battery_curve("2026-03-11").unwrap().is_empty());
    }

    #[test]
    fn the_readings_of_a_night_round_trip_and_an_empty_list_keeps_them() {
        let db = crate::db::scratch_db("hrv-readings");
        let one = [HrvReading {
            time: "2026-03-10T23:35".into(),
            value: 41.0,
        }];
        db.save_hrv_readings("2026-03-11", &one).unwrap();
        db.save_hrv_readings("2026-03-11", &[]).unwrap();
        assert_eq!(db.hrv_readings("2026-03-11").unwrap(), one);
        assert!(db.hrv_readings("2026-03-12").unwrap().is_empty());
    }
}
