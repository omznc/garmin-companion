//! One streamed completion request. A trait, so a test can use a fake stream.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use futures_util::{Stream, StreamExt};
use garmin_core::db::Db;
use serde_json::{json, Value};

use crate::cloud::CloudClient;
use crate::config::{load_config, openrouter_key, Provider};
use crate::sse::{SseParser, StreamResult};
use crate::turn::EventSink;

/// The answer ceiling that the hosted coach asks for. The worker holds credits
/// for this many answer tokens, so a lower value leaves room for a later turn.
/// The model thinks first, and the thinking counts in this limit (at least 1024
/// tokens at low effort), so a value near 1500 leaves almost no room to answer.
const CLOUD_MAX_TOKENS: u32 = 3000;

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
        // Ollama ignores `tool_choice`, so for it "none" means: send no tools.
        let tools =
            tools.filter(|_| !(self.provider == Provider::Ollama && tool_choice == Some("none")));
        if let Some(tools) = tools {
            body["tools"] = tools.clone();
            if let Some(choice) = tool_choice {
                body["tool_choice"] = json!(choice);
            }
        }
        let request = async {
            match &self.cloud {
                // The worker sets the model: the body carries none. A smaller
                // `max_tokens` makes the credit hold smaller.
                Some(cloud) => {
                    body["max_tokens"] = json!(CLOUD_MAX_TOKENS);
                    cloud.chat(&body).await
                }
                None => self.send(body).await,
            }
        };
        // Stop must work while the provider is slow to answer.
        let resp = tokio::select! {
            r = request => r?,
            () = stopped(stop) => return Ok(StreamResult::default()),
        };
        read_stream(resp, sink, stop).await
    }

    fn caches_prompt(&self) -> bool {
        self.provider != Provider::Ollama
    }
}

/// The longest wait for the next data event. A keepalive comment does not
/// count: a queued model can send them for ever.
const IDLE: Duration = Duration::from_secs(120);
/// How often a silent wait looks at the stop flag.
const TICK: Duration = Duration::from_millis(250);

async fn read_stream(
    resp: reqwest::Response,
    sink: &dyn EventSink,
    stop: &AtomicBool,
) -> Result<StreamResult> {
    read_events(resp.bytes_stream(), sink, stop, IDLE).await
}

async fn read_events<B, E>(
    mut stream: impl Stream<Item = Result<B, E>> + Unpin,
    sink: &dyn EventSink,
    stop: &AtomicBool,
    idle: Duration,
) -> Result<StreamResult>
where
    B: AsRef<[u8]>,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut parser = SseParser::default();
    let mut last_data = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            // A half-built tool call is dropped: nobody waits for it.
            let mut out = parser.finish();
            out.tool_calls.clear();
            return Ok(out);
        }
        let chunk = match tokio::time::timeout(TICK, stream.next()).await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => return Ok(parser.finish()),
            Err(_) if last_data.elapsed() < idle => continue,
            Err(_) => return Err(anyhow!(STALLED)),
        };
        let before = parser.data_events();
        parser.feed(
            chunk.context("the model stream was cut short")?.as_ref(),
            sink,
        )?;
        if parser.data_events() != before {
            last_data = Instant::now();
        } else if last_data.elapsed() >= idle {
            // Only keepalive comments arrive.
            return Err(anyhow!(STALLED));
        }
    }
}

const STALLED: &str = "The model stopped answering. Try again, or choose a different model.";

/// Wait until `stop` is set.
async fn stopped(stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        tokio::time::sleep(TICK).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::Event;
    use std::io;

    struct Quiet;
    impl EventSink for Quiet {
        fn emit(&self, _: Event) {}
    }

    /// A provider that keeps the connection open and sends only keepalive
    /// comments, as OpenRouter does for a queued model.
    fn keepalives() -> impl Stream<Item = Result<Vec<u8>, io::Error>> + Unpin {
        Box::pin(futures_util::stream::unfold((), |()| async {
            tokio::time::sleep(Duration::from_millis(40)).await;
            Some((Ok(b": OPENROUTER PROCESSING\n\n".to_vec()), ()))
        }))
    }

    #[tokio::test]
    async fn a_stream_of_keepalives_ends_with_a_readable_error() {
        let stop = AtomicBool::new(false);
        let run = read_events(keepalives(), &Quiet, &stop, Duration::from_millis(300));
        let out = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("the read must end, not wait for ever");
        assert_eq!(out.unwrap_err().to_string(), STALLED);
    }

    #[tokio::test]
    async fn a_silent_stream_ends_and_stop_ends_it_at_once() {
        let silent = || futures_util::stream::pending::<Result<Vec<u8>, io::Error>>();
        let idle = Duration::from_millis(300);
        let err = read_events(silent(), &Quiet, &AtomicBool::new(false), idle).await;
        assert_eq!(err.unwrap_err().to_string(), STALLED);
        let stop = AtomicBool::new(true);
        let out = read_events(silent(), &Quiet, &stop, Duration::from_secs(60)).await;
        assert!(out.unwrap().content.is_empty());
    }
}
