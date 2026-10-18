//! Command logic for the new desktop and mobile UIs.
//!
//! This crate sits on top of `garmin_core::Db`. It holds the typed read paths
//! and the view models that the old TypeScript `derive.ts` computed. A UI calls
//! the functions directly (desktop) or sends JSON to [`call`] (mobile bridge).
//!
//! Only read paths exist for now: the Today view model and the cache reads. Nothing here talks to
//! Garmin over the network.

pub mod activities;
pub mod body;
mod derive;
mod dispatch;
mod error;
pub mod health;
pub mod personas;
mod profile;
pub mod session;
pub mod settings;
pub mod sleep_view;
mod text;
pub mod today;

pub use activities::{activities_view, activity_detail_view, ActivitiesView, ActivityDetailView};
pub use derive::{Fuel, Hydration, HydrationLatest, Latest, Link, Observation};
pub use dispatch::call;
pub use error::ApiError;
pub use health::{health_view, HealthView};
pub use profile::{activity_profile, ActivityProfile, FamilyStat};
pub use sleep_view::{sleep_view, SleepView};
pub use today::{today_view, TodayView};

use garmin_core::Db;
use std::path::{Path, PathBuf};

/// Set the folder that holds the cache. Call it once, before [`call`].
///
/// The mobile host passes its app-private folder here. A desktop host can skip
/// it and get the default data folder.
pub fn init(base_dir: impl Into<PathBuf>) {
    garmin_core::paths::set_base_dir(base_dir);
}

/// Open the cache at an exact path.
///
/// Migrations run on open. Do not point this at a live file that another
/// version of the app also uses.
pub fn open_db(path: &Path) -> Result<Db, ApiError> {
    Ok(Db::open(path)?)
}

/// Open the cache in the folder that [`init`] set, or the default folder.
pub fn open_default_db() -> Result<Db, ApiError> {
    Ok(Db::open_default()?)
}
