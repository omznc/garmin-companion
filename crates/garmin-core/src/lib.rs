//! Shared Garmin Connect client, token storage, and local cache.
//!
//! Both the desktop app and the MCP server build on this crate so there is only
//! ever one implementation of the auth dance to keep working.

pub mod analysis;
pub mod auth;
pub mod client;
pub mod coach;
pub mod db;
pub mod findings;
pub mod goals;
pub mod paths;
pub mod query;
pub mod records;
pub mod secrets;
pub mod signal;
pub mod sleep;
pub mod stats;
pub mod store;
pub mod strength;
pub mod sync;
pub mod theme;
pub mod workout;

pub use analysis::ActivityAnalysis;
pub use auth::Tokens;
pub use client::{ActivitySummary, GarminClient, Profile};
pub use coach::{Nudge, NudgeKind};
pub use db::{CachedActivity, DailyMetrics, Db, WeighIn};
pub use goals::{Goals, WeekProgress};
pub use records::{PersonalRecord, RacePredictions, TrainingStatus};
pub use strength::{ExerciseSet, StrengthSession};
pub use workout::WorkoutDraft;

use anyhow::{Context, Result};
use std::sync::Arc;

/// Today, in the machine's own timezone.
///
/// One place for it so that "today" means the same thing on every screen. The
/// athlete's data is stamped in local dates by Garmin, so a UTC "today" is the
/// wrong day for anyone west of London for part of every evening.
pub fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

/// The `YYYY-MM-DD` cutoff for a window of `days` ending today, inclusive of
/// both ends — `days_ago(1)` is today, `days_ago(7)` is a week counting today.
///
/// Every query that takes a day count goes through this rather than reaching
/// for a row limit, which is the bug this exists to make hard to write again.
pub fn days_ago(days: u32) -> String {
    let span = chrono::Duration::days(days.saturating_sub(1) as i64);
    (today() - span).format("%Y-%m-%d").to_string()
}

/// Whole days between two `YYYY-MM-DD` dates, or `None` if `date` won't parse.
/// Positive means `date` is in the past.
pub fn days_between(date: &str, from: chrono::NaiveDate) -> Option<i64> {
    // Activity dates arrive as either a plain date or a local timestamp, and
    // both start with the ten characters that matter.
    let parsed = chrono::NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d").ok()?;
    Some((from - parsed).num_days())
}

/// Build a client from tokens in the OS keyring, persisting rotated refresh
/// tokens back as they change.
///
/// Returns `Ok(None)` when nothing is stored yet — "not connected" is a normal
/// state, not an error.
pub fn client_from_keyring() -> Result<Option<GarminClient>> {
    let Some(tokens) = store::load_tokens()? else {
        return Ok(None);
    };
    let on_change: Arc<dyn Fn(&Tokens) + Send + Sync> = Arc::new(|t: &Tokens| {
        if let Err(e) = store::save_tokens(t) {
            eprintln!("warning: could not persist refreshed Garmin tokens: {e:#}");
        }
    });
    Ok(Some(GarminClient::new(tokens, on_change)?))
}

/// Same, but treating "not connected" as an error — for entry points that
/// can't do anything useful without a session.
pub fn require_client() -> Result<GarminClient> {
    client_from_keyring()?.context(
        "No Garmin session stored. Connect an account in the desktop app, or \
         import an existing ~/.garminconnect token file.",
    )
}
