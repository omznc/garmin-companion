//! The timed sync. The app syncs the last 30 days at launch when the last good
//! sync is old, and again each time that it becomes old while the app runs.
//!
//! A demo run, a signed-out app and a running sync never start one. A sync that
//! failed waits 30 minutes. The core and the session lock stop two syncs at once.

use std::time::Duration;

use chrono::{DateTime, Utc};
use garmin_api::settings;
use gpui::Context;

use crate::shell::Root;

/// A good sync older than this is stale.
const STALE_AFTER: chrono::Duration = chrono::Duration::hours(3);
/// The wait after a failed sync.
const BACKOFF: chrono::Duration = chrono::Duration::minutes(30);
/// How often the app looks at the age. A short step also catches a wake from sleep.
const TICK: Duration = Duration::from_secs(5 * 60);
/// The wait before the first look, so the window opens first.
const START_DELAY: Duration = Duration::from_secs(5);

/// True when a sync should start now. A cache that never synced is stale.
pub fn due(
    last_good: Option<DateTime<Utc>>,
    last_failed: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    if last_failed.is_some_and(|f| now - f < BACKOFF) {
        return false;
    }
    last_good.is_none_or(|g| now - g >= STALE_AFTER)
}

impl Root {
    /// Look at the age of the last good sync at launch and then every few minutes.
    pub fn watch_for_sync(&mut self, cx: &mut Context<Self>) {
        if self.account.demo {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(START_DELAY).await;
            loop {
                let last_good = cx
                    .background_executor()
                    .spawn(async { settings::last_sync() })
                    .await;
                let alive = this.update(cx, |root, cx| {
                    let failed = root.account.sync_failed_at;
                    if root.account.connected && due(last_good, failed, Utc::now()) {
                        root.start_sync(last_good.is_none(), cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
                cx.background_executor().timer(TICK).await;
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::<Utc>::UNIX_EPOCH + chrono::Duration::minutes(minutes)
    }

    #[test]
    fn a_sync_older_than_three_hours_is_due() {
        let now = at(1000);
        assert!(!due(Some(at(1000 - 179)), None, now));
        assert!(due(Some(at(1000 - 180)), None, now));
        assert!(due(None, None, now));
    }

    #[test]
    fn a_failure_holds_the_next_try_for_half_an_hour() {
        let now = at(1000);
        let old = Some(at(0));
        assert!(!due(old, Some(at(1000 - 29)), now));
        assert!(due(old, Some(at(1000 - 30)), now));
        // A fresh cache stays fresh, failed or not.
        assert!(!due(Some(at(999)), Some(at(0)), now));
    }
}
