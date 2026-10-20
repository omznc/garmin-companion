//! VO2 max by day, and the fitness age.
//!
//! VO2 max has its own history endpoint. It answers with an empty list for an
//! account that never ran outdoors with GPS, so an empty reply is normal.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;

pub(crate) const SCHEMA: &str = r#"
    CREATE TABLE IF NOT EXISTS vo2_days (
        date TEXT PRIMARY KEY,
        vo2  REAL NOT NULL
    );

    -- One row for each day that the sync read it. The newest row is the
    -- current fitness age.
    CREATE TABLE IF NOT EXISTS fitness_age (
        date           TEXT PRIMARY KEY,
        fitness_age    REAL,
        chronological_age REAL,
        achievable_age REAL
    );
"#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vo2Day {
    pub date: String,
    pub vo2: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitnessAge {
    pub date: String,
    pub fitness_age: Option<f64>,
    pub chronological_age: Option<f64>,
    pub achievable_age: Option<f64>,
}

/// The VO2 max days in a history reply. Each entry holds a `generic` object
/// (running) and a `cycling` object. The generic one is used, and the entry
/// itself when it has the figure at the top. An entry without a date or a
/// positive figure is skipped.
pub fn parse_vo2_days(v: &Value) -> Vec<Vo2Day> {
    let Some(list) = v.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|e| {
            let g = e.get("generic").filter(|g| g.is_object()).unwrap_or(e);
            let date = g.get("calendarDate")?.as_str()?.get(..10)?.to_owned();
            let vo2 = ["vo2MaxPreciseValue", "vo2MaxValue"]
                .iter()
                .find_map(|k| g.get(*k)?.as_f64())?;
            (vo2 > 0.0).then_some(Vo2Day { date, vo2 })
        })
        .collect()
}

/// The fitness age in a reply, dated `date`. `None` without a fitness age.
pub fn parse_fitness_age(date: &str, v: &Value) -> Option<FitnessAge> {
    let f = |k: &str| v.get(k).and_then(Value::as_f64);
    let age = FitnessAge {
        date: date.to_owned(),
        fitness_age: f("fitnessAge"),
        chronological_age: f("chronologicalAge"),
        achievable_age: f("achievableFitnessAge"),
    };
    age.fitness_age.is_some().then_some(age)
}

impl Db {
    pub fn save_vo2_days(&self, days: &[Vo2Day]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for d in days {
            tx.execute(
                "INSERT OR REPLACE INTO vo2_days (date, vo2) VALUES (?1, ?2)",
                params![d.date, d.vo2],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The VO2 max days from `from` up to the as-of date, oldest first.
    pub fn vo2_since(&self, from: &str) -> Result<Vec<Vo2Day>> {
        let mut stmt = self.conn.prepare(
            "SELECT date, vo2 FROM vo2_days WHERE date >= ?1 AND date <= ?2 ORDER BY date",
        )?;
        let rows = stmt
            .query_map(params![from, crate::until()], |r| {
                Ok(Vo2Day {
                    date: r.get(0)?,
                    vo2: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn save_fitness_age(&self, a: &FitnessAge) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO fitness_age (date, fitness_age, chronological_age, achievable_age)
             VALUES (?1,?2,?3,?4)",
            params![a.date, a.fitness_age, a.chronological_age, a.achievable_age],
        )?;
        Ok(())
    }

    /// The newest fitness age on or before the as-of date.
    pub fn latest_fitness_age(&self) -> Result<Option<FitnessAge>> {
        Ok(self
            .conn
            .query_row(
                "SELECT date, fitness_age, chronological_age, achievable_age
                 FROM fitness_age WHERE date <= ?1 ORDER BY date DESC LIMIT 1",
                params![crate::until()],
                |r| {
                    Ok(FitnessAge {
                        date: r.get(0)?,
                        fitness_age: r.get(1)?,
                        chronological_age: r.get(2)?,
                        achievable_age: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_history_reads_the_running_figure_and_skips_an_odd_entry() {
        let v = json!([
            { "generic": { "calendarDate": "2026-03-01", "vo2MaxPreciseValue": 51.4, "vo2MaxValue": 51 }, "cycling": null },
            { "generic": { "calendarDate": "2026-03-08", "vo2MaxValue": 52 } },
            { "calendarDate": "2026-03-09", "vo2MaxValue": 53 },
            { "generic": { "calendarDate": "2026-03-10" } },
            { "generic": { "vo2MaxValue": 50 } },
            { "generic": { "calendarDate": "2026-03-11", "vo2MaxValue": 0 } },
            "junk",
        ]);
        let got = parse_vo2_days(&v);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].vo2, 51.4);
        assert!(parse_vo2_days(&json!([])).is_empty());
        assert!(parse_vo2_days(&json!(null)).is_empty());
    }

    #[test]
    fn the_fitness_age_needs_a_figure_and_the_newest_wins() {
        assert!(parse_fitness_age("2026-03-10", &json!({ "chronologicalAge": 40 })).is_none());
        let db = crate::db::scratch_db("fitness-age");
        assert_eq!(db.latest_fitness_age().unwrap(), None);
        for (d, a) in [("2026-03-01", 38.0), ("2026-03-10", 37.5)] {
            let age = parse_fitness_age(
                d,
                &json!({ "fitnessAge": a, "chronologicalAge": 40, "achievableFitnessAge": 33.2 }),
            )
            .unwrap();
            db.save_fitness_age(&age).unwrap();
        }
        let latest = db.latest_fitness_age().unwrap().unwrap();
        assert_eq!(
            (latest.date.as_str(), latest.fitness_age),
            ("2026-03-10", Some(37.5))
        );
        assert_eq!(latest.achievable_age, Some(33.2));
    }

    #[test]
    fn the_vo2_days_are_kept_by_date_and_a_later_value_replaces() {
        let db = crate::db::scratch_db("vo2-days");
        let day = |d: &str, v| Vo2Day {
            date: d.into(),
            vo2: v,
        };
        db.save_vo2_days(&[day("2026-03-01", 50.0), day("2026-03-02", 51.0)])
            .unwrap();
        db.save_vo2_days(&[day("2026-03-02", 52.0)]).unwrap();
        let got = db.vo2_since("2026-03-02").unwrap();
        assert_eq!(got, vec![day("2026-03-02", 52.0)]);
        assert_eq!(db.vo2_since("2026-01-01").unwrap().len(), 2);
    }

    #[test]
    fn the_vo2_reads_stop_at_the_as_of_date() {
        let db = crate::db::scratch_db("vo2-as-of");
        let day = |d: &str, v| Vo2Day {
            date: d.into(),
            vo2: v,
        };
        db.save_vo2_days(&[day("2026-03-01", 50.0), day("2026-03-09", 51.0)])
            .unwrap();
        let age = |d: &str, a| parse_fitness_age(d, &json!({ "fitnessAge": a })).unwrap();
        db.save_fitness_age(&age("2026-03-01", 38.0)).unwrap();
        db.save_fitness_age(&age("2026-03-09", 37.0)).unwrap();
        let as_of = chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        crate::with_as_of(as_of, || {
            assert_eq!(db.vo2_since("2026-01-01").unwrap().len(), 1);
            let latest = db.latest_fitness_age().unwrap().unwrap();
            assert_eq!(latest.date, "2026-03-01");
        });
    }
}
