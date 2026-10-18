//! The data the Today screen reads, and the one place it comes from.
//!
//! [`load_today`] is the only function the rest of the app calls. The view
//! model below is a stand-in for the typed Today view of the `garmin-api`
//! crate. It reads the synthetic sample fixture until that crate is on
//! `rewrite`. Then the body of [`load_today`] calls `garmin-api` against a copy
//! of the cache, and nothing else changes.

use anyhow::{Context, Result};
use serde::Deserialize;

/// The synthetic sample. It holds no real data.
const SAMPLE: &str = include_str!("../fixtures/today.sample.json");

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
    pub target: f32,
    pub summary: String,
    /// Oldest first.
    pub runs: Vec<DriftRun>,
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
    pub metrics: Vec<Metric>,
    pub fuel_note: String,
    pub weight_note: Option<String>,
    pub attention: Vec<Attention>,
    pub last_session: Option<Session>,
    pub zone_drift: Option<Drift>,
    pub week: Week,
}

/// The Today view. The single switch point between the fixture and the API.
pub fn load_today() -> Result<Today> {
    serde_json::from_str(SAMPLE).context("the sample Today fixture does not parse")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_fixture_parses_and_is_internally_consistent() {
        let today = load_today().unwrap();
        assert_eq!(today.metrics.len(), 6);
        assert_eq!(today.week.km.len(), today.week.axis.len());
        let zones: f32 = today.last_session.unwrap().zones.iter().sum();
        assert!((zones - 100.0).abs() < 0.5, "zones sum to {zones}");
        let drift = today.zone_drift.unwrap();
        assert!(drift
            .runs
            .iter()
            .all(|r| (0.0..=100.0).contains(&r.above_z2)));
    }
}
