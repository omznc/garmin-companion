//! One assistant turn: tool rounds until the model stops asking for tools,
//! then the streamed answer.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Result};
use garmin_core::db::Db;
use garmin_core::workout::WorkoutDraft;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::prompt::{grounding_message, system_message, trim_history};
use crate::sse::ToolCall;
use crate::tools::{self, Period};
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
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export, rename = "AskEvent"))]
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
    /// The text streamed so far was not the answer: its round ended with tool
    /// calls. The host moves that text out of the answer and into the trace.
    /// Only the text of the last round is the answer.
    Note {
        text: String,
    },
    /// A workout that the model proposed and the tool accepted. The host shows
    /// it as a card. Nothing is saved to Garmin until the person asks.
    Draft {
        call_id: String,
        draft: WorkoutDraft,
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
/// A `period` bounds what the tools read.
pub async fn ask_in(
    history: Vec<HistoryMessage>,
    sink: &dyn EventSink,
    stop: &AtomicBool,
    drafts: bool,
    period: Option<Period>,
) {
    let transport = Db::open_default().and_then(|db| HttpTransport::from_config(&db));
    match transport {
        Ok(t) => {
            // The turn reports its own end to the sink.
            let _ = run_turn_in(&t, sink, stop, history, &Db::open_default, drafts, period).await;
        }
        Err(e) => sink.emit(Event::Error { text: chain(&e) }),
    }
}

/// The last message gets one more line that names the period, so the model
/// answers from that period. The thread on screen does not show the line.
pub fn with_period_note(
    mut messages: Vec<HistoryMessage>,
    period: Option<Period>,
) -> Vec<HistoryMessage> {
    if let (Some(p), Some(last)) = (period, messages.last_mut()) {
        last.content = format!(
            "{}\n\nFocus period: {} to {} (ISO dates). Use only data from this period unless the user asks for more. \
             The tools read this period: they count days back from its last day.",
            last.content, p.start, p.end
        );
    }
    messages
}

/// The engine. `open_db` opens the cache for each tool call and for the
/// grounding message, so no handle lives across an await. `drafts` is on for
/// a host that shows a workout card: it adds the `draft_workout` tool and its
/// prompt text.
pub async fn run_turn<T: Transport>(
    transport: &T,
    sink: &dyn EventSink,
    stop: &AtomicBool,
    history: Vec<HistoryMessage>,
    open_db: &(dyn Fn() -> Result<Db> + Sync),
    drafts: bool,
) -> Result<Vec<String>> {
    run_turn_in(transport, sink, stop, history, open_db, drafts, None).await
}

/// [`run_turn`] with a `period`. When set, "today" for every tool is the end
/// of the period, and a tool call without `days` reads the whole period.
pub async fn run_turn_in<T: Transport>(
    transport: &T,
    sink: &dyn EventSink,
    stop: &AtomicBool,
    history: Vec<HistoryMessage>,
    open_db: &(dyn Fn() -> Result<Db> + Sync),
    drafts: bool,
    period: Option<Period>,
) -> Result<Vec<String>> {
    match rounds(transport, sink, stop, history, open_db, drafts, period).await {
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
    drafts: bool,
    period: Option<Period>,
) -> Result<Vec<String>> {
    let mut messages = vec![
        system_message(transport.caches_prompt(), drafts),
        grounding_message(&open_db()?, period)?,
    ];
    messages.extend(
        trim_history(history)
            .into_iter()
            .map(|m| json!({ "role": m.role, "content": m.content })),
    );

    let schemas = tools::schemas(drafts);
    let mut sources = Vec::new();
    let mut used_ids = std::collections::HashSet::new();

    for round in 0..=MAX_TOOL_ROUNDS {
        if stop.load(Ordering::Relaxed) {
            return Ok(sources);
        }
        // The last round forbids tools, so the model must answer with what it
        // has. The schemas stay in the request: the messages hold tool calls.
        let last = round == MAX_TOOL_ROUNDS;
        let choice = last.then_some("none");
        let mut stream = transport
            .complete(&messages, Some(&schemas), choice, sink, stop)
            .await?;

        if stop.load(Ordering::Relaxed) {
            return Ok(sources);
        }
        if last || stream.tool_calls.is_empty() {
            return answered(&stream.content, sources);
        }

        make_ids_unique(&mut stream.tool_calls, &mut used_ids);

        // This text was narration, not the answer. Say so before the tools run.
        // Whitespace sends an empty note: the host clears it from the answer.
        if !stream.content.is_empty() {
            sink.emit(Event::Note {
                text: stream.content.trim().into(),
            });
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
            let result = run_call(&call.name, &args, drafts, open_db, period);
            let ok = result.get("error").is_none();
            sink.emit(tool_event(&call.id, &label, false, ok));
            report_call(sink, &mut sources, call, &args, label, ok);
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
    Err(no_answer())
}

/// Give each call an id that is unique in the turn. A provider can leave the id
/// out or reuse it in a later round. The app finds a draft by this id.
fn make_ids_unique(calls: &mut [ToolCall], used: &mut std::collections::HashSet<String>) {
    for call in calls {
        let base = call.id.clone();
        let mut n = 1;
        while !used.insert(call.id.clone()) {
            call.id = format!("{base}-{n}");
            n += 1;
        }
    }
}

/// Run one tool call. A host with drafts off does not know `draft_workout`,
/// even if the model calls it.
fn run_call(
    name: &str,
    args: &Value,
    drafts: bool,
    open_db: &(dyn Fn() -> Result<Db> + Sync),
    period: Option<Period>,
) -> Value {
    if name == tools::DRAFT_TOOL && !drafts {
        return json!({ "error": format!("unknown tool {name}") });
    }
    tools::run(name, args, open_db, period)
}

/// Tell the host what a finished call produced. A read adds its label to the
/// sources. A draft is not a read: it becomes a `Draft` event, built with the
/// same parse that the tool used, so the host gets what the tool accepted.
fn report_call(
    sink: &dyn EventSink,
    sources: &mut Vec<String>,
    call: &ToolCall,
    args: &Value,
    label: String,
    ok: bool,
) {
    if call.name != tools::DRAFT_TOOL {
        sources.push(label);
        return;
    }
    if let Some(draft) = ok
        .then(|| garmin_core::workout::parse_draft(args).ok())
        .flatten()
    {
        sink.emit(Event::Draft {
            call_id: call.id.clone(),
            draft,
        });
    }
}

/// A reasoning model can spend its whole budget thinking and return nothing.
/// The earlier rounds only held narration, so a blank last round has no answer.
/// Say so: a blank reply looks like a broken app.
fn answered(last_round: &str, sources: Vec<String>) -> Result<Vec<String>> {
    if last_round.trim().is_empty() {
        return Err(no_answer());
    }
    Ok(sources)
}

fn no_answer() -> anyhow::Error {
    anyhow!("The model finished without writing an answer. Ask again, or try a different model.")
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

    #[test]
    fn a_period_adds_its_dates_to_the_last_message_only() {
        let day = |d| chrono::NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
        let msg = |t: &str| HistoryMessage {
            role: "user".into(),
            content: t.into(),
        };
        let period = Some(Period {
            start: day(14),
            end: day(20),
        });
        let out = with_period_note(vec![msg("a"), msg("b")], period);
        assert_eq!(out[0].content, "a");
        assert!(out[1]
            .content
            .contains("Focus period: 2026-10-14 to 2026-10-20 (ISO dates)."));
        assert_eq!(with_period_note(vec![msg("a")], None)[0].content, "a");
    }

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

    /// Narration and a tool call in one round.
    fn narrated_call(say: &str, name: &str) -> String {
        let d = json!({"choices":[{"delta":{"content":say,"tool_calls":[
            {"index":0,"id":"c1","function":{"name":name,"arguments":"{}"}}]}}]});
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
        let sources = run_turn(&mock, &sink, &stop, user("How am I?"), &empty_db, false)
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
    async fn narration_becomes_a_note_before_the_tools_of_its_round() {
        let mock = Mock::new(&[
            &narrated_call(" Let me look. ", "cache_status"),
            &text("Rest today."),
        ]);
        let sink = Rec::default();
        run_turn(
            &mock,
            &sink,
            &AtomicBool::new(false),
            user("hi"),
            &empty_db,
            false,
        )
        .await
        .unwrap();
        let events = sink.events();
        let note = events
            .iter()
            .position(|e| matches!(e, Event::Note { text } if text == "Let me look."));
        let tool = events.iter().position(|e| matches!(e, Event::Tool { .. }));
        assert!(note.unwrap() < tool.unwrap());
        // The final round has no note. Its text is the answer.
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::Note { .. }))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn whitespace_narration_still_sends_a_note_to_clear_it() {
        let mock = Mock::new(&[&narrated_call("\n\n", "cache_status"), &text("Rest.")]);
        let sink = Rec::default();
        run_turn(
            &mock,
            &sink,
            &AtomicBool::new(false),
            user("hi"),
            &empty_db,
            false,
        )
        .await
        .unwrap();
        assert!(sink.events().contains(&Event::Note {
            text: String::new()
        }));
    }

    #[tokio::test]
    async fn narration_alone_is_not_an_answer() {
        let calls: Vec<String> = (0..=MAX_TOOL_ROUNDS)
            .map(|_| narrated_call("Looking.", "cache_status"))
            .collect();
        // The last round says text and asks for a tool: its text is the answer.
        let replies: Vec<&str> = calls.iter().map(String::as_str).collect();
        let mock = Mock::new(&replies);
        let ok = run_turn(
            &mock,
            &Rec::default(),
            &AtomicBool::new(false),
            user("hi"),
            &empty_db,
            false,
        )
        .await;
        assert!(ok.is_ok());
        let mut again: Vec<String> = (0..MAX_TOOL_ROUNDS)
            .map(|_| narrated_call("Looking.", "cache_status"))
            .collect();
        again.push(call("cache_status", "{}"));
        let replies: Vec<&str> = again.iter().map(String::as_str).collect();
        let mock = Mock::new(&replies);
        let err = run_turn(
            &mock,
            &Rec::default(),
            &AtomicBool::new(false),
            user("hi"),
            &empty_db,
            false,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("without writing an answer"));
    }

    #[tokio::test]
    async fn an_unknown_tool_is_a_failed_step_and_not_a_failed_turn() {
        let mock = Mock::new(&[&call("nope", "{}"), &text("Sorry.")]);
        let sink = Rec::default();
        run_turn(
            &mock,
            &sink,
            &AtomicBool::new(false),
            user("hi"),
            &empty_db,
            false,
        )
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
        run_turn(
            &mock,
            &sink,
            &AtomicBool::new(false),
            user("hi"),
            &empty_db,
            false,
        )
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
            false,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("without writing an answer"));
    }

    #[tokio::test]
    async fn an_empty_answer_is_an_error_the_person_can_read() {
        let mock = Mock::new(&["data: [DONE]\n\n"]);
        let sink = Rec::default();
        let err = run_turn(
            &mock,
            &sink,
            &AtomicBool::new(false),
            user("hi"),
            &empty_db,
            false,
        )
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
        run_turn(&mock, &sink, &stop, user("hi"), &empty_db, false)
            .await
            .unwrap();
        assert_eq!(sink.events(), vec![Event::Done { sources: vec![] }]);
    }

    fn good_draft_args() -> String {
        json!({
            "name": "Easy 30", "sport": "running",
            "steps": [{ "kind": "interval", "end": { "seconds": 1800 } }]
        })
        .to_string()
    }

    #[tokio::test]
    async fn a_draft_call_emits_a_draft_and_is_not_a_source() {
        let args = good_draft_args();
        let mock = Mock::new(&[&call("draft_workout", &args), &text("Here it is.")]);
        let sink = Rec::default();
        let sources = run_turn(
            &mock,
            &sink,
            &AtomicBool::new(false),
            user("plan"),
            &empty_db,
            true,
        )
        .await
        .unwrap();
        assert!(sources.is_empty());
        let drafts: Vec<_> = sink
            .events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Draft { call_id, draft } => Some((call_id, draft)),
                _ => None,
            })
            .collect();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].0, "c1");
        assert_eq!(drafts[0].1.name, "Easy 30");
        let json = serde_json::to_value(Event::Draft {
            call_id: "c1".into(),
            draft: drafts[0].1.clone(),
        })
        .unwrap();
        assert_eq!(json["type"], "draft");
        assert_eq!(json["callId"], "c1");
        assert_eq!(json["draft"]["name"], "Easy 30");
    }

    #[tokio::test]
    async fn two_drafts_in_one_turn_get_distinct_ids() {
        let args = good_draft_args();
        let mock = Mock::new(&[
            &call("draft_workout", &args),
            &call("draft_workout", &args),
            &text("Here."),
        ]);
        let sink = Rec::default();
        let stop = AtomicBool::new(false);
        run_turn(&mock, &sink, &stop, user("plan"), &empty_db, true)
            .await
            .unwrap();
        let ids: Vec<_> = sink
            .events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Draft { call_id, .. } => Some(call_id),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["c1", "c1-1"]);
    }

    #[tokio::test]
    async fn an_invalid_draft_is_an_error_result_and_no_draft_event() {
        let bad = r#"{"name":"Long","sport":"running","steps":[{"kind":"interval","end":{"seconds":90000}}]}"#;
        let mock = Mock::new(&[&call("draft_workout", bad), &text("Sorry.")]);
        let sink = Rec::default();
        run_turn(
            &mock,
            &sink,
            &AtomicBool::new(false),
            user("plan"),
            &empty_db,
            true,
        )
        .await
        .unwrap();
        let events = sink.events();
        assert!(!events.iter().any(|e| matches!(e, Event::Draft { .. })));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Tool {
                running: false,
                ok: false,
                ..
            }
        )));
    }

    #[tokio::test]
    async fn drafts_off_hides_the_tool_and_rejects_a_call_to_it() {
        let args = good_draft_args();
        let mock = Mock::new(&[&call("draft_workout", &args), &text("Sorry.")]);
        let sink = Rec::default();
        run_turn(
            &mock,
            &sink,
            &AtomicBool::new(false),
            user("plan"),
            &empty_db,
            false,
        )
        .await
        .unwrap();
        assert!(!sink
            .events()
            .iter()
            .any(|e| matches!(e, Event::Draft { .. })));
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
