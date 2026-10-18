//! Which provider and model the coach uses.
//!
//! The choice lives in the cache's key-value table, under the same keys as the
//! old app. The OpenRouter key lives in the secret store of `garmin-core`.

use anyhow::Result;
use garmin_core::{db::Db, store};
use serde::{Deserialize, Serialize};

pub(crate) const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";
pub(crate) const OLLAMA_BASE: &str = "http://localhost:11434/v1";

/// The model that OpenRouter gets when the user chose none. It is cheap, it has
/// a long context, and it calls tools. The coach cannot work without that.
pub const DEFAULT_OPENROUTER_MODEL: &str = "inclusionai/ling-3.0-flash";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// The hosted coach with credits ("Garmin Companion cloud"). See `cloud.rs`.
    Cloud,
    Openrouter,
    Ollama,
}

impl Provider {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cloud" => Some(Self::Cloud),
            "openrouter" => Some(Self::Openrouter),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::Openrouter => "openrouter",
            Self::Ollama => "ollama",
        }
    }

    /// The base URL of the chat API. The hosted coach has its own client.
    pub(crate) fn base(self) -> Option<&'static str> {
        match self {
            Self::Cloud => None,
            Self::Openrouter => Some(OPENROUTER_BASE),
            Self::Ollama => Some(OLLAMA_BASE),
        }
    }

    /// The model to use when none was chosen. Ollama has none: it depends on
    /// what the user pulled.
    fn default_model(self) -> Option<&'static str> {
        match self {
            Self::Openrouter => Some(DEFAULT_OPENROUTER_MODEL),
            Self::Cloud | Self::Ollama => None,
        }
    }
}

/// What a settings screen needs to draw the current choice.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatConfig {
    pub provider: Option<&'static str>,
    pub model: Option<String>,
    pub has_key: bool,
    pub ollama_reachable: bool,
    pub ollama_models: Vec<String>,
}

/// The provider when none was saved. Own OpenRouter key first, else the hosted
/// coach. A local Ollama needs a model name, so the user picks it on purpose.
pub(crate) fn default_provider(has_own_key: bool) -> Provider {
    if has_own_key {
        Provider::Openrouter
    } else {
        Provider::Cloud
    }
}

/// The saved choice, or the default. The provider is always set.
pub fn load_config(db: &Db) -> Result<(Option<Provider>, Option<String>)> {
    let provider = match db
        .sync_state("chat_provider")?
        .and_then(|s| Provider::parse(&s))
    {
        Some(p) => p,
        None => default_provider(matches!(openrouter_key(), Ok(Some(_)))),
    };
    let provider = Some(provider);
    let model = match (db.sync_state("chat_model")?, provider) {
        (Some(m), _) => Some(m),
        (None, Some(p)) => p.default_model().map(str::to_string),
        (None, None) => None,
    };
    Ok((provider, model))
}

pub fn save_config(db: &Db, provider: Provider, model: &str) -> Result<()> {
    db.set_sync_state("chat_provider", provider.as_str())?;
    db.set_sync_state("chat_model", model)
}

/// The key of the user's own OpenRouter account, if one is stored.
pub(crate) fn openrouter_key() -> Result<Option<String>> {
    store::load_openrouter_key()
}

/// Whether a local Ollama runs, and which models it has. Failure is an answer
/// ("not running"), not an error.
pub async fn probe_ollama(http: &reqwest::Client) -> (bool, Vec<String>) {
    let resp = http
        .get(format!("{OLLAMA_BASE}/models"))
        .timeout(std::time::Duration::from_millis(700))
        .send()
        .await;
    let Ok(resp) = resp else {
        return (false, Vec::new());
    };
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let models = body["data"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|m| m["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    (true, models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_round_trip_through_their_names() {
        for p in [Provider::Cloud, Provider::Openrouter, Provider::Ollama] {
            assert_eq!(Provider::parse(p.as_str()), Some(p));
        }
        assert_eq!(Provider::parse("nope"), None);
    }

    #[test]
    fn the_hosted_coach_has_its_own_client_and_no_base_url() {
        assert!(Provider::Cloud.base().is_none());
        assert!(Provider::Ollama.base().is_some());
    }

    #[test]
    fn the_hosted_coach_is_the_default_without_an_own_key() {
        assert_eq!(default_provider(false), Provider::Cloud);
        assert_eq!(default_provider(true), Provider::Openrouter);
    }
}
