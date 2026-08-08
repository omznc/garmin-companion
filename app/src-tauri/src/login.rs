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
//!
//! # Two shapes of the same trick
//!
//! Desktop opens a second window and watches it. Android cannot: a Tauri mobile
//! app has exactly one window, and `WebviewWindowBuilder` has nothing to build
//! into. So there the *main* webview is sent to Garmin and brought back
//! afterwards, which is why `lib.rs` constructs it in `setup` on mobile rather
//! than letting the config create it — a navigation handler can only be
//! attached at build time, and by the time anyone presses Sign in it is far too
//! late to add one.
//!
//! The consequence is that the frontend is destroyed mid-command. Nothing
//! awaits `run` on that side, and the invoke promise never resolves because the
//! page that made it is gone. The result is picked up after the reload instead:
//! `garmin_status` already reports the connection, and [`take_last_error`]
//! carries the reason across for the case where there is one. It reads like a
//! detour, and it is — but a webview that navigates away takes the JS context
//! with it, and no amount of event plumbing survives that.

use anyhow::{anyhow, Context, Result};
use garmin_core::{auth, store};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Url};

// Both desktop-only: the progress narration goes to a setup screen that, on
// mobile, has been destroyed by the navigation to Garmin before there is
// anything to narrate.
#[cfg(desktop)]
use std::sync::Arc;
#[cfg(desktop)]
use tauri::Emitter;

#[cfg(desktop)]
use tauri::{WebviewUrl, WebviewWindowBuilder};

#[cfg(desktop)]
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

/// The CAS sign-in page for this client and service.
fn sign_in_url() -> Result<Url> {
    Url::parse_with_params(
        SIGN_IN_URL,
        &[("clientId", SSO_CLIENT_ID), ("service", SERVICE_URL)],
    )
    .context("bad sign-in URL")
}

/// Trade a captured ticket for a token pair and store it.
///
/// Shared by both platforms: the part that differs is only how a ticket is got
/// hold of, and this is everything after that.
async fn redeem(ticket: &str) -> Result<()> {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .context("failed to build HTTP client")?;
    let tokens = auth::exchange_service_ticket(&http, ticket, SERVICE_URL).await?;
    store::save_tokens(&tokens).context("could not save the Garmin tokens")?;
    Ok(())
}

/// Opens the sign-in window, waits for a ticket, and writes the tokens it buys
/// to the keyring.
///
/// Progress is emitted on `garmin-login` so the setup screen can narrate what
/// is happening rather than showing an unexplained pause.
#[cfg(desktop)]
pub async fn run(app: AppHandle) -> Result<()> {
    if let Some(existing) = app.get_webview_window(WINDOW_LABEL) {
        // A second click while the window is open should surface it, not open
        // a second identical window that races the first.
        let _ = existing.set_focus();
        return Err(anyhow!("A Garmin sign-in window is already open."));
    }

    let sign_in = sign_in_url()?;

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
    redeem(&ticket).await?;
    let _ = app.emit("garmin-login", "Signed in.");
    Ok(())
}

/* -------------------------------------------------------------- mobile --- */

/// Where the ticket lands when the main webview's navigation handler spots one.
///
/// A static rather than something threaded through `AppState`, because the
/// handler is installed while the app is still being built — there is no state
/// to borrow yet, and the closure has to outlive the call that made it.
#[cfg(mobile)]
static TICKET: Mutex<Option<String>> = Mutex::new(None);

/// Why the last sign-in attempt failed, kept for the frontend that wasn't
/// running to be told. Cleared by the read — see [`take_last_error`].
#[cfg(mobile)]
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// Called by the main webview's `on_navigation` on every navigation.
///
/// `false` stops the load. That matters for the redirect carrying the ticket:
/// it is single-use, and letting Connect have it spends it before the exchange
/// can. Everything else is waved through, or sign-in wouldn't work at all.
#[cfg(mobile)]
pub fn intercept(url: &Url) -> bool {
    let Some(found) = ticket_in(url) else {
        return true;
    };
    if let Ok(mut slot) = TICKET.lock() {
        slot.get_or_insert(found);
    }
    false
}

/// The reason the last mobile sign-in failed, if it did, clearing it as it goes.
///
/// Taken rather than read so a failure is reported once. Leaving it in place
/// would mean a stale message appearing on top of a later successful attempt.
#[cfg(mobile)]
pub fn take_last_error() -> Option<String> {
    LAST_ERROR.lock().ok().and_then(|mut slot| slot.take())
}

/// Sends the one webview there is to Garmin, waits for the ticket, and brings
/// it back.
///
/// Returns `Ok(())` on success, but the caller almost certainly isn't there to
/// receive it — see the note at the top of this module.
#[cfg(mobile)]
pub async fn run(app: AppHandle) -> Result<()> {
    let webview = app
        .get_webview_window("main")
        .ok_or_else(|| anyhow!("no main window to sign in with"))?;

    // Captured before leaving so there is something to come back to. It is the
    // app's own asset URL, whose scheme and host differ between platforms and
    // between dev and release — asking the webview beats hardcoding four cases.
    let home = webview.url().context("could not read the current page")?;

    if let Ok(mut slot) = TICKET.lock() {
        // A previous abandoned attempt could otherwise be redeemed as though it
        // were this one, and a stale ticket fails the exchange.
        *slot = None;
    }

    webview
        .navigate(sign_in_url()?)
        .context("could not open the Garmin sign-in page")?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT_SECS);
    let outcome = loop {
        if let Some(found) = TICKET.lock().ok().and_then(|slot| slot.clone()) {
            break Ok(found);
        }
        if std::time::Instant::now() > deadline {
            break Err(anyhow!("Timed out waiting for sign-in."));
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
    };

    // Whatever happened, the app has to come back — an Android user left on
    // Garmin's website with no way home would have to force-quit.
    let result = match outcome {
        Ok(ticket) => redeem(&ticket).await,
        Err(e) => Err(e),
    };
    let _ = webview.navigate(home);

    if let Err(e) = &result {
        if let Ok(mut slot) = LAST_ERROR.lock() {
            *slot = Some(
                e.chain()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(": "),
            );
        }
    }
    result
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
