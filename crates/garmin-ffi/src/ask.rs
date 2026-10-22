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

use crate::chat::{self, DraftEntry, Status, Step, StepKind, Turn};
use crate::FfiError;

/// The turn that runs now: its session, its stop flag and its live state.
static RUNNING: Mutex<Option<Arc<SinkState>>> = Mutex::new(None);

/// The `ask` payload: the event JSON, the session and the turn it belongs to.
fn payload(event: &Event, session_id: &str, turn_id: &str) -> Value {
    let mut value = serde_json::to_value(event).unwrap_or(Value::Null);
    if let Some(map) = value.as_object_mut() {
        map.insert("sessionId".into(), session_id.into());
        map.insert("turnId".into(), turn_id.into());
    }
    value
}

struct SinkState {
    session_id: String,
    /// The id that the app gave to this turn. The ids of two turns never match.
    turn_id: String,
    /// The thread was deleted. The turn may end, but it saves nothing: the same
    /// session id can already hold a new turn.
    discarded: AtomicBool,
    fixture: bool,
    flag: Arc<AtomicBool>,
    turn: Mutex<Turn>,
}

/// Keeps the turn of one thread: it sends each event to the app, applies the
/// event to the turn, and saves the turn when it ends.
#[derive(Clone)]
struct TurnSink(Arc<SinkState>);

impl TurnSink {
    fn apply(&self, event: &Event) -> Option<Turn> {
        let mut turn = self.0.turn.lock().unwrap_or_else(|e| e.into_inner());
        match event {
            Event::Tool {
                call_id,
                label,
                running,
                ok,
            } => {
                let step = Step {
                    kind: StepKind::Tool,
                    call_id: call_id.clone(),
                    label: label.clone(),
                    running: *running,
                    ok: (!running).then_some(*ok),
                };
                match turn.steps.iter_mut().find(|s| s.call_id == *call_id) {
                    Some(slot) => *slot = step,
                    None => turn.steps.push(step),
                }
            }
            Event::Delta { text } => turn.answer.push_str(text),
            Event::Draft { call_id, draft } => turn.drafts.push(DraftEntry {
                call_id: call_id.clone(),
                draft: draft.clone(),
                saved_workout_id: None,
            }),
            Event::Note { text } => apply_note(&mut turn, text),
            Event::Done { .. } => {
                turn.status = if self.0.flag.load(Ordering::Relaxed) {
                    Status::Stopped
                } else {
                    Status::Done
                }
            }
            Event::Error { text } => {
                turn.status = Status::Error;
                turn.error = Some(text.clone());
            }
        }
        // A draft is stored at once, so the app can save it while the turn runs.
        (turn.status != Status::Running || matches!(event, Event::Draft { .. }))
            .then(|| turn.clone())
    }
}

fn apply_note(turn: &mut Turn, text: &str) {
    // The answer holds only the text of the round that just ended.
    turn.answer.clear();
    // A blank note only clears the answer: no empty row in the trace.
    if !text.is_empty() {
        let n = turn.steps.len();
        turn.steps.push(Step {
            kind: StepKind::Note,
            call_id: format!("note-{n}"),
            label: text.to_owned(),
            running: false,
            ok: None,
        });
    }
}

impl EventSink for TurnSink {
    fn emit(&self, event: Event) {
        // The save comes first, so the app finds the turn in the store when it reads after `done`.
        if let Some(turn) = self.apply(&event) {
            if !self.0.discarded.load(Ordering::Relaxed) {
                // A failed save cannot reach the app in a better way than the thread missing the turn.
                let _ = chat::finish_turn(self.0.fixture, &self.0.session_id, &turn);
            }
            if turn.status != Status::Running {
                clear_running(&self.0);
            }
        }
        crate::emit("ask", payload(&event, &self.0.session_id, &self.0.turn_id));
    }
}

fn clear_running(state: &Arc<SinkState>) {
    let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    if running.as_ref().is_some_and(|s| Arc::ptr_eq(s, state)) {
        *running = None;
    }
}

/// The session of the turn that runs now, if any.
pub(crate) fn running_session() -> Option<String> {
    let running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    running.as_ref().map(|s| s.session_id.clone())
}

/// A draft was saved to Garmin: record the id on the turn in memory too, so
/// the live turn that `chat_get` returns agrees with the store.
pub(crate) fn mark_saved(session_id: &str, turn_id: &str, call_id: &str, workout_id: i64) {
    let Some(state) = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return;
    };
    let mut turn = state.turn.lock().unwrap_or_else(|e| e.into_inner());
    if state.session_id == session_id && turn.id == turn_id {
        for d in turn.drafts.iter_mut().filter(|d| d.call_id == call_id) {
            d.saved_workout_id = Some(workout_id);
        }
    }
}

/// The turn of this session as it is now in memory, with its partial answer.
pub(crate) fn live_turn(session_id: &str) -> Option<Turn> {
    let state = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).clone()?;
    if state.session_id != session_id {
        return None;
    }
    let turn = state.turn.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Some(turn)
}

/// The thread is deleted: stop its turn, and let the turn save nothing.
pub(crate) fn discard_session(session_id: &str) {
    let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    if running.as_ref().is_some_and(|s| s.session_id == session_id) {
        if let Some(state) = running.take() {
            state.discarded.store(true, Ordering::Relaxed);
            state.flag.store(true, Ordering::Relaxed);
        }
    }
}

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
#[serde(rename_all = "camelCase")]
struct Send {
    session_id: String,
    turn_id: String,
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
    if let Some(state) = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).take() {
        state.flag.store(true, Ordering::Relaxed);
    }
}

fn send(args: &Value, fixture: bool) -> Result<String, FfiError> {
    let a: Send = serde_json::from_value(args.clone()).map_err(bad)?;
    if a.session_id.is_empty() {
        return Err(bad("sessionId is empty"));
    }
    if a.turn_id.is_empty() {
        return Err(bad("turnId is empty"));
    }
    let question = a
        .history
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .ok_or_else(|| bad("history has no user message"))?;
    let history: Vec<HistoryMessage> = a
        .history
        .into_iter()
        .map(|m| HistoryMessage {
            role: m.role,
            content: m.content,
        })
        .collect();
    stop();
    let turn = chat::begin_turn(fixture, &a.session_id, &question).map_err(failed)?;
    let flag = Arc::new(AtomicBool::new(false));
    let state = Arc::new(SinkState {
        session_id: a.session_id,
        turn_id: a.turn_id,
        discarded: AtomicBool::new(false),
        fixture,
        flag,
        turn: Mutex::new(turn),
    });
    *RUNNING.lock().unwrap_or_else(|e| e.into_inner()) = Some(state.clone());
    let sink = TurnSink(state);
    let run = sink.clone();
    let task = if fixture {
        tokio::spawn(async move { fixture_turn(&run).await })
    } else {
        tokio::spawn(async move { garmin_coach::ask(history, &run, &run.0.flag, true).await })
    };
    tokio::spawn(report_panic(task, sink));
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
async fn fixture_turn(sink: &TurnSink) {
    let stop = &sink.0.flag;
    let label = "Reading 14 days of recovery signals";
    let note = "I will read your recovery first.";
    for word in note.split_inclusive(' ') {
        sink.emit(Event::Delta { text: word.into() });
    }
    sink.emit(Event::Note { text: note.into() });
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
    if let Ok(draft) = garmin_core::workout::parse_draft(&sample_draft()) {
        sink.emit(Event::Draft {
            call_id: "fixture-draft".into(),
            draft,
        });
    }
    sink.emit(Event::Done {
        sources: vec![label.into()],
    });
}

/// The workout of the sample turn, so the sample mode shows a draft card.
fn sample_draft() -> Value {
    json!({
        "name": "4 x 3 min steady",
        "sport": "running",
        "description": "An easy start, four steady blocks and an easy end.",
        "steps": [
            { "kind": "warmup", "end": { "seconds": 600 }, "target": { "zone": 2 } },
            { "times": 4, "steps": [
                { "kind": "interval", "end": { "seconds": 180 }, "target": { "zone": 3 } },
                { "kind": "recovery", "end": { "seconds": 120 }, "target": { "zone": 1 } }
            ] },
            { "kind": "cooldown", "end": { "seconds": 300 } }
        ]
    })
}

async fn config(fixture: bool) -> Result<String, FfiError> {
    if fixture {
        let saved = SAMPLE_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
        let (provider, model) = saved.clone().unwrap_or(("cloud".into(), String::new()));
        return Ok(
            json!({ "provider": provider, "model": model, "hasKey": false,
            "ollamaReachable": false, "ollamaModels": [] })
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
        // `ask_stop` ends the turn of a parallel test, so it takes the lock.
        let _seq = crate::TEST_LOCK.lock().await;
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

    #[test]
    fn a_blank_note_clears_the_answer_and_stores_no_step() {
        let turn = Turn {
            id: "s:0".into(),
            question: "q".into(),
            answer: String::new(),
            steps: Vec::new(),
            status: Status::Running,
            error: None,
            drafts: Vec::new(),
        };
        let sink = TurnSink(Arc::new(SinkState {
            session_id: "s".into(),
            turn_id: "t".into(),
            discarded: AtomicBool::new(false),
            fixture: true,
            flag: Arc::new(AtomicBool::new(false)),
            turn: Mutex::new(turn),
        }));
        let draft = garmin_core::workout::parse_draft(&sample_draft()).unwrap();
        sink.apply(&Event::Draft {
            call_id: "d1".into(),
            draft: draft.clone(),
        });
        {
            let turn = sink.0.turn.lock().unwrap();
            assert_eq!(turn.drafts.len(), 1);
            assert_eq!(turn.drafts[0].draft, draft);
            assert_eq!(turn.drafts[0].saved_workout_id, None);
        }
        sink.apply(&Event::Delta {
            text: "\n\n".into(),
        });
        sink.apply(&Event::Note {
            text: String::new(),
        });
        sink.apply(&Event::Note {
            text: "Real.".into(),
        });
        let turn = sink.0.turn.lock().unwrap();
        assert_eq!(turn.answer, "");
        assert_eq!(turn.steps.len(), 1);
        assert_eq!(turn.steps[0].label, "Real.");
    }

    fn running_sink(session: &str) -> TurnSink {
        let turn = Turn {
            id: format!("{session}:0"),
            question: "q".into(),
            answer: String::new(),
            steps: Vec::new(),
            status: Status::Running,
            error: None,
            drafts: Vec::new(),
        };
        TurnSink(Arc::new(SinkState {
            session_id: session.into(),
            turn_id: format!("{session}:0"),
            discarded: AtomicBool::new(false),
            fixture: true,
            flag: Arc::new(AtomicBool::new(false)),
            turn: Mutex::new(turn),
        }))
    }

    fn draft_event(call_id: &str) -> Event {
        Event::Draft {
            call_id: call_id.into(),
            draft: garmin_core::workout::parse_draft(&sample_draft()).unwrap(),
        }
    }

    #[test]
    fn a_draft_is_stored_at_once_while_the_turn_runs() {
        let sink = running_sink("s-draft");
        assert!(sink.apply(&Event::Delta { text: "x".into() }).is_none());
        let stored = sink.apply(&draft_event("d1")).expect("draft is stored");
        assert_eq!(stored.drafts.len(), 1);
    }

    #[tokio::test]
    async fn a_save_marks_only_its_draft_in_the_live_turn() {
        let _seq = crate::TEST_LOCK.lock().await;
        let sink = running_sink("s-mark");
        sink.apply(&draft_event("c1"));
        sink.apply(&draft_event("c1-1"));
        *RUNNING.lock().unwrap() = Some(sink.0.clone());
        mark_saved("s-mark", "s-mark:0", "c1-1", 42);
        let live = live_turn("s-mark").unwrap();
        *RUNNING.lock().unwrap() = None;
        let saved: Vec<_> = live.drafts.iter().map(|d| d.saved_workout_id).collect();
        assert_eq!(saved, vec![None, Some(42)]);
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

    fn thread(id: &str) -> Value {
        let out = chat::handle("chat_get", &json!({ "sessionId": id }), true)
            .unwrap()
            .unwrap();
        serde_json::from_str(&out).unwrap()
    }

    async fn send_to(id: &str, text: &str) {
        let args = json!({ "sessionId": id, "turnId": format!("{id}-{text}"),
            "history": [{ "role": "user", "content": text }] });
        handle("ask_send", &args, true).await.unwrap().unwrap();
    }

    async fn wait_idle(id: &str) {
        for _ in 0..100 {
            if thread(id)["running"] == false {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the turn did not end");
    }

    #[tokio::test]
    async fn a_send_saves_the_turn_and_finishes_it() {
        let _seq = crate::TEST_LOCK.lock().await;
        send_to("t-send", "How did I sleep?").await;
        assert_eq!(thread("t-send")["turns"][0]["status"], "running");
        wait_idle("t-send").await;
        let t = thread("t-send");
        assert_eq!(t["title"], "How did I sleep?");
        assert_eq!(t["turns"][0]["id"], "t-send:0");
        assert_eq!(t["turns"][0]["status"], "done");
        // The narration moved to the trace: the stored answer is the final text only.
        let answer = t["turns"][0]["answer"].as_str().unwrap();
        assert!(answer.starts_with("This is a sample answer"));
        assert!(!answer.contains("I will read your recovery"));
        // The first step is the note of the fixture, the second is the tool.
        assert_eq!(t["turns"][0]["steps"][0]["kind"], "note");
        assert_eq!(t["turns"][0]["steps"][1]["ok"], true);
        send_to("t-send", "And last week?").await;
        wait_idle("t-send").await;
        assert_eq!(thread("t-send")["turns"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_rename_during_a_run_survives_the_finish_and_a_later_turn() {
        let _seq = crate::TEST_LOCK.lock().await;
        send_to("t-name", "First question").await;
        let args = json!({ "sessionId": "t-name", "title": "Mine" });
        chat::handle("chat_rename", &args, true).unwrap().unwrap();
        wait_idle("t-name").await;
        send_to("t-name", "Second question").await;
        wait_idle("t-name").await;
        assert_eq!(thread("t-name")["title"], "Mine");
    }

    #[tokio::test]
    async fn a_delete_during_a_run_stops_it_and_keeps_it_deleted() {
        let _seq = crate::TEST_LOCK.lock().await;
        send_to("t-del", "Question").await;
        assert_eq!(running_session().as_deref(), Some("t-del"));
        let args = json!({ "sessionId": "t-del" });
        chat::handle("chat_delete", &args, true).unwrap().unwrap();
        assert!(running_session().is_none());
        tokio::time::sleep(Duration::from_millis(900)).await;
        assert!(thread("t-del").is_null());
    }

    #[tokio::test]
    async fn a_late_end_of_a_deleted_turn_leaves_the_new_turn_alone() {
        let _seq = crate::TEST_LOCK.lock().await;
        send_to("t-reuse", "Q1").await;
        let args = json!({ "sessionId": "t-reuse" });
        chat::handle("chat_delete", &args, true).unwrap().unwrap();
        send_to("t-reuse", "Q2").await;
        // The old turn ends at about 400 ms, while the new turn still runs.
        tokio::time::sleep(Duration::from_millis(700)).await;
        // `running` keeps the last turn as stored, so this reads the stored status.
        let saved = garmin_api::chat::get_thread(garmin_api::Source::Sample, "t-reuse", true, None)
            .unwrap()
            .unwrap();
        let turns = saved.turns;
        assert_eq!(turns.len(), 1);
        assert_eq!(
            (turns[0].question.as_str(), turns[0].status),
            ("Q2", Status::Running)
        );
        wait_idle("t-reuse").await;
        assert_eq!(thread("t-reuse")["turns"][0]["status"], "done");
    }

    #[test]
    fn every_event_carries_its_turn_id() {
        let value = payload(&Event::Delta { text: "x".into() }, "s", "s-1");
        assert_eq!(
            (value["sessionId"].as_str(), value["turnId"].as_str()),
            (Some("s"), Some("s-1"))
        );
    }

    #[tokio::test]
    async fn a_send_to_another_thread_stops_the_old_turn_under_its_id() {
        let _seq = crate::TEST_LOCK.lock().await;
        send_to("t-old", "Old").await;
        send_to("t-new", "New").await;
        wait_idle("t-new").await;
        let old = thread("t-old");
        assert_eq!(old["turns"][0]["status"], "stopped");
        assert_eq!(thread("t-new")["turns"][0]["status"], "done");
    }

    #[tokio::test]
    async fn a_thread_that_opens_during_a_run_shows_the_partial_answer() {
        let _seq = crate::TEST_LOCK.lock().await;
        send_to("t-live", "Question").await;
        tokio::time::sleep(Duration::from_millis(700)).await;
        let t = thread("t-live");
        assert_eq!(t["running"], true);
        let turn = &t["turns"][0];
        assert_eq!(turn["status"], "running");
        assert!(!turn["answer"].as_str().unwrap().is_empty());
        assert_eq!(turn["steps"].as_array().unwrap().len(), 2);
        wait_idle("t-live").await;
    }
}
