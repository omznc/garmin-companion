//! One streamed completion request. A trait, so a test can use a fake stream.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use garmin_core::db::Db;
use serde_json::{json, Value};

use crate::cloud::CloudClient;
use crate::config::{load_config, openrouter_key, Provider};
use crate::sse::{SseParser, StreamResult};
use crate::turn::EventSink;

/// The answer ceiling that the hosted coach asks for. The worker holds credits
/// for this many answer tokens, so a lower value leaves room for a later turn.
const CLOUD_MAX_TOKENS: u32 = 1500;

pub trait Transport: Send + Sync {
    /// Send the messages and read the answer. Text deltas go to the sink as
    /// they arrive. When `stop` is set, the call returns what it has and drops
    /// the connection, so a stopped turn stops costing money. `tool_choice` is
    /// sent with the tools: `"none"` keeps the schemas in the request, which
    /// the API needs once the messages hold tool calls, and forces an answer.
    fn complete(
        &self,
        messages: &[Value],
        tools: Option<&Value>,
        tool_choice: Option<&str>,
        sink: &dyn EventSink,
        stop: &AtomicBool,
    ) -> impl Future<Output = Result<StreamResult>> + Send;

    /// Whether the system message carries a prompt cache marker.
    fn caches_prompt(&self) -> bool {
        false
    }
}

/// An OpenAI-compatible chat API: OpenRouter with the user's own key, Ollama,
/// or the hosted coach (`cloud` is set, and the worker picks the model).
pub struct HttpTransport {
    http: reqwest::Client,
    provider: Provider,
    base: &'static str,
    model: String,
    key: Option<String>,
    cloud: Option<CloudClient>,
}

impl HttpTransport {
    /// Build the transport that the saved config asks for.
    pub fn from_config(db: &Db) -> Result<Self> {
        let (provider, model) = load_config(db)?;
        let provider =
            provider.context("No model provider chosen yet. Choose one in the coach settings.")?;
        if provider == Provider::Cloud {
            return Ok(Self::cloud(CloudClient::default()));
        }
        let model = model.context("No model chosen yet.")?;
        let base = provider.base().context("This provider has no address.")?;
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
            cloud: None,
        })
    }

    /// The transport of the hosted coach.
    pub fn cloud(client: CloudClient) -> Self {
        Self {
            http: client.http.clone(),
            provider: Provider::Cloud,
            base: "",
            model: String::new(),
            key: None,
            cloud: Some(client),
        }
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

    async fn send(&self, mut body: Value) -> Result<reqwest::Response> {
        body["model"] = json!(self.model);
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
        Ok(resp)
    }
}

impl Transport for HttpTransport {
    async fn complete(
        &self,
        messages: &[Value],
        tools: Option<&Value>,
        tool_choice: Option<&str>,
        sink: &dyn EventSink,
        stop: &AtomicBool,
    ) -> Result<StreamResult> {
        let mut body = json!({ "messages": messages, "stream": true });
        if let Some(tools) = tools {
            body["tools"] = tools.clone();
            if let Some(choice) = tool_choice {
                body["tool_choice"] = json!(choice);
            }
        }
        let resp = match &self.cloud {
            // The worker sets the model: the body carries none. A smaller
            // `max_tokens` makes the credit hold smaller.
            Some(cloud) => {
                body["max_tokens"] = json!(CLOUD_MAX_TOKENS);
                cloud.chat(&body).await?
            }
            None => self.send(body).await?,
        };
        read_stream(resp, sink, stop).await
    }

    fn caches_prompt(&self) -> bool {
        self.provider != Provider::Ollama
    }
}

async fn read_stream(
    resp: reqwest::Response,
    sink: &dyn EventSink,
    stop: &AtomicBool,
) -> Result<StreamResult> {
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
