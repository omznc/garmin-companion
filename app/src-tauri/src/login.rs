//! First-run Garmin sign-in through a real browser window.
//!
//! Garmin's SSO endpoints sit behind Cloudflare, which fingerprints the TLS
//! handshake and rejects non-browser clients — the reason this app cannot post
//! a username and password itself, however correct the request is. Token
//! *refresh* and every data endpoint are reachable from a plain native stack;
//! only the initial sign-in is gated.
//!
//! Tauri's webview is a real browser, so it clears that gate for free. The user
//! signs in on Garmin's own page — this app never sees the password — and CAS
//! finishes by redirecting to `service?ticket=…`. That ticket is the whole
//! prize: [`auth::exchange_service_ticket`] trades it for the DI token pair,
//! which goes straight into the OS keyring.
//!
//! The ticket is read off the *navigation*, not out of the page. Garmin Connect
//! keeps its own web session in cookies, so there is no token pair sitting in
//! the page's storage to lift — an earlier version scraped `localStorage` and
//! waited forever for something that was never going to appear.

use anyhow::{anyhow, Context, Result};
use garmin_core::{auth, store};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindowBuilder};

const WINDOW_LABEL: &str = "garmin-login";
const SIGN_IN_URL: &str = "https://sso.garmin.com/portal/sso/en-US/sign-in";

/// The CAS client and service to sign in as. This pairing is the one Garmin's
/// own web portal uses, so it is certain to be whitelisted; `SERVICE_URL` has
/// to be repeated verbatim in the exchange or the ticket won't validate.
const SSO_CLIENT_ID: &str = "GarminConnect";
const SERVICE_URL: &str = "https://connect.garmin.com/app";

const POLL_INTERVAL_MS: u64 = 400;
/// Long enough for a slow sign-in with two-factor; short enough that a window
/// left open doesn't poll forever.
const TIMEOUT_SECS: u64 = 600;

/// Opens the sign-in window, waits for a ticket, and writes the tokens it buys
/// to the keyring.
///
/// Progress is emitted on `garmin-login` so the setup screen can narrate what
/// is happening rather than showing an unexplained pause.
pub async fn run(app: AppHandle) -> Result<()> {
    if let Some(existing) = app.get_webview_window(WINDOW_LABEL) {
        // A second click while the window is open should surface it, not open
        // a second identical window that races the first.
        let _ = existing.set_focus();
        return Err(anyhow!("A Garmin sign-in window is already open."));
    }

    let sign_in = Url::parse_with_params(
        SIGN_IN_URL,
        &[("clientId", SSO_CLIENT_ID), ("service", SERVICE_URL)],
    )
    .context("bad sign-in URL")?;

    let ticket: Arc<Mutex<Option<String>>> = Arc::default();
    let captured = ticket.clone();

    let window = WebviewWindowBuilder::new(&app, WINDOW_LABEL, WebviewUrl::External(sign_in))
        .title("Sign in to Garmin")
        .inner_size(520.0, 760.0)
        .on_navigation(move |url| {
            let Some(found) = ticket_in(url) else {
                return true;
            };
            if let Ok(mut slot) = captured.lock() {
                slot.get_or_insert(found);
            }
            // Stop the redirect here. The ticket is single-use, and letting
            // Connect load it would spend it before the exchange can.
            false
        })
        .build()
        .context("could not open the sign-in window")?;

    let _ = app.emit("garmin-login", "Waiting for you to sign in to Garmin…");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT_SECS);
    let result = loop {
        if let Some(found) = ticket.lock().ok().and_then(|slot| slot.clone()) {
            break Ok(found);
        }
        if std::time::Instant::now() > deadline {
            break Err(anyhow!("Timed out waiting for sign-in."));
        }
        // The user closing the window is a cancellation, not a failure to
        // report loudly — but the caller still needs to stop waiting.
        if app.get_webview_window(WINDOW_LABEL).is_none() {
            break Err(anyhow!("Sign-in window was closed."));
        }
        // Backstop for platforms that route server-issued redirects past the
        // navigation handler: the ticket is still sitting in the address bar.
        if let Some(found) = window.url().ok().as_ref().and_then(ticket_in) {
            break Ok(found);
        }

        tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
    };

    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        let _ = w.close();
    }

    let ticket = result?;
    let _ = app.emit("garmin-login", "Signing you in…");

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .context("failed to build HTTP client")?;
    let tokens = auth::exchange_service_ticket(&http, &ticket, SERVICE_URL).await?;

    store::save_tokens(&tokens).context("could not write tokens to the keyring")?;
    let _ = app.emit("garmin-login", "Signed in.");
    Ok(())
}

/// The CAS ticket carried by a redirect back to our service, if this is one.
///
/// Scoped to garmin.com so an unrelated `ticket` query parameter picked up
/// somewhere else in the flow can't be mistaken for the real thing — there is
/// only one attempt, and a wrong guess burns it.
fn ticket_in(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    if host != "garmin.com" && !host.ends_with(".garmin.com") {
        return None;
    }
    url.query_pairs()
        .find(|(key, value)| key == "ticket" && !value.is_empty())
        .map(|(_, value)| value.into_owned())
}

#[cfg(test)]
mod tests {
    use super::ticket_in;
    use tauri::Url;

    fn ticket(raw: &str) -> Option<String> {
        ticket_in(&Url::parse(raw).unwrap())
    }

    #[test]
    fn reads_the_ticket_off_the_redirect() {
        assert_eq!(
            ticket("https://connect.garmin.com/app?ticket=ST-123-abc-cas"),
            Some("ST-123-abc-cas".into())
        );
    }

    #[test]
    fn ignores_pages_without_one() {
        assert_eq!(
            ticket("https://sso.garmin.com/portal/sso/en-US/sign-in"),
            None
        );
        assert_eq!(ticket("https://connect.garmin.com/app?ticket="), None);
    }

    #[test]
    fn ignores_tickets_from_anywhere_but_garmin() {
        assert_eq!(ticket("https://example.com/app?ticket=ST-123"), None);
        // A suffix match on the bare string would let this through.
        assert_eq!(ticket("https://notgarmin.com/app?ticket=ST-123"), None);
    }
}
