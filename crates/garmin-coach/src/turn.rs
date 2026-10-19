//! One assistant turn: tool rounds until the model stops asking for tools,
//! then the streamed answer.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Result};
use garmin_core::db::Db;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::prompt::{grounding_message, system_message, trim_history};
use crate::tools;
use crate::transport::{HttpTransport, Transport};

/// Cap on tool rounds in one turn. Every tool reads a bounded slice, so a model
/// that keeps calling them is looping.
const MAX_TOOL_ROUNDS: usize = 6;

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

/// What a turn reports to the host. The JSON form is the payload of the mobile
/// `ask` event: `{"type":"delta","text":"..."}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    /// One tool call, twice: when it starts (`running`) and when it returns.
    /// `ok` has no meaning while `running` is true.
    Tool {
        call_id: String,
        label: String,
        running: bool,
        ok: bool,
    },
    Delta {
        text: String,
    },
    /// The turn ended. `sources` are the labels of what it read.
    Done {
        sources: Vec<String>,
    },
    Error {
        text: String,
    },
}

/// Where a turn sends its events. A host calls it from any thread.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: Event);
}

/// Run one turn with the saved config. The result goes to the sink as `Done`
/// or `Error`. A stopped turn ends with `Done`, and the host keeps the text.
pub async fn ask(history: Vec<HistoryMessage>, sink: &dyn EventSink, stop: &AtomicBool) {
    let transport = Db::open_default().and_then(|db| HttpTransport::from_config(&db));
    match transport {
        Ok(t) => {
            // The turn reports its own end to the sink.
            let _ = run_turn(&t, sink, stop, history, &Db::open_default).await;
        }
        Err(e) => sink.emit(Event::Error { text: chain(&e) }),
    }
}

/// The engine. `open_db` opens the cache for each tool call and for the
/// grounding message, so no handle lives across an await.
pub async fn run_turn<T: Transport>(
    transport: &T,
    sink: &dyn EventSink,
    stop: &AtomicBool,
    history: Vec<HistoryMessage>,
    open_db: &(dyn Fn() -> Result<Db> + Sync),
) -> Result<Vec<String>> {
    match rounds(transport, sink, stop, history, open_db).await {
        Ok(sources) => {
            sink.emit(Event::Done {
                sources: sources.clone(),
            });
            Ok(sources)
        }
        Err(e) => {
            let text = chain(&e);
            sink.emit(Event::Error { text: text.clone() });
            Err(anyhow!(text))
        }
    }
}

fn chain(e: &anyhow::Error) -> String {
    e.chain()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}

async fn rounds<T: Transport>(
    transport: &T,
    sink: &dyn EventSink,
    stop: &AtomicBool,
    history: Vec<HistoryMessage>,
    open_db: &(dyn Fn() -> Result<Db> + Sync),
) -> Result<Vec<String>> {
    let mut messages = vec![
        system_message(transport.caches_prompt()),
        grounding_message(&open_db()?)?,
    ];
    messages.extend(
        trim_history(history)
            .into_iter()
            .map(|m| json!({ "role": m.role, "content": m.content })),
    );

    let schemas = tools::schemas();
    let mut sources = Vec::new();
    let mut said_anything = false;

    for round in 0..=MAX_TOOL_ROUNDS {
        if stop.load(Ordering::Relaxed) {
            return Ok(sources);
        }
        // The last round forbids tools, so the model must answer with what it
        // has. The schemas stay in the request: the messages hold tool calls.
        let last = round == MAX_TOOL_ROUNDS;
        let choice = last.then_some("none");
        let stream = transport
            .complete(&messages, Some(&schemas), choice, sink, stop)
            .await?;
        said_anything |= !stream.content.trim().is_empty();

        if stop.load(Ordering::Relaxed) {
            return Ok(sources);
        }
        if last || stream.tool_calls.is_empty() {
            return answered(said_anything, sources);
        }

        // The tool-call message goes back before the results, or the API
        // rejects the orphaned tool responses.
        messages.push(json!({
            "role": "assistant",
            "content": stream.content,
            "tool_calls": stream.tool_calls.iter().map(|c| json!({
                "id": c.id,
                "type": "function",
                "function": { "name": c.name, "arguments": c.arguments },
            })).collect::<Vec<_>>(),
        }));
        for call in &stream.tool_calls {
            let args: Value = serde_json::from_str(&call.arguments).unwrap_or(json!({}));
            let label = tools::describe(&call.name, &args);
            sink.emit(tool_event(&call.id, &label, true, true));
            sources.push(label.clone());
            let result = tools::run(&call.name, &args, open_db);
            let ok = result.get("error").is_none();
            sink.emit(tool_event(&call.id, &label, false, ok));
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call.id,
                "name": call.name,
                "content": result.to_string(),
            }));
            if stop.load(Ordering::Relaxed) {
                return Ok(sources);
            }
        }
    }
    answered(said_anything, sources)
}

/// A reasoning model can spend its whole budget thinking and return nothing.
/// Say so: a blank reply looks like a broken app.
fn answered(said_anything: bool, sources: Vec<String>) -> Result<Vec<String>> {
    if said_anything {
        return Ok(sources);
    }
    Err(anyhow!(
        "The model finished without writing an answer. Ask again, or try a different model."
    ))
}

fn tool_event(call_id: &str, label: &str, running: bool, ok: bool) -> Event {
    Event::Tool {
        call_id: call_id.into(),
        label: label.into(),
        running,
        ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sse::{SseParser, StreamResult};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Rec(Mutex<Vec<Event>>);
    impl EventSink for Rec {
        fn emit(&self, e: Event) {
            self.0.lock().unwrap().push(e);
        }
    }
    impl Rec {
        fn events(&self) -> Vec<Event> {
            self.0.lock().unwrap().clone()
        }
    }

    /// A fake provider. Each reply is raw SSE text, cut into small pieces so
    /// the parser sees partial events. It records the requests it received.
    struct Mock {
        replies: Mutex<Vec<String>>,
        /// Per request: message count, tools sent, tool choice.
        seen: Mutex<Vec<(usize, bool, Option<String>)>>,
    }
    impl Mock {
        fn new(replies: &[&str]) -> Self {
            Self {
                replies: Mutex::new(replies.iter().rev().map(|s| s.to_string()).collect()),
                seen: Mutex::default(),
            }
        }
    }
    impl Transport for Mock {
        async fn complete(
            &self,
            messages: &[Value],
            tools: Option<&Value>,
            tool_choice: Option<&str>,
            sink: &dyn EventSink,
            _stop: &AtomicBool,
        ) -> Result<StreamResult> {
            self.seen.lock().unwrap().push((
                messages.len(),
                tools.is_some(),
                tool_choice.map(String::from),
            ));
            let reply = self.replies.lock().unwrap().pop().expect("a reply is left");
            let mut parser = SseParser::default();
            for piece in reply.as_bytes().chunks(7) {
                parser.feed(piece, sink)?;
            }
            Ok(parser.finish())
        }
    }

    fn text(s: &str) -> String {
        format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":{"content":s}}]})
        )
    }

    fn call(name: &str, args: &str) -> String {
        let d = json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"c1","function":{"name":name,"arguments":args}}]}}]});
        format!("data: {d}\n\ndata: [DONE]\n\n")
    }

    fn empty_db() -> Result<Db> {
        Db::open(std::path::Path::new(":memory:"))
    }

    fn user(s: &str) -> Vec<HistoryMessage> {
        vec![HistoryMessage {
            role: "user".into(),
            content: s.into(),
        }]
    }

    #[tokio::test]
    async fn a_tool_round_then_an_answer() {
        let mock = Mock::new(&[
            &call("recovery", "{\"days\":7}"),
            &text("Your sleep is fine."),
        ]);
        let sink = Rec::default();
        let stop = AtomicBool::new(false);
        let sources = run_turn(&mock, &sink, &stop, user("How am I?"), &empty_db)
            .await
            .unwrap();
        assert_eq!(sources, vec!["Reading 7 days of recovery signals"]);
        let events = sink.events();
        assert!(matches!(&events[0], Event::Tool { running: true, .. }));
        assert!(matches!(
            &events[1],
            Event::Tool {
                running: false,
                ok: true,
                ..
            }
        ));
        assert_eq!(events.last(), Some(&Event::Done { sources }));
        let deltas: String = events
            .iter()
            .filter_map(|e| match e {
                Event::Delta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, "Your sleep is fine.");
        // Two requests. The second carries the assistant call and the result.
        let seen = mock.seen.lock().unwrap().clone();
        assert_eq!(seen, vec![(3, true, None), (5, true, None)]);
    }

    #[tokio::test]
    async fn an_unknown_tool_is_a_failed_step_and_not_a_failed_turn() {
        let mock = Mock::new(&[&call("nope", "{}"), &text("Sorry.")]);
        let sink = Rec::default();
        run_turn(&mock, &sink, &AtomicBool::new(false), user("hi"), &empty_db)
            .await
            .unwrap();
        assert!(sink.events().iter().any(|e| matches!(
            e,
            Event::Tool {
                running: false,
                ok: false,
                ..
            }
        )));
    }

    #[tokio::test]
    async fn the_last_round_forbids_tools_but_still_sends_them() {
        let calls: Vec<String> = (0..MAX_TOOL_ROUNDS)
            .map(|_| call("cache_status", "{}"))
            .collect();
        let mut replies: Vec<&str> = calls.iter().map(String::as_str).collect();
        let answer = text("Done.");
        replies.push(&answer);
        let mock = Mock::new(&replies);
        let sink = Rec::default();
        run_turn(&mock, &sink, &AtomicBool::new(false), user("hi"), &empty_db)
            .await
            .unwrap();
        let seen = mock.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), MAX_TOOL_ROUNDS + 1);
        // The earlier requests leave the choice open.
        assert!(seen[..MAX_TOOL_ROUNDS].iter().all(|s| s.2.is_none()));
        // The messages hold tool calls, so the schemas must stay.
        assert!(seen.last().unwrap().1);
        assert_eq!(seen.last().unwrap().2.as_deref(), Some("none"));
    }

    #[tokio::test]
    async fn an_empty_final_round_is_an_error_too() {
        let calls: Vec<String> = (0..MAX_TOOL_ROUNDS)
            .map(|_| call("cache_status", "{}"))
            .collect();
        let mut replies: Vec<&str> = calls.iter().map(String::as_str).collect();
        // The model ignores the choice and asks for a tool again.
        let again = call("cache_status", "{}");
        replies.push(&again);
        let mock = Mock::new(&replies);
        let err = run_turn(
            &mock,
            &Rec::default(),
            &AtomicBool::new(false),
            user("hi"),
            &empty_db,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("without writing an answer"));
    }

    #[tokio::test]
    async fn an_empty_answer_is_an_error_the_person_can_read() {
        let mock = Mock::new(&["data: [DONE]\n\n"]);
        let sink = Rec::default();
        let err = run_turn(&mock, &sink, &AtomicBool::new(false), user("hi"), &empty_db)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("without writing an answer"));
        assert!(matches!(sink.events().last(), Some(Event::Error { .. })));
    }

    #[tokio::test]
    async fn stop_ends_the_turn_with_done() {
        let mock = Mock::new(&[]);
        let sink = Rec::default();
        let stop = AtomicBool::new(true);
        run_turn(&mock, &sink, &stop, user("hi"), &empty_db)
            .await
            .unwrap();
        assert_eq!(sink.events(), vec![Event::Done { sources: vec![] }]);
    }

    #[test]
    fn events_serialize_in_camel_case() {
        let json = serde_json::to_value(tool_event("c1", "L", true, false)).unwrap();
        assert_eq!(
            json,
            json!({"type":"tool","callId":"c1","label":"L","running":true,"ok":false})
        );
    }
}
