//! Token persistence. Tokens live in the OS keyring (kwallet/gnome-keyring,
//! Keychain, Credential Manager) — never in the app's own config files, and
//! never anywhere the webview can reach them.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::auth::Tokens;

const SERVICE: &str = "no.omznc.garmincoach";
const ACCOUNT_GARMIN: &str = "garmin-di-tokens";
const ACCOUNT_OPENROUTER: &str = "openrouter-api-key";

fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).context("could not open the OS keyring")
}

pub fn save_tokens(tokens: &Tokens) -> Result<()> {
    let json = serde_json::to_string(tokens)?;
    entry(ACCOUNT_GARMIN)?
        .set_password(&json)
        .context("could not write Garmin tokens to the keyring")
}

pub fn load_tokens() -> Result<Option<Tokens>> {
    match entry(ACCOUNT_GARMIN)?.get_password() {
        Ok(json) => Ok(Some(
            serde_json::from_str(&json).context("stored Garmin tokens are corrupt")?,
        )),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).context("could not read Garmin tokens from the keyring"),
    }
}

pub fn clear_tokens() -> Result<()> {
    match entry(ACCOUNT_GARMIN)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("could not clear Garmin tokens"),
    }
}

pub fn save_openrouter_key(key: &str) -> Result<()> {
    entry(ACCOUNT_OPENROUTER)?
        .set_password(key)
        .context("could not write the OpenRouter key to the keyring")
}

pub fn load_openrouter_key() -> Result<Option<String>> {
    match entry(ACCOUNT_OPENROUTER)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).context("could not read the OpenRouter key"),
    }
}

pub fn clear_openrouter_key() -> Result<()> {
    match entry(ACCOUNT_OPENROUTER)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("could not clear the OpenRouter key"),
    }
}

/// Default location of the token file written by the `garminconnect` Python
/// library, so an existing MCP setup can be adopted without logging in again.
pub fn python_token_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".garminconnect").join("garmin_tokens.json"))
}

/// Read a `garminconnect`-format token file. Only the three DI fields matter;
/// older files carrying oauth1/oauth2 blobs are rejected outright rather than
/// half-imported, since those predate the current auth flow and won't refresh.
pub fn import_python_tokens(path: &Path) -> Result<Tokens> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).context("token file is not valid JSON")?;

    let field = |name: &str| -> Result<String> {
        value
            .get(name)
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .with_context(|| {
                format!(
                    "token file has no `{name}` — it predates Garmin's current \
                     DI auth flow, so you'll need to log in again"
                )
            })
    };

    Ok(Tokens {
        di_token: field("di_token")?,
        di_refresh_token: field("di_refresh_token")?,
        di_client_id: field("di_client_id")?,
    })
}
