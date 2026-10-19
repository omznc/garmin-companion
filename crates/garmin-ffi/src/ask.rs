//! The Ask commands of the bridge: the coach chat.
//!
//! `ask_send` starts a turn and returns at once. The turn sends `ask` events
//! (the JSON of `garmin_coach::Event`) through the hub until it ends with a
//! `done` or `error` event. One turn runs at a time: a new one stops the old.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use garmin_coach::{Event, EventSink, HistoryMessage};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::FfiError;

struct HubSink;

impl EventSink for HubSink {
    fn emit(&self, event: Event) {
        crate::emit("ask", serde_json::to_value(&event).unwrap_or(Value::Null));
    }
}

static RUNNING: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// The coach choice in sample mode: provider and model. It stays in memory,
/// because a real save would create the cache and end sample mode.
static SAMPLE_CONFIG: Mutex<Option<(String, String)>> = Mutex::new(None);

pub(crate) fn bad(e: impl ToString) -> FfiError {
    FfiError::BadArgs {
        reason: e.to_string(),
    }
}

pub(crate) fn failed(e: impl ToString) -> FfiError {
    FfiError::Command {
        json: json!({ "kind": "internal", "message": e.to_string() }).to_string(),
    }
}

#[derive(Deserialize)]
struct Send {
    history: Vec<HistoryRow>,
}

#[derive(Deserialize)]
struct HistoryRow {
    role: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Save {
    provider: String,
    model: Option<String>,
    key: Option<String>,
}

/// Handle an `ask_*` command, or return `None` for any other command.
pub(crate) async fn handle(
    cmd: &str,
    args: &Value,
    fixture: bool,
) -> Option<Result<String, FfiError>> {
    Some(match cmd {
        "ask_send" => send(args, fixture),
        "ask_stop" => {
            stop();
            Ok("{}".into())
        }
        "ask_config" => config(fixture).await,
        "ask_save_config" => save(args, fixture),
        _ => return None,
    })
}

fn stop() {
    if let Some(flag) = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).take() {
        flag.store(true, Ordering::Relaxed);
    }
}

fn send(args: &Value, fixture: bool) -> Result<String, FfiError> {
    let a: Send = serde_json::from_value(args.clone()).map_err(bad)?;
    let history: Vec<HistoryMessage> = a
        .history
        .into_iter()
        .map(|m| HistoryMessage {
            role: m.role,
            content: m.content,
        })
        .collect();
    stop();
    let flag = Arc::new(AtomicBool::new(false));
    *RUNNING.lock().unwrap_or_else(|e| e.into_inner()) = Some(flag.clone());
    let turn = if fixture {
        tokio::spawn(fixture_turn(flag))
    } else {
        tokio::spawn(async move { garmin_coach::ask(history, &HubSink, &flag).await })
    };
    tokio::spawn(report_panic(turn, HubSink));
    Ok("{}".into())
}

/// A panic in the turn ends it with no event, and the screen would stay busy.
async fn report_panic(turn: tokio::task::JoinHandle<()>, sink: impl EventSink) {
    if turn.await.is_err() {
        sink.emit(Event::Error {
            text: "The coach stopped unexpectedly. Try again.".into(),
        });
    }
}

/// A canned answer for the sample mode, so the screen runs with no model.
async fn fixture_turn(stop: Arc<AtomicBool>) {
    let sink = HubSink;
    let label = "Reading 14 days of recovery signals";
    sink.emit(Event::Tool {
        call_id: "fixture".into(),
        label: label.into(),
        running: true,
        ok: true,
    });
    tokio::time::sleep(Duration::from_millis(400)).await;
    sink.emit(Event::Tool {
        call_id: "fixture".into(),
        label: label.into(),
        running: false,
        ok: true,
    });
    let answer = "This is a sample answer. With a model set up, the coach reads \
        your own Garmin data first, then answers. Sleep and recovery come \
        first, and sport advice follows only when your data shows that sport.";
    for word in answer.split_inclusive(' ') {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        sink.emit(Event::Delta { text: word.into() });
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    sink.emit(Event::Done {
        sources: vec![label.into()],
    });
}

async fn config(fixture: bool) -> Result<String, FfiError> {
    if fixture {
        let saved = SAMPLE_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
        let (provider, model) = saved.clone().unwrap_or(("ollama".into(), "sample".into()));
        return Ok(
            json!({ "provider": provider, "model": model, "hasKey": false,
            "ollamaReachable": true, "ollamaModels": ["sample"] })
            .to_string(),
        );
    }
    let (provider, model) = tokio::task::spawn_blocking(|| {
        let db = garmin_api::open_default_db().map_err(|e| e.to_string())?;
        garmin_coach::load_config(&db).map_err(|e| e.to_string())
    })
    .await
    .map_err(failed)?
    .map_err(failed)?;
    let has_key = matches!(garmin_core::store::load_openrouter_key(), Ok(Some(_)));
    let http = reqwest::Client::new();
    let (ollama_reachable, ollama_models) = garmin_coach::probe_ollama(&http).await;
    serde_json::to_string(&garmin_coach::ChatConfig {
        provider: provider.map(garmin_coach::Provider::as_str),
        model,
        has_key,
        ollama_reachable,
        ollama_models,
    })
    .map_err(failed)
}

fn save(args: &Value, fixture: bool) -> Result<String, FfiError> {
    let a: Save = serde_json::from_value(args.clone()).map_err(bad)?;
    let provider =
        garmin_coach::Provider::parse(&a.provider).ok_or_else(|| bad("unknown provider"))?;
    if fixture {
        // The key is dropped on purpose: sample mode stores no secret.
        let model = a.model.unwrap_or_default();
        *SAMPLE_CONFIG.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((provider.as_str().to_owned(), model));
        return Ok("{}".into());
    }
    if let Some(key) = a.key.as_deref().filter(|k| !k.is_empty()) {
        garmin_core::store::save_openrouter_key(key).map_err(failed)?;
    }
    let db = garmin_api::open_default_db().map_err(failed)?;
    let model = a.model.unwrap_or_else(|| match provider {
        garmin_coach::Provider::Openrouter => garmin_coach::DEFAULT_OPENROUTER_MODEL.into(),
        _ => String::new(),
    });
    garmin_coach::save_config(&db, provider, &model).map_err(failed)?;
    Ok("{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn only_ask_commands_are_taken() {
        assert!(handle("today", &json!({}), true).await.is_none());
        assert!(handle("ask_stop", &json!({}), true).await.is_some());
    }

    #[tokio::test]
    async fn a_send_without_history_is_a_bad_argument() {
        let out = handle("ask_send", &json!({}), true).await.unwrap();
        assert!(matches!(out, Err(FfiError::BadArgs { .. })));
    }

    #[derive(Default, Clone)]
    struct Rec(Arc<Mutex<Vec<Event>>>);
    impl EventSink for Rec {
        fn emit(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[tokio::test]
    async fn a_panicking_turn_ends_with_an_error_event() {
        let rec = Rec::default();
        let turn = tokio::spawn(async { panic!("boom") });
        report_panic(turn, rec.clone()).await;
        assert!(matches!(
            rec.0.lock().unwrap().as_slice(),
            [Event::Error { .. }]
        ));
        // A turn that ends normally adds nothing.
        let ok = Rec::default();
        report_panic(tokio::spawn(async {}), ok.clone()).await;
        assert!(ok.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_unknown_provider_is_a_bad_argument() {
        let out = handle("ask_save_config", &json!({ "provider": "x" }), false)
            .await
            .unwrap();
        assert!(matches!(out, Err(FfiError::BadArgs { .. })));
    }
}
