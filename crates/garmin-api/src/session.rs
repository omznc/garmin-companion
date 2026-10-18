//! Garmin sign-in state, sign-out and sync.
//!
//! The keyring names and the data folder are the same as in the old Tauri app,
//! because `garmin_core::store` and `garmin_core::paths` own them. A user of
//! v0.9.1 keeps the login.
//!
//! Garmin does not accept a password from a native client (Cloudflare checks
//! the TLS handshake). The host app shows [`sign_in_url`] in a real browser
//! view and watches the navigation. [`ticket_in`] reads the single-use ticket
//! from that navigation, and [`redeem_ticket`] trades it for the tokens.

use crate::ApiError;
use garmin_core::sync::SyncProgress;
use garmin_core::{auth, store};
use serde::Serialize;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Mutex;

const SIGN_IN_URL: &str = "https://sso.garmin.com/portal/sso/en-US/sign-in";
/// The CAS client and service of Garmin's own web portal. The service URL must
/// be the same text in the sign-in page and in the ticket exchange.
const SSO_CLIENT_ID: &str = "GarminConnect";
const SERVICE_URL: &str = "https://connect.garmin.com/app";

/// Only one sync runs at a time. A second caller waits.
static SYNCING: Mutex<()> = Mutex::new(());

/// The sign-in state.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub connected: bool,
    /// A token file of the `garminconnect` library, when one exists and the
    /// user is not connected. The UI can offer to import it.
    pub importable_token_path: Option<String>,
}

/// One progress step of a sync, with the sentence that the UI shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEvent {
    pub phase: String,
    pub detail: String,
    pub done: u32,
    pub total: Option<u32>,
    pub line: String,
}

/// What a finished sync did, as a sentence and the raw counts.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub line: String,
    pub activities_seen: usize,
    pub activities_written: usize,
    pub days_written: usize,
    pub warnings: Vec<String>,
}

pub fn status() -> Result<Status, ApiError> {
    let connected = store::load_tokens()?.is_some();
    let importable = store::python_token_path()
        .filter(|p| p.exists())
        .map(|p| p.display().to_string());
    Ok(Status {
        connected,
        importable_token_path: if connected { None } else { importable },
    })
}

pub fn sign_out() -> Result<(), ApiError> {
    Ok(store::clear_tokens()?)
}

/// Adopt the tokens of a `garminconnect` install. `None` takes the default path.
pub fn import_tokens(path: Option<&str>) -> Result<(), ApiError> {
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => store::python_token_path()
            .ok_or_else(|| ApiError::Internal("could not locate a home directory".into()))?,
    };
    store::save_tokens(&store::import_python_tokens(&path)?)?;
    Ok(())
}

/// The Garmin sign-in page that the host shows in a browser view.
pub fn sign_in_url() -> String {
    let mut url = url::Url::parse(SIGN_IN_URL).expect("the sign-in URL is a constant");
    url.query_pairs_mut()
        .append_pair("clientId", SSO_CLIENT_ID)
        .append_pair("service", SERVICE_URL);
    url.into()
}

/// The CAS ticket in a redirect to Garmin, if `link` is one.
///
/// Only a garmin.com host counts. A wrong guess spends the one attempt.
pub fn ticket_in(link: &str) -> Option<String> {
    let url = url::Url::parse(link).ok()?;
    let host = url.host_str()?;
    if host != "garmin.com" && !host.ends_with(".garmin.com") {
        return None;
    }
    url.query_pairs()
        .find(|(key, value)| key == "ticket" && !value.is_empty())
        .map(|(_, value)| value.into_owned())
}

/// Trade a ticket for the token pair and store it.
pub async fn redeem_ticket_async(ticket: &str) -> Result<(), ApiError> {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let tokens = auth::exchange_service_ticket(&http, ticket, SERVICE_URL).await?;
    store::save_tokens(&tokens)?;
    Ok(())
}

/// [`redeem_ticket_async`] for a caller with no async runtime.
pub fn redeem_ticket(ticket: &str) -> Result<(), ApiError> {
    block_on(redeem_ticket_async(ticket))
}

fn block_on<T>(future: impl Future<Output = Result<T, ApiError>>) -> Result<T, ApiError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .block_on(future)
}

/// Sync from Garmin into the cache. It blocks until the sync ends.
///
/// `on` receives each step. Call it from a worker thread, never from a UI
/// thread. `full` walks the whole history.
pub fn sync(
    days: u32,
    full: bool,
    on: &(dyn Fn(SyncEvent) + Send + Sync),
) -> Result<SyncSummary, ApiError> {
    let client = garmin_core::client_from_keyring()?.ok_or(ApiError::NotConnected)?;
    // A poisoned lock only means that an earlier sync panicked.
    let _busy = SYNCING.lock().unwrap_or_else(|e| e.into_inner());
    let db = crate::open_default_db()?;
    let sink = |p: SyncProgress| on(event_of(&p));
    let report = block_on(async {
        garmin_core::sync::sync_all_with(&client, &db, days, full, &sink)
            .await
            .map_err(ApiError::from)
    })?;
    Ok(summary_of(&report))
}

fn event_of(p: &SyncProgress) -> SyncEvent {
    SyncEvent {
        phase: p.phase.to_string(),
        detail: p.detail.clone(),
        done: p.done,
        total: p.total,
        line: progress_line(p),
    }
}

/// The sentence for one progress step.
pub fn progress_line(p: &SyncProgress) -> String {
    let what = match p.phase {
        "activities" => "Reading your activities",
        "wellness" => "Reading your daily health data",
        "workouts" => "Reading your workouts",
        "tracks" => "Reading your routes",
        "done" => return "Done.".into(),
        _ => "Syncing",
    };
    match (p.detail.is_empty(), p.total) {
        (true, Some(total)) => format!("{what} ({} of {total})", p.done),
        (true, None) => format!("{what}."),
        (false, Some(total)) => format!("{what}: {} ({} of {total})", p.detail, p.done),
        (false, None) => format!("{what}: {}", p.detail),
    }
}

fn summary_of(r: &garmin_core::sync::SyncReport) -> SyncSummary {
    let mut line = format!(
        "{} seen, {} new or updated, {} days of health data.",
        r.activities_seen, r.activities_written, r.days_written
    );
    if let Some(first) = r.warnings.first() {
        let n = r.warnings.len();
        line.push_str(&format!(
            " {n} warning{}: {first}",
            if n == 1 { "" } else { "s" }
        ));
    }
    SyncSummary {
        line,
        activities_seen: r.activities_seen,
        activities_written: r.activities_written,
        days_written: r.days_written,
        warnings: r.warnings.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(phase: &'static str, detail: &str, done: u32, total: Option<u32>) -> SyncProgress {
        SyncProgress {
            phase,
            detail: detail.into(),
            done,
            total,
        }
    }

    #[test]
    fn the_ticket_comes_from_a_garmin_redirect_only() {
        assert_eq!(
            ticket_in("https://connect.garmin.com/app?ticket=ST-1-abc"),
            Some("ST-1-abc".into())
        );
        assert_eq!(ticket_in("https://connect.garmin.com/app?ticket="), None);
        assert_eq!(ticket_in(&sign_in_url()), None);
        assert_eq!(ticket_in("https://example.com/app?ticket=ST-1"), None);
        assert_eq!(ticket_in("https://notgarmin.com/app?ticket=ST-1"), None);
        assert_eq!(ticket_in("not a url"), None);
    }

    #[test]
    fn the_sign_in_url_names_the_client_and_the_service() {
        let url = sign_in_url();
        assert!(url.starts_with("https://sso.garmin.com/portal/sso/en-US/sign-in?"));
        assert!(url.contains("clientId=GarminConnect"));
        assert!(url.contains("service=https%3A%2F%2Fconnect.garmin.com%2Fapp"));
    }

    #[test]
    fn progress_lines_read_as_sentences() {
        assert_eq!(
            progress_line(&step("wellness", "2026-03-01", 3, Some(30))),
            "Reading your daily health data: 2026-03-01 (3 of 30)"
        );
        assert_eq!(
            progress_line(&step("activities", "", 0, None)),
            "Reading your activities."
        );
        assert_eq!(progress_line(&step("done", "", 1, Some(1))), "Done.");
    }

    #[test]
    fn the_summary_counts_warnings() {
        let report = garmin_core::sync::SyncReport {
            activities_seen: 5,
            activities_written: 2,
            days_written: 7,
            warnings: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        assert_eq!(
            summary_of(&report).line,
            "5 seen, 2 new or updated, 7 days of health data. 2 warnings: a"
        );
    }
}
