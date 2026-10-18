//! The data the Today screen reads, and the one place it comes from.
//!
//! [`load_today`] is the only function the rest of the app calls. The view
//! model below is the formatted form that the screen draws. It comes from one
//! of two sources: the synthetic sample fixture, or `garmin-api` run against a
//! copy of the cache (`--db <path>`).

use std::path::PathBuf;

use anyhow::{ensure, Context, Result};
use chrono::NaiveDate;
use serde::Deserialize;

/// The synthetic sample. It holds no real data.
const SAMPLE: &str = include_str!("../fixtures/today.sample.json");

/// Where the Today view comes from.
pub enum Source {
    Sample,
    /// A copy of the cache. Never point this at the live file: opening a
    /// database runs migrations.
    Db {
        path: PathBuf,
        /// The day to read. `None` is today. Only a demo on an old cache sets it.
        date: Option<NaiveDate>,
    },
}

/// A piece of a metric value. A unit draws smaller, as in "6h 24m".
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Part {
    pub text: String,
    #[serde(default)]
    pub unit: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Metric {
    pub label: String,
    pub value: Vec<Part>,
    /// Only a worse than usual figure takes the accent colour.
    #[serde(default)]
    pub accent: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Link {
    pub label: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Attention {
    pub text: String,
    #[serde(default)]
    pub accent: bool,
    #[serde(default)]
    pub spark: Vec<f32>,
    pub link: Option<Link>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Stat {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Session {
    pub name: String,
    pub detail: String,
    /// Percent of the time in each of Z1 to Z5.
    pub zones: [f32; 5],
    pub stats: Vec<Stat>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DriftRun {
    pub date: String,
    /// Percent of the run above Z2.
    pub above_z2: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Drift {
    /// What the bars count, such as "runs". It comes from the view model, so
    /// the screen has no sport words of its own.
    #[serde(default = "sessions")]
    pub noun: String,
    pub target: f32,
    pub summary: String,
    /// Oldest first.
    pub runs: Vec<DriftRun>,
}

fn sessions() -> String {
    "sessions".into()
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Week {
    pub km: Vec<f32>,
    /// One axis label for each day.
    pub axis: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Today {
    /// The athlete's name, for the top of the sidebar.
    pub who: Option<String>,
    pub synced_ago: Option<String>,
    pub eyebrow: String,
    pub title: String,
    pub lede: String,
    /// True when the cache holds nothing. The screen then says so.
    #[serde(default)]
    pub empty: bool,
    /// The tiles that have a figure. A user without a heart rate watch gets no tile.
    pub metrics: Vec<Metric>,
    pub fuel_note: Option<String>,
    pub weight_note: Option<String>,
    pub attention: Vec<Attention>,
    pub last_session: Option<Session>,
    pub zone_drift: Option<Drift>,
    pub week: Option<Week>,
}

/// The Today view. The single switch point between the fixture and the API.
pub fn load_today(source: &Source) -> Result<Today> {
    match source {
        Source::Sample => {
            serde_json::from_str(SAMPLE).context("the sample Today fixture does not parse")
        }
        Source::Db { path, date } => {
            // `open_db` creates a missing file, so a wrong path must stop here.
            ensure!(path.is_file(), "no cache copy at {}", path.display());
            let db = garmin_api::open_db(path).context("cannot open the cache copy")?;
            let today = date.unwrap_or_else(|| chrono::Local::now().date_naive());
            let view = garmin_api::today_view(&db, today).context("cannot build the Today view")?;
            Ok(crate::map::today(&view, None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_cache_copy_is_an_error_and_makes_no_file() {
        let path = std::env::temp_dir().join("gc-desktop-missing/none.sqlite3");
        let source = Source::Db {
            path: path.clone(),
            date: None,
        };
        assert!(load_today(&source).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn the_sample_fixture_parses_and_is_internally_consistent() {
        let today = load_today(&Source::Sample).unwrap();
        assert_eq!(today.metrics.len(), 6);
        let week = today.week.as_ref().unwrap();
        assert_eq!(week.km.len(), week.axis.len());
        let zones: f32 = today.last_session.unwrap().zones.iter().sum();
        assert!((zones - 100.0).abs() < 0.5, "zones sum to {zones}");
        let drift = today.zone_drift.unwrap();
        assert!(drift
            .runs
            .iter()
            .all(|r| (0.0..=100.0).contains(&r.above_z2)));
    }
}
