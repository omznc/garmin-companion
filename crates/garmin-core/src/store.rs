//! Token persistence. Tokens live in the OS keyring (kwallet/gnome-keyring,
//! Keychain, Credential Manager) — never in the app's own config files, and
//! never anywhere the webview can reach them.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::auth::Tokens;

const SERVICE: &str = "no.omznc.garmincoach";
const ACCOUNT_GARMIN: &str = "garmin-di-tokens";
const ACCOUNT_OPENROUTER: &str = "openrouter-api-key";
/// Renamed when ids stopped being minted here and started being issued by the
/// proxy. An install carrying one of the old locally-minted ids finds nothing
/// under this name and enrols once, quietly, instead of sending an id the
/// server has never heard of and being refused for it.
const ACCOUNT_DEVICE: &str = "cloud-install-id";

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

/// This install's identifier for the hosted proxy, issued by it on first use.
///
/// The proxy pays for every request it forwards, so it needs something to count
/// and something to revoke — an open OpenAI-compatible endpoint with a funded
/// key behind it gets found. This is that something, and it is deliberately the
/// least it can be: a random value tied to no account, carrying no name, never
/// sent anywhere except as the bearer token on a request the athlete just made.
///
/// It used to be minted here, which meant the per-install limits on the other
/// end counted something this side could replace at will. It comes from the
/// server now (`POST /v1/install`), and this module only stores what it is
/// given — see `chat::enroll` for the asking.
///
/// It lives in the keyring beside the Garmin tokens rather than in the cache,
/// for the same reason they do: the webview can read the database.
pub fn stored_install_id() -> Result<Option<String>> {
    match entry(ACCOUNT_DEVICE)?.get_password() {
        Ok(id) if is_install_id(&id) => Ok(Some(id)),
        // A keyring entry someone hand-edited is not an id. Enrol again rather
        // than send the proxy something it will only refuse.
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).context("could not read the install id from the keyring"),
    }
}

pub fn save_install_id(id: &str) -> Result<()> {
    if !is_install_id(id) {
        bail!("the coach issued an id in a shape this build doesn't recognise");
    }
    entry(ACCOUNT_DEVICE)?
        .set_password(id)
        .context("could not write the install id to the keyring")
}

/// Forget the id, so the next hosted request asks for a new one.
///
/// Not the reset button that used to be in Settings, and nothing in the UI
/// calls it. It is for the single case where the proxy says it has never heard
/// of the id we hold — an expired record, or one deleted by hand — where the
/// choice is between enrolling again and being permanently unable to ask a
/// question. Issuing is rate-limited on the server, so this costs a slot rather
/// than handing out a fresh quota.
pub fn forget_install_id() -> Result<()> {
    match entry(ACCOUNT_DEVICE)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("could not clear the install id"),
    }
}

/// The shape the proxy issues and the only shape it accepts.
fn is_install_id(id: &str) -> bool {
    id.len() == 32
        && id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
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

#[cfg(test)]
mod tests {
    use super::is_install_id;

    /// The same shape the worker's `deviceId` accepts. Both ends check it, and
    /// they have to agree: an id this side would store but that side refuses is
    /// an install that enrols successfully and then cannot ask anything.
    #[test]
    fn an_install_id_is_thirty_two_lowercase_hex_characters() {
        assert!(is_install_id("0123456789abcdef0123456789abcdef"));

        assert!(!is_install_id(""));
        assert!(!is_install_id("short"));
        // A UUID with its dashes, which is what the server mints before it
        // strips them — worth pinning, since it is the near miss.
        assert!(!is_install_id("01234567-89ab-cdef-0123-456789abcdef"));
        assert!(!is_install_id("0123456789ABCDEF0123456789ABCDEF"));
        assert!(!is_install_id("0123456789abcdef0123456789abcdef0"));
        assert!(!is_install_id("not hex but exactly 32 chars long"));
    }
}
