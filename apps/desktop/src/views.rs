//! The garmin-api views for the charts, the tiles and the extras. Every
//! block can read them through `blocks::Ctx`.

use std::path::Path;

use chrono::NaiveDate;
use garmin_api::charts::ChartsView;
use garmin_api::extras::ExtrasView;
use garmin_api::tiles::TilesView;

/// A view is `None` until its read ends, and when the read fails.
#[derive(Default)]
pub struct Views {
    pub charts: Option<ChartsView>,
    pub tiles: Option<TilesView>,
    pub extras: Option<ExtrasView>,
}

fn keep<T, E: std::fmt::Display>(name: &str, read: Result<T, E>) -> Option<T> {
    read.map_err(|e| eprintln!("could not read the {name} view: {e}"))
        .ok()
}

impl Views {
    /// Read the three views from the cache at `path`, for the day `today`.
    /// This opens the database, so call it on the background executor.
    pub fn load(path: &Path, today: NaiveDate) -> Self {
        let Some(db) = keep("cache", garmin_api::open_db(path)) else {
            return Self::default();
        };
        // The as-of date hides data after `today`, on this thread.
        garmin_core::with_as_of(today, || Self {
            charts: keep("charts", garmin_api::charts::charts_view(&db, today)),
            tiles: keep("tiles", garmin_api::tiles::tiles_view(&db, today)),
            extras: keep("extras", garmin_api::extras::extras_view(&db, today)),
        })
    }
}
