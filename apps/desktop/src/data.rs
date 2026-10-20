//! The data the Today screen reads, and the one place it comes from.
//!
//! [`load_today`] is the only function the rest of the app calls. The view
//! model below is the formatted form that the screen draws. It comes from one
//! of two sources: a made-up `garmin-api` persona (`sample` by default), or
//! `garmin-api` run against a copy of the cache (`--db <path>`).

use std::path::PathBuf;

use anyhow::{ensure, Context, Result};
use chrono::NaiveDate;

/// The made-up users of `garmin-api`, in the form that the API returns.
pub const PERSONAS: [(&str, &str); 5] = [
    (
        "sample",
        include_str!("../../../crates/garmin-api/fixtures/today.sample.json"),
    ),
    (
        "golfer",
        include_str!("../../../crates/garmin-api/fixtures/today.golfer.json"),
    ),
    (
        "gym",
        include_str!("../../../crates/garmin-api/fixtures/today.gym.json"),
    ),
    (
        "health_only",
        include_str!("../../../crates/garmin-api/fixtures/today.health_only.json"),
    ),
    (
        "new_user",
        include_str!("../../../crates/garmin-api/fixtures/today.new_user.json"),
    ),
];

/// Where the Today view comes from.
#[derive(Clone)]
pub enum Source {
    /// The cache in the data folder. This is the shipped behaviour.
    Default,
    /// One of the `garmin-api` personas, by name, such as `golfer`. The
    /// default is `sample`. The persona files hold made-up data only.
    Persona(String),
    /// A copy of the cache. Never point this at the live file: opening a
    /// database runs migrations.
    Db {
        path: PathBuf,
        /// The day to read. `None` is today. Only a demo on an old cache sets it.
        date: Option<NaiveDate>,
    },
}

impl Source {
    /// The same source, read for `day`. The real today keeps the shipped
    /// source. A persona is fixed and keeps its day.
    pub fn on_day(&self, day: NaiveDate) -> Source {
        match self {
            Source::Default if day == chrono::Local::now().date_naive() => Source::Default,
            Source::Default => match garmin_core::db::default_path() {
                Some(path) => Source::Db {
                    path,
                    date: Some(day),
                },
                None => Source::Default,
            },
            Source::Db { path, .. } => Source::Db {
                path: path.clone(),
                date: Some(day),
            },
            Source::Persona(name) => Source::Persona(name.clone()),
        }
    }
}

/// A piece of a metric value. A unit draws smaller, as in "6h 24m".
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub text: String,
    pub unit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub label: String,
    pub value: Vec<Part>,
    /// Only a worse than usual figure takes the accent colour.
    pub accent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attention {
    pub text: String,
    pub accent: bool,
    /// A missing day is `None`, and the chart leaves a gap there.
    pub spark: Vec<Option<f32>>,
    pub link: Option<Link>,
    /// The id of the Insights item that tells more about this note.
    pub insight: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stat {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: i64,
    pub name: String,
    /// The sport, such as "Easy run".
    pub sport: String,
    /// When it started, such as "Mon 19 Oct, 07:12".
    pub when: Option<String>,
    /// Seconds in each of Z1 to Z5, when the zone verdict suits.
    pub zone_secs: Option<[f32; 5]>,
    pub max_hr: Option<f32>,
    pub detail: String,
    /// Percent of the time in each of Z1 to Z5. `None` for a session that a
    /// zone verdict does not suit, or one with no heart rate data.
    pub zones: Option<[f32; 5]>,
    /// Said in place of the zone bar when the verdict suits but no data exists.
    pub zone_note: Option<String>,
    /// Only the figures that exist. A session with no distance has no distance tile.
    pub stats: Vec<Stat>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DriftRun {
    pub date: String,
    /// Percent of the run above Z2.
    pub above_z2: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Drift {
    /// The mean share above Z2 of the newest runs.
    pub recent_avg: Option<f32>,
    /// What the bars count, such as "runs". It comes from `ZoneDrift.noun` in
    /// the API, so the screen has no sport words of its own.
    pub noun: String,
    pub target: f32,
    pub summary: String,
    /// Oldest first.
    pub runs: Vec<DriftRun>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Week {
    /// Active minutes for each day. Every sport has minutes.
    pub minutes: Vec<f32>,
    /// One axis label for each day.
    pub axis: Vec<String>,
    pub summary: String,
    pub sessions: usize,
    pub total_minutes: f32,
    /// The distance of the week, such as "27.7 km". Only a user with distance has it.
    pub distance: Option<String>,
}

/// The fuel balance, with the sentence under it.
#[derive(Debug, Clone, PartialEq)]
pub struct Fuel {
    /// The balance as a tile. `None` when no day has a balance.
    pub tile: Option<Metric>,
    pub note: String,
    /// The day of the log: "today", "yesterday" or "3 days ago".
    pub when: String,
    pub eaten: Option<f32>,
    pub burned: Option<f32>,
}

/// One of the vital signs of this morning.
#[derive(Debug, Clone, PartialEq)]
pub struct Vital {
    /// The key of its series in the Health view: `resting_hr`, `hrv`,
    /// `body_battery`, `stress`, or `sleep`.
    pub key: &'static str,
    pub label: String,
    pub value: String,
    pub unit: Option<String>,
    /// A line under the figure, such as "+5 vs 30 days".
    pub sub: Option<String>,
    /// Only a worse than usual figure takes the accent colour.
    pub accent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Today {
    /// The athlete's name, for the top of the sidebar.
    pub who: Option<String>,
    pub synced_ago: Option<String>,
    pub eyebrow: String,
    pub title: String,
    pub lede: String,
    /// True when the cache holds nothing. The screen then says so.
    pub empty: bool,
    /// The section keys in drawing order. A key that the screen does not know,
    /// or one with no data, is skipped.
    pub sections: Vec<String>,
    /// The health tiles that have a figure. A user without the sensor gets no tile.
    pub metrics: Vec<Metric>,
    /// The vital signs that have a figure, in the order of the Today block.
    pub vitals: Vec<Vital>,
    pub fuel: Option<Fuel>,
    pub weight_note: Option<String>,
    pub attention: Vec<Attention>,
    pub last_session: Option<Session>,
    pub zone_drift: Option<Drift>,
    pub week: Option<Week>,
}

/// The Today view. The single switch point between the personas and the cache.
pub fn load_today(source: &Source) -> Result<Today> {
    match source {
        Source::Default => {
            let db = garmin_api::open_default_db().context("cannot open the cache")?;
            let view = garmin_api::today_view(&db, chrono::Local::now().date_naive())
                .context("cannot build the Today view")?;
            Ok(crate::map::today(&view, None))
        }
        Source::Persona(name) => {
            let (_, json) = PERSONAS
                .iter()
                .find(|(n, _)| n == name)
                .with_context(|| format!("no persona {name}"))?;
            let view: garmin_api::TodayView =
                serde_json::from_str(json).context("the persona file does not parse")?;
            Ok(crate::map::today(&view, None))
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
    fn the_default_persona_parses_and_is_internally_consistent() {
        let today = load_today(&Source::Persona("sample".into())).unwrap();
        assert!(today.metrics.len() >= 5);
        assert!(today.fuel.as_ref().unwrap().tile.is_some());
        let week = today.week.as_ref().unwrap();
        assert_eq!(week.minutes.len(), week.axis.len());
        let zones: f32 = today.last_session.unwrap().zones.unwrap().iter().sum();
        assert!((zones - 100.0).abs() < 0.5, "zones sum to {zones}");
        let drift = today.zone_drift.unwrap();
        assert_eq!(drift.noun, "runs");
        assert!(drift
            .runs
            .iter()
            .all(|r| (0.0..=100.0).contains(&r.above_z2)));
    }
}
