//! Save a drafted workout to the Garmin account of the person.

use crate::ApiError;
use garmin_core::workout::WorkoutDraft;

/// Create the workout on Garmin and return its id. Then refresh the cached
/// workouts. That refresh is best effort: the workout exists on Garmin even
/// when the refresh fails.
///
/// This calls the network and blocks. Call it from a worker thread.
pub fn create_workout(draft: &WorkoutDraft) -> Result<i64, ApiError> {
    draft
        .validate()
        .map_err(|e| ApiError::BadArguments(e.to_string()))?;
    let client = garmin_core::client_from_keyring()?.ok_or(ApiError::NotConnected)?;
    crate::session::block_on(async {
        let id = client.create_workout(draft).await?;
        if let Ok(db) = garmin_core::db::Db::open_default() {
            let mut report = garmin_core::sync::SyncReport::default();
            let _ = garmin_core::sync::sync_workouts(&client, &db, &mut report, &|_| {}).await;
        }
        Ok(id)
    })
}
