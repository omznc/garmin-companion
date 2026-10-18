//! The hosted coach: "Garmin Companion cloud". The contract is `worker/V2.md`.
//!
//! A free user is anonymous and holds an install id that the worker issues.
//! A premium user signs in by email code and holds a session token. Both live
//! in the secret store, behind a [`Vault`]. The UI never sees a token: it sees
//! an [`AccountView`] with the sentences already built.

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use garmin_core::store;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const CLOUD_BASE: &str = "https://coach.omznc.workers.dev/v2";
/// The worker refuses a larger chat body with 413.
const BODY_LIMIT: usize = 100_000;
/// Left in place of a tool result that was dropped to make the body fit.
const LEFT_OUT: &str = "{\"note\":\"An older result was left out to save space.\"}";

/* ------------------------------------------------------------------ vault --- */

/// Where the two tokens are kept. The real one is the OS secret store.
pub trait Vault: Send + Sync {
    fn install_id(&self) -> Result<Option<String>>;
    fn save_install_id(&self, id: &str) -> Result<()>;
    fn forget_install_id(&self) -> Result<()>;
    fn session(&self) -> Result<Option<String>>;
    fn save_session(&self, token: &str) -> Result<()>;
    fn clear_session(&self) -> Result<()>;
}

/// The keyring on desktop, the encrypted file on Android, the Keychain on iOS.
pub struct KeyringVault;

impl Vault for KeyringVault {
    fn install_id(&self) -> Result<Option<String>> {
        store::stored_install_id_v2()
    }
    fn save_install_id(&self, id: &str) -> Result<()> {
        store::save_install_id_v2(id)
    }
    fn forget_install_id(&self) -> Result<()> {
        store::forget_install_id_v2()
    }
    fn session(&self) -> Result<Option<String>> {
        store::stored_session()
    }
    fn save_session(&self, token: &str) -> Result<()> {
        store::save_session(token)
    }
    fn clear_session(&self) -> Result<()> {
        store::clear_session()
    }
}

/// Tokens in memory only: for tests and for a tool that must leave no trace.
#[derive(Default)]
pub struct MemoryVault(Mutex<(Option<String>, Option<String>)>);

impl MemoryVault {
    fn with<T>(&self, f: impl FnOnce(&mut (Option<String>, Option<String>)) -> T) -> T {
        f(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl Vault for MemoryVault {
    fn install_id(&self) -> Result<Option<String>> {
        Ok(self.with(|v| v.0.clone()))
    }
    fn save_install_id(&self, id: &str) -> Result<()> {
        self.with(|v| v.0 = Some(id.into()));
        Ok(())
    }
    fn forget_install_id(&self) -> Result<()> {
        self.with(|v| v.0 = None);
        Ok(())
    }
    fn session(&self) -> Result<Option<String>> {
        Ok(self.with(|v| v.1.clone()))
    }
    fn save_session(&self, token: &str) -> Result<()> {
        self.with(|v| v.1 = Some(token.into()));
        Ok(())
    }
    fn clear_session(&self) -> Result<()> {
        self.with(|v| v.1 = None);
        Ok(())
    }
}

/* ----------------------------------------------------------------- errors --- */

/// An error answer of the worker. `Display` is the sentence for the person.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudError {
    pub status: u16,
    /// `error.type` of the contract. The UI acts on this, not on the text.
    pub kind: String,
    pub message: String,
}

impl fmt::Display for CloudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CloudError {}

impl CloudError {
    fn parse(status: u16, retry_after: Option<u64>, body: &str, now: DateTime<Utc>) -> Self {
        let v: Value = serde_json::from_str(body).unwrap_or_default();
        let kind = v["error"]["type"].as_str().unwrap_or("unknown").to_string();
        let resets = v["error"]["resets_at"].as_str();
        let server = v["error"]["message"].as_str();
        let message = explain(&kind, status, resets, retry_after, server, now);
        Self {
            status,
            kind,
            message,
        }
    }

    fn unreachable() -> Self {
        Self {
            status: 0,
            kind: "unreachable".into(),
            message: "Could not reach the cloud coach. Check your connection.".into(),
        }
    }
}

/// The kind of a cloud error inside an `anyhow` error, if it has one.
pub fn error_kind(e: &anyhow::Error) -> Option<&str> {
    e.downcast_ref::<CloudError>().map(|c| c.kind.as_str())
}

/// The sentence for each error type that needs no number in it.
const FIXED: &[(&str, &str)] = &[
    (
        "coach_paused",
        "The cloud coach is paused. Try again later, or use your own key.",
    ),
    ("login_required", "Sign in with your email to use this."),
    (
        "session_expired",
        "Your sign-in expired. Sign in again in the coach settings.",
    ),
    (
        "blocked",
        "The cloud coach is blocked for this install or account.",
    ),
    (
        "upstream_error",
        "The model provider failed. Nothing was charged. Try again.",
    ),
    (
        "context_too_large",
        "This chat is too long. Start a new chat.",
    ),
    (
        "duplicate_request",
        "That request was already sent. Try again.",
    ),
    (
        "unknown_install",
        "The cloud coach does not know this install. Try again.",
    ),
    ("mail_unavailable", "Email sign-in is not available yet."),
    (
        "mail_failed",
        "The code email could not be sent. Try again later.",
    ),
    ("bad_email", "Enter a valid email address."),
    (
        "invalid_code",
        "That code is wrong or it expired. Check it, or ask for a new one.",
    ),
    ("already_premium", "This account is already premium."),
    ("no_customer", "This account has no purchase to manage."),
    (
        "billing_unavailable",
        "Billing is not available now. Try again later.",
    ),
    (
        "billing_failed",
        "Billing is not available now. Try again later.",
    ),
];

fn explain(
    kind: &str,
    status: u16,
    resets: Option<&str>,
    retry_after: Option<u64>,
    server: Option<&str>,
    now: DateTime<Utc>,
) -> String {
    let back = resets.map_or_else(|| "at the next reset".to_string(), |r| until(r, now));
    match kind {
        "out_of_credits" => format!(
            "You are out of credits. They return {back}. Get premium, or use your own key or Ollama in the coach settings."
        ),
        "fair_use_ceiling" => format!(
            "You reached the premium limit, and the free credits are spent. Credits return {back}."
        ),
        "rate_limited" => match retry_after {
            Some(s) => format!("Too many requests. Wait {}, then try again.", wait(s)),
            None => "Too many requests. Wait a minute, then try again.".into(),
        },
        "install_limit" => match retry_after {
            Some(s) => format!(
                "This network made too many new installs. Try again in {}.",
                wait(s)
            ),
            None => "This network made too many new installs. Try again later.".into(),
        },
        _ => FIXED.iter().find(|(k, _)| *k == kind).map_or_else(
            || match server {
                Some(m) => format!("The cloud coach said: {m}"),
                None => format!("The cloud coach returned an error ({status})."),
            },
            |(_, text)| (*text).into(),
        ),
    }
}

/// A wait in seconds, as the person reads it: "40 seconds", "12 min", "5 h 12 min".
fn wait(seconds: u64) -> String {
    match seconds {
        s if s < 90 => format!("{s} seconds"),
        s if s < 3600 => format!("{} min", s.div_ceil(60)),
        s => {
            let m = s.div_ceil(60);
            format!("{} h {} min", m / 60, m % 60)
        }
    }
}

/// "in 5 h 12 min" for a reset time, counted from `now`.
pub fn until(resets_at: &str, now: DateTime<Utc>) -> String {
    let Ok(at) = DateTime::parse_from_rfc3339(resets_at) else {
        return "at the next reset".into();
    };
    let minutes = (at.with_timezone(&Utc) - now).num_minutes();
    match minutes {
        m if m <= 0 => "soon".into(),
        m if m < 60 => format!("in {m} min"),
        m => format!("in {} h {} min", m / 60, m % 60),
    }
}

/* ---------------------------------------------------------------- balance --- */

/// `GET /balance`, as the worker sends it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Balance {
    #[serde(default)]
    pub premium: bool,
    pub email: Option<String>,
    #[serde(default)]
    pub credits_remaining: u64,
    #[serde(default)]
    pub daily_grant: u64,
    pub resets_at: Option<String>,
    #[serde(default)]
    pub month_used: u64,
    #[serde(default)]
    pub month_ceiling: u64,
    pub renews_at: Option<String>,
    #[serde(default)]
    pub cancels_at_period_end: bool,
}

/// What a screen needs to draw the cloud account. The core builds the lines.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub premium: bool,
    pub email: Option<String>,
    pub credits_remaining: u64,
    /// The small line near the composer.
    pub credits_line: String,
    pub plan_line: String,
    /// No credits left: the screen shows the upgrade card.
    pub out: bool,
    /// Below a fifth of the daily grant: the screen shows a quiet line.
    pub low: bool,
}

impl Balance {
    pub fn view(&self, now: DateTime<Utc>) -> AccountView {
        let left = self.credits_remaining;
        let back = self
            .resets_at
            .as_deref()
            .map_or_else(|| "at the next reset".into(), |r| until(r, now));
        let credits_line = match (left, self.premium) {
            (0, _) => format!("No credits left. They return {back}."),
            (n, true) => format!(
                "{n} credits left today. {} of {} used this month.",
                self.month_used, self.month_ceiling
            ),
            (n, false) => format!("{n} of {} credits left today.", self.daily_grant),
        };
        AccountView {
            premium: self.premium,
            email: self.email.clone(),
            credits_remaining: left,
            credits_line,
            plan_line: self.plan_line(),
            out: left == 0,
            low: self.daily_grant > 0 && left * 5 < self.daily_grant,
        }
    }

    fn plan_line(&self) -> String {
        let who = self.email.as_deref();
        if !self.premium {
            return match who {
                Some(email) => format!("Signed in as {email}. Free plan."),
                None => format!("Free plan: {} credits a day.", self.daily_grant),
            };
        }
        let date = self
            .renews_at
            .as_deref()
            .map_or("", |d| d.get(..10).unwrap_or(d));
        let verb = if self.cancels_at_period_end {
            "It ends on"
        } else {
            "It renews on"
        };
        let name = who.map(|e| format!(" for {e}")).unwrap_or_default();
        format!("Premium{name}. {verb} {date}.")
    }
}

/* ----------------------------------------------------------------- client --- */

#[derive(Clone)]
pub struct CloudClient {
    pub(crate) http: reqwest::Client,
    base: String,
    vault: Arc<dyn Vault>,
}

impl Default for CloudClient {
    fn default() -> Self {
        Self::new(CLOUD_BASE, Arc::new(KeyringVault))
    }
}

impl CloudClient {
    pub fn new(base: &str, vault: Arc<dyn Vault>) -> Self {
        Self {
            http: reqwest::Client::builder()
                // No total timeout: a long answer streams for minutes. A dead
                // connection stops the read.
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(90))
                .build()
                .unwrap_or_default(),
            base: base.trim_end_matches('/').into(),
            vault,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// The bearer token: the session if there is one, else the install id.
    /// The flag says that the token is an install id.
    async fn token(&self) -> Result<(String, bool)> {
        if let Some(s) = self.vault.session()? {
            return Ok((s, false));
        }
        if let Some(id) = self.vault.install_id()? {
            return Ok((id, true));
        }
        Ok((self.enroll().await?, true))
    }

    /// Ask the worker for an install id and keep it. One enrol runs at a time:
    /// a second caller finds the id that the first one stored.
    async fn enroll(&self) -> Result<String> {
        static ENROLL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let _lock = ENROLL.lock().await;
        if let Some(id) = self.vault.install_id()? {
            return Ok(id);
        }
        let resp = self.plain(self.http.post(self.url("/install"))).await?;
        let v: Value = resp
            .json()
            .await
            .context("the install answer was not JSON")?;
        let id = v["id"].as_str().context("the install answer had no id")?;
        self.vault.save_install_id(id)?;
        Ok(id.into())
    }

    /// Send a request that needs no token, and turn an error answer into an error.
    async fn plain(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let resp = req
            .send()
            .await
            .map_err(|e| anyhow!(e).context(CloudError::unreachable()))?;
        if resp.status().is_success() {
            return Ok(resp);
        }
        Err(read_error(resp).await.into())
    }

    /// Send a request with the bearer token. A forgotten install id is issued
    /// again once. A gone session is dropped.
    async fn authed(
        &self,
        build: impl Fn(&str) -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response> {
        let mut retried = false;
        loop {
            let (token, is_install) = self.token().await?;
            match self.plain(build(&token)).await {
                Ok(resp) => return Ok(resp),
                Err(e) => match error_kind(&e) {
                    Some("unknown_install") if is_install && !retried => {
                        retried = true;
                        self.vault.forget_install_id()?;
                    }
                    Some("session_expired") => {
                        self.vault.clear_session()?;
                        return Err(e);
                    }
                    _ => return Err(e),
                },
            }
        }
    }

    pub async fn balance(&self) -> Result<Balance> {
        let resp = self
            .authed(|t| self.http.get(self.url("/balance")).bearer_auth(t))
            .await?;
        resp.json().await.context("the balance was not JSON")
    }

    /// `POST /auth/email/send`. Answers 503 until the worker has a mail sender.
    pub async fn send_code(&self, email: &str) -> Result<()> {
        let req = self
            .http
            .post(self.url("/auth/email/send"))
            .json(&json!({ "email": email }));
        self.plain(req).await.map(|_| ())
    }

    /// `POST /auth/email/verify`. The session is stored. Returns the email.
    pub async fn verify_code(&self, email: &str, code: &str) -> Result<String> {
        let req = self
            .http
            .post(self.url("/auth/email/verify"))
            .json(&json!({ "email": email, "code": code }));
        let v: Value = self.plain(req).await?.json().await?;
        let token = v["token"].as_str().context("the answer had no session")?;
        self.vault.save_session(token)?;
        Ok(v["email"].as_str().unwrap_or(email).into())
    }

    /// Revoke the session on the worker and drop it here. A failed revoke does
    /// not keep the user signed in.
    pub async fn sign_out(&self) -> Result<()> {
        if let Some(s) = self.vault.session()? {
            let req = self.http.post(self.url("/auth/signout")).bearer_auth(s);
            let _ = self.plain(req).await;
        }
        self.vault.clear_session()
    }

    /// The Polar checkout page. The caller opens it in the system browser.
    pub async fn checkout_url(&self) -> Result<String> {
        self.link("/billing/checkout").await
    }

    /// The Polar customer portal: cancel, card, invoices.
    pub async fn portal_url(&self) -> Result<String> {
        self.link("/billing/portal").await
    }

    async fn link(&self, path: &str) -> Result<String> {
        let resp = self
            .authed(|t| self.http.post(self.url(path)).bearer_auth(t))
            .await?;
        let v: Value = resp.json().await?;
        let url = v["url"].as_str().context("the answer had no link")?;
        // The caller opens this in the browser: refuse any other scheme.
        anyhow::ensure!(
            url.starts_with("https://"),
            "the link was not an https link"
        );
        Ok(url.into())
    }

    /// One streamed chat request. Each HTTP request has its own id, so the
    /// worker holds credits for it once. A body that is too large is trimmed.
    pub(crate) async fn chat(&self, body: &Value) -> Result<reqwest::Response> {
        let mut limit = BODY_LIMIT;
        loop {
            let mut sized = body.clone();
            fit(&mut sized, limit);
            let sent = self
                .authed(|t| {
                    self.http
                        .post(self.url("/chat/completions"))
                        .bearer_auth(t)
                        .header("X-Request-Id", uuid::Uuid::new_v4().to_string())
                        .json(&sized)
                })
                .await;
            match sent {
                Err(e) if error_kind(&e) == Some("context_too_large") && limit == BODY_LIMIT => {
                    limit /= 2;
                }
                other => return other,
            }
        }
    }
}

async fn read_error(resp: reqwest::Response) -> CloudError {
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let text = resp.text().await.unwrap_or_default();
    CloudError::parse(status, retry_after, &text, Utc::now())
}

/// Drop the oldest tool results until the body is within `limit` bytes. The
/// newest tool result and every other message stay.
fn fit(body: &mut Value, limit: usize) {
    while body.to_string().len() > limit {
        let Some(messages) = body["messages"].as_array_mut() else {
            return;
        };
        let tools: Vec<usize> = (0..messages.len())
            .filter(|&i| messages[i]["role"] == "tool" && messages[i]["content"] != LEFT_OUT)
            .collect();
        // Keep the newest one: the model needs it to answer.
        let [oldest, _, ..] = tools[..] else { return };
        messages[oldest]["content"] = json!(LEFT_OUT);
    }
}

#[cfg(test)]
mod tests;
