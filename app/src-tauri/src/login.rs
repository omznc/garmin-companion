//! First-run Garmin sign-in through a real browser window.
//!
//! Garmin's SSO endpoints sit behind Cloudflare, which fingerprints the TLS
//! handshake and rejects non-browser clients — the reason this app cannot post
//! a username and password itself, however correct the request is. Token
//! *refresh* and every data endpoint are reachable from a plain native stack;
//! only the initial sign-in is gated.
//!
//! Tauri's webview is a real browser, so it clears that gate for free. The user
//! signs in to Garmin's own page — this app never sees the password — and once
//! Garmin Connect has issued a token, it is lifted out of the page's own
//! storage and moved into the OS keyring.

use anyhow::{anyhow, Context, Result};
use garmin_core::{auth::Tokens, store};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const WINDOW_LABEL: &str = "garmin-login";
const SIGN_IN_URL: &str = "https://connect.garmin.com/signin";
const POLL_INTERVAL_MS: u64 = 1200;
/// Long enough for a slow sign-in with two-factor; short enough that a window
/// left open doesn't poll forever.
const TIMEOUT_SECS: u64 = 600;

/// Reads the page's own token storage and hands back anything shaped like an
/// OAuth2 token pair.
///
/// Garmin has changed the storage key more than once, so this scans every entry
/// rather than naming one, and matches on the shape of the value. It returns
/// null until the user has actually signed in.
const EXTRACT_JS: &str = r#"
(function () {
  function fromString(raw) {
    if (!raw || raw.length < 40) return null;
    var v;
    try { v = JSON.parse(raw); } catch (e) { return null; }
    if (!v || typeof v !== 'object') return null;
    var access = v.access_token || v.accessToken || v.di_token;
    var refresh = v.refresh_token || v.refreshToken || v.di_refresh_token;
    if (typeof access === 'string' && typeof refresh === 'string') {
      return { access_token: access, refresh_token: refresh };
    }
    return null;
  }
  for (var store of [window.localStorage, window.sessionStorage]) {
    if (!store) continue;
    for (var i = 0; i < store.length; i++) {
      var hit = fromString(store.getItem(store.key(i)));
      if (hit) return JSON.stringify(hit);
    }
  }
  return null;
})()
"#;

/// Opens the sign-in window and waits for tokens to appear, writing them to the
/// keyring on success.
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

    let window = WebviewWindowBuilder::new(
        &app,
        WINDOW_LABEL,
        WebviewUrl::External(SIGN_IN_URL.parse().context("bad sign-in URL")?),
    )
    .title("Sign in to Garmin")
    .inner_size(520.0, 760.0)
    .build()
    .context("could not open the sign-in window")?;

    let _ = app.emit("garmin-login", "Waiting for you to sign in to Garmin…");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT_SECS);
    let result = loop {
        if std::time::Instant::now() > deadline {
            break Err(anyhow!("Timed out waiting for sign-in."));
        }
        // The user closing the window is a cancellation, not a failure to
        // report loudly — but the caller still needs to stop waiting.
        if app.get_webview_window(WINDOW_LABEL).is_none() {
            break Err(anyhow!("Sign-in window was closed."));
        }

        match probe(&window).await {
            Ok(Some(tokens)) => break Ok(tokens),
            Ok(None) => {}
            // An eval failure mid-navigation is expected: the page is between
            // documents. Only a persistent failure matters, and the timeout
            // catches that.
            Err(_) => {}
        }

        tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
    };

    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        let _ = w.close();
    }

    let tokens = result?;
    store::save_tokens(&tokens).context("could not write tokens to the keyring")?;
    let _ = app.emit("garmin-login", "Signed in.");
    Ok(())
}

/// One extraction attempt against the live page.
async fn probe(window: &tauri::WebviewWindow) -> Result<Option<Tokens>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    // `eval_with_callback` takes an `Fn`, not a `FnOnce`, so the sender has to
    // live behind something that can give it up on the first (and only) call.
    let tx = std::sync::Mutex::new(Some(tx));
    window
        .eval_with_callback(EXTRACT_JS, move |value| {
            if let Ok(mut guard) = tx.lock() {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(value);
                }
            }
        })
        .context("could not evaluate in the sign-in window")?;

    let raw = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .context("the sign-in window stopped responding")?
        .context("the sign-in window went away")?;

    parse_tokens(&raw)
}

/// The callback hands back a JSON-encoded value, so the payload is a JSON
/// string *containing* JSON — decode twice before looking at the fields.
fn parse_tokens(raw: &str) -> Result<Option<Tokens>> {
    let outer: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    let inner = match &outer {
        Value::String(s) => s.as_str(),
        Value::Null => return Ok(None),
        _ => raw,
    };
    if inner.is_empty() || inner == "null" {
        return Ok(None);
    }

    let v: Value = match serde_json::from_str(inner) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let (Some(access), Some(refresh)) = (v["access_token"].as_str(), v["refresh_token"].as_str())
    else {
        return Ok(None);
    };

    Ok(Some(Tokens {
        di_token: access.to_string(),
        di_refresh_token: refresh.to_string(),
        di_client_id: garmin_core::auth::DEFAULT_CLIENT_ID.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::parse_tokens;

    #[test]
    fn ignores_a_page_with_no_session() {
        assert!(parse_tokens("null").unwrap().is_none());
        assert!(parse_tokens("\"null\"").unwrap().is_none());
        assert!(parse_tokens("").unwrap().is_none());
    }

    #[test]
    fn ignores_unrelated_storage_entries() {
        assert!(parse_tokens("\"{\\\"theme\\\":\\\"dark\\\"}\"")
            .unwrap()
            .is_none());
    }

    #[test]
    fn reads_a_double_encoded_token_pair() {
        let inner = r#"{"access_token":"abc","refresh_token":"def"}"#;
        let outer = serde_json::to_string(inner).unwrap();
        let tokens = parse_tokens(&outer).unwrap().expect("should parse");
        assert_eq!(tokens.di_token, "abc");
        assert_eq!(tokens.di_refresh_token, "def");
    }
}
