//! Shared Garmin Connect client, token storage, and local cache.
//!
//! Both the desktop app and the MCP server build on this crate so there is only
//! ever one implementation of the auth dance to keep working.

pub mod analysis;
pub mod auth;
pub mod client;
pub mod db;
pub mod query;
pub mod store;
pub mod sync;
pub mod theme;
pub mod workout;

pub use analysis::ActivityAnalysis;
pub use auth::Tokens;
pub use client::{ActivitySummary, GarminClient, Profile};
pub use db::{CachedActivity, DailyMetrics, Db, WeighIn};
pub use workout::WorkoutDraft;

use anyhow::{Context, Result};
use std::sync::Arc;

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
