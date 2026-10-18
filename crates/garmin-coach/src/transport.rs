//! One streamed completion request. A trait, so a test can use a fake stream.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use garmin_core::db::Db;
use serde_json::{json, Value};

use crate::config::{load_config, openrouter_key, Provider};
use crate::sse::{SseParser, StreamResult};
use crate::turn::EventSink;

pub trait Transport: Send + Sync {
    /// Send the messages and read the answer. Text deltas go to the sink as
    /// they arrive. When `stop` is set, the call returns what it has and drops
    /// the connection, so a stopped turn stops costing money.
    fn complete(
        &self,
        messages: &[Value],
        tools: Option<&Value>,
        sink: &dyn EventSink,
        stop: &AtomicBool,
    ) -> impl Future<Output = Result<StreamResult>> + Send;
}

/// An OpenAI-compatible chat API: OpenRouter with the user's own key, or Ollama.
pub struct HttpTransport {
    http: reqwest::Client,
    provider: Provider,
    base: &'static str,
    model: String,
    key: Option<String>,
}

impl HttpTransport {
    /// Build the transport that the saved config asks for.
    pub fn from_config(db: &Db) -> Result<Self> {
        let (provider, model) = load_config(db)?;
        let provider = provider.context("No model provider chosen yet.")?;
        let model = model.context("No model chosen yet.")?;
        let base = provider
            .base()
            .context("The hosted coach is not available in this app yet. Choose OpenRouter with your own key, or a local Ollama.")?;
        let key = match provider {
            Provider::Openrouter => Some(
                openrouter_key()?
                    .context("No OpenRouter key stored. Add one in the coach settings.")?,
            ),
            _ => None,
        };
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()?;
        Ok(Self {
            http,
            provider,
            base,
            model,
            key,
        })
    }

    fn request(&self, body: &Value) -> reqwest::RequestBuilder {
        let req = self
            .http
            .post(format!("{}/chat/completions", self.base))
            .json(body);
        let Some(key) = &self.key else { return req };
        // OpenRouter attributes requests by the two headers. They are optional.
        req.header("Authorization", format!("Bearer {key}"))
            .header("HTTP-Referer", "https://github.com/omznc/garmin-companion")
            .header("X-Title", "Garmin Companion")
    }
}

impl Transport for HttpTransport {
    async fn complete(
        &self,
        messages: &[Value],
        tools: Option<&Value>,
        sink: &dyn EventSink,
        stop: &AtomicBool,
    ) -> Result<StreamResult> {
        let mut body = json!({ "model": self.model, "messages": messages, "stream": true });
        if let Some(tools) = tools {
            body["tools"] = tools.clone();
        }
        let resp = self
            .request(&body)
            .send()
            .await
            .context("could not reach the model")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "{} returned {status}: {}",
                self.provider.as_str(),
                text.chars().take(300).collect::<String>()
            ));
        }
        let mut parser = SseParser::default();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if stop.load(Ordering::Relaxed) {
                // A half-built tool call is dropped: nobody waits for it.
                let mut out = parser.finish();
                out.tool_calls.clear();
                return Ok(out);
            }
            parser.feed(&chunk.context("the model stream was cut short")?, sink)?;
        }
        Ok(parser.finish())
    }
}
