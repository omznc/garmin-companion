//! Save a drafted workout to the Garmin account of the person.

use crate::ApiError;
use garmin_core::workout::WorkoutDraft;

/// Create the workout on Garmin and return its id. Do not refresh the cache
/// here: the caller must store the id first. Then call [`refresh_workouts`].
///
/// A dead refresh token returns `NotConnected`, so the app asks for a sign-in.
///
/// This calls the network and blocks. Call it from a worker thread.
pub fn create_workout(draft: &WorkoutDraft) -> Result<i64, ApiError> {
    draft
        .validate()
        .map_err(|e| ApiError::BadArguments(e.to_string()))?;
    let client = garmin_core::client_from_keyring()?.ok_or(ApiError::NotConnected)?;
    crate::session::block_on(async {
        client
            .create_workout(draft)
            .await
            .map_err(|e| match ApiError::from(e) {
                ApiError::Internal(m) if refresh_token_is_dead(&m) => ApiError::NotConnected,
                other => other,
            })
    })
}

/// Refresh the cached workouts. This is best effort: the workout exists on
/// Garmin even when the refresh fails.
///
/// This calls the network and blocks. Call it from a worker thread.
pub fn refresh_workouts() {
    let Ok(Some(client)) = garmin_core::client_from_keyring() else {
        return;
    };
    let _ = crate::session::block_on(async {
        if let Ok(db) = garmin_core::db::Db::open_default() {
            let mut report = garmin_core::sync::SyncReport::default();
            let _ = garmin_core::sync::sync_workouts(&client, &db, &mut report, &|_| {}).await;
        }
        Ok(())
    });
}

/// `auth::refresh` answers 400 or 401 when the refresh token is dead.
// shortcut: this matches the error text, because the client has no typed auth error.
fn refresh_token_is_dead(message: &str) -> bool {
    message.contains("DI token refresh failed: 400")
        || message.contains("DI token refresh failed: 401")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_dead_refresh_token_asks_for_a_sign_in() {
        assert!(refresh_token_is_dead(
            "DI token refresh failed: 401 Unauthorized x"
        ));
        assert!(refresh_token_is_dead(
            "a: DI token refresh failed: 400 Bad Request"
        ));
        assert!(!refresh_token_is_dead("DI token refresh failed: 503 x"));
        assert!(!refresh_token_is_dead("request failed"));
    }
}
