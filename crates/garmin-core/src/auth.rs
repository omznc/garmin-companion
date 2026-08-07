//! Garmin DI (Digital Identity) OAuth2 token handling.
//!
//! Garmin's SSO login endpoints sit behind Cloudflare TLS fingerprinting that
//! rejects non-browser clients. The token *refresh* and data endpoints do not —
//! verified against a live account with a plain OpenSSL client. So we only ever
//! refresh here; acquiring the first token pair is the webview's job.

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DI_TOKEN_URL: &str = "https://diauth.garmin.com/di-oauth2-service/oauth/token";
pub const CONNECT_API: &str = "https://connectapi.garmin.com";

/// The Connect Mobile Android client the DI flow expects. Garmin rotates these
/// per quarter; older ids keep working for a while, so we store whichever the
/// token was actually issued for rather than hardcoding one.
pub const DEFAULT_CLIENT_ID: &str = "GARMIN_CONNECT_MOBILE_ANDROID_DI_2025Q2";

const NATIVE_UA: &str = "GCM-Android-5.23";
const NATIVE_X_GARMIN_UA: &str = "com.garmin.android.apps.connectmobile/5.23; ; \
     Google/sdk_gphone64_arm64/google; Android/33; Dalvik/2.1.0";

/// Refresh a little early so a long-running sync can't have the token expire
/// out from under it mid-request.
const EXPIRY_SKEW_SECS: i64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub di_token: String,
    pub di_refresh_token: String,
    pub di_client_id: String,
}

impl Tokens {
    /// Seconds until the access token expires, read from the JWT `exp` claim.
    /// Returns `None` if the token isn't a readable JWT, which we treat as
    /// "refresh now" rather than "assume valid".
    pub fn expires_in_secs(&self) -> Option<i64> {
        let claims = decode_jwt_claims(&self.di_token)?;
        let exp = claims.get("exp")?.as_i64()?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
        Some(exp - now)
    }

    pub fn is_expired(&self) -> bool {
        match self.expires_in_secs() {
            Some(secs) => secs <= EXPIRY_SKEW_SECS,
            None => true,
        }
    }
}

fn decode_jwt_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn client_id_from_jwt(token: &str) -> Option<String> {
    decode_jwt_claims(token)?
        .get("client_id")?
        .as_str()
        .map(str::to_owned)
}

fn basic_auth(client_id: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{client_id}:"))
    )
}

/// Headers the Connect Mobile Android app sends. Garmin's edge is picky about
/// these on the auth host; omitting them yields 403s that look like bad
/// credentials but aren't.
pub fn native_headers() -> reqwest::header::HeaderMap {
    use reqwest::header::{HeaderMap, HeaderValue, ACCEPT_LANGUAGE, USER_AGENT};
    let mut h = HeaderMap::new();
    h.insert(USER_AGENT, HeaderValue::from_static(NATIVE_UA));
    h.insert(
        "X-Garmin-User-Agent",
        HeaderValue::from_static(NATIVE_X_GARMIN_UA),
    );
    h.insert(
        "X-Garmin-Paired-App-Version",
        HeaderValue::from_static("10861"),
    );
    h.insert(
        "X-Garmin-Client-Platform",
        HeaderValue::from_static("Android"),
    );
    h.insert("X-App-Ver", HeaderValue::from_static("10861"));
    h.insert("X-Lang", HeaderValue::from_static("en"));
    h.insert("X-GCExperience", HeaderValue::from_static("GC5"));
    h.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    h
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

/// Exchange the refresh token for a fresh access token.
///
/// Garmin may or may not rotate the refresh token on each call; when it does
/// we must persist the new one or the next refresh fails.
pub async fn refresh(client: &reqwest::Client, tokens: &Tokens) -> Result<Tokens> {
    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", tokens.di_client_id.as_str()),
        ("refresh_token", tokens.di_refresh_token.as_str()),
    ];

    let resp = client
        .post(DI_TOKEN_URL)
        .headers(native_headers())
        .header("Authorization", basic_auth(&tokens.di_client_id))
        .header("Accept", "application/json")
        .header("Cache-Control", "no-cache")
        .form(&form)
        .send()
        .await
        .context("DI token refresh request failed")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        // 400/401 here means the refresh token is dead and the user has to log
        // in again; anything else is likely transient. The caller distinguishes
        // these to decide whether to surface a re-login prompt.
        return Err(anyhow!(
            "DI token refresh failed: {} {}",
            status,
            body.chars().take(200).collect::<String>()
        ));
    }

    let parsed: TokenResponse =
        serde_json::from_str(&body).context("DI token refresh returned malformed JSON")?;

    let di_client_id =
        client_id_from_jwt(&parsed.access_token).unwrap_or_else(|| tokens.di_client_id.clone());

    Ok(Tokens {
        di_client_id,
        di_refresh_token: parsed
            .refresh_token
            .unwrap_or_else(|| tokens.di_refresh_token.clone()),
        di_token: parsed.access_token,
    })
}

/// True when the failure means "the user must log in again" rather than
/// "try again later".
pub fn is_auth_fatal(err: &anyhow::Error) -> bool {
    let msg = err.to_string();
    msg.contains("400") || msg.contains("401")
}
