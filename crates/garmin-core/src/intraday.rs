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
"#;

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
