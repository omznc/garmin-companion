//! The saved threads of the Ask screen: `chat_list`, `chat_get`,
//! `chat_rename` and `chat_delete`.
//!
//! A thread is one `chat_sessions` row. Its `messages` column holds
//! `{"v":1,"turns":[...]}`. The threads stay on the device: the cloud worker
//! never sees them. In sample mode the rows stay in memory, because a real
//! save would create the cache and end sample mode.

use std::sync::{Mutex, MutexGuard};

use chrono::SecondsFormat;
use garmin_core::db::{ChatSession, ChatSessionMeta, Db};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::ask::{bad, failed, live_turn, running_session, stop_session};
use crate::FfiError;

const SAMPLE_THREADS: &str = include_str!("../../garmin-api/fixtures/chat.sample.json");
const TITLE_MAX: usize = 80;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Step {
    pub call_id: String,
    pub label: String,
    pub running: bool,
    /// `None` while the call runs.
    pub ok: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Status {
    Running,
    Done,
    Error,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Turn {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub steps: Vec<Step>,
    pub status: Status,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    v: u32,
    turns: Vec<Turn>,
}

pub(crate) fn encode(turns: &[Turn]) -> String {
    serde_json::to_string(&json!({ "v": 1, "turns": turns })).unwrap_or_default()
}

/// A body that does not parse reads as an empty thread, so the list still opens.
pub(crate) fn decode(messages: &str) -> Vec<Turn> {
    serde_json::from_str::<Envelope>(messages).map_or_else(|_| Vec::new(), |e| e.turns)
}

/// The first question, cut on a word boundary to fit `TITLE_MAX` characters.
pub(crate) fn make_title(question: &str) -> String {
    let q = question.trim();
    if q.chars().count() <= TITLE_MAX {
        return q.to_owned();
    }
    let cut: String = q.chars().take(TITLE_MAX - 1).collect();
    let words = match cut.rfind(char::is_whitespace) {
        Some(i) if i > 0 => &cut[..i],
        _ => &cut,
    };
    format!("{}…", words.trim_end())
}

/// Where the rows live: the cache, or the memory of sample mode.
pub(crate) trait Store {
    fn get(&self, id: &str) -> Result<Option<ChatSession>, String>;
    fn put(&self, row: &ChatSession) -> Result<(), String>;
    fn list(&self, limit: u32, offset: u32) -> Result<Vec<ChatSessionMeta>, String>;
    /// Change only the turns of a thread that exists. A missing thread stays missing.
    fn update_turns(&self, id: &str, turns: &[Turn]) -> Result<(), String>;
    fn rename(&self, id: &str, title: &str) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}

impl Store for Db {
    fn get(&self, id: &str) -> Result<Option<ChatSession>, String> {
        self.chat_session(id).map_err(|e| e.to_string())
    }
    fn put(&self, row: &ChatSession) -> Result<(), String> {
        self.save_chat_session(row).map_err(|e| e.to_string())
    }
    fn list(&self, limit: u32, offset: u32) -> Result<Vec<ChatSessionMeta>, String> {
        self.chat_sessions(limit, offset).map_err(|e| e.to_string())
    }
    fn update_turns(&self, id: &str, turns: &[Turn]) -> Result<(), String> {
        self.update_chat_messages(id, &now(), turns.len() as i64, &encode(turns))
            .map_err(|e| e.to_string())
    }
    fn rename(&self, id: &str, title: &str) -> Result<(), String> {
        self.rename_chat_session(id, title)
            .map_err(|e| e.to_string())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        self.delete_chat_session(id).map_err(|e| e.to_string())
    }
}

static SAMPLE: Mutex<Option<Vec<ChatSession>>> = Mutex::new(None);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedThread {
    session_id: String,
    title: String,
    updated_at: String,
    turns: Vec<Turn>,
}

fn seed() -> Vec<ChatSession> {
    let threads: Vec<SeedThread> = serde_json::from_str(SAMPLE_THREADS).unwrap_or_default();
    threads
        .into_iter()
        .map(|t| ChatSession {
            session_id: t.session_id,
            started_at: t.updated_at.clone(),
            updated_at: t.updated_at,
            title: t.title,
            message_count: t.turns.len() as i64,
            messages: encode(&t.turns),
        })
        .collect()
}

fn with_sample<T>(f: impl FnOnce(&mut Vec<ChatSession>) -> T) -> T {
    let mut rows: MutexGuard<Option<Vec<ChatSession>>> =
        SAMPLE.lock().unwrap_or_else(|e| e.into_inner());
    f(rows.get_or_insert_with(seed))
}

struct Sample;

impl Store for Sample {
    fn get(&self, id: &str) -> Result<Option<ChatSession>, String> {
        Ok(with_sample(|r| {
            r.iter().find(|s| s.session_id == id).cloned()
        }))
    }
    fn put(&self, row: &ChatSession) -> Result<(), String> {
        with_sample(
            |r| match r.iter_mut().find(|s| s.session_id == row.session_id) {
                Some(slot) => *slot = row.clone(),
                None => r.push(row.clone()),
            },
        );
        Ok(())
    }
    fn list(&self, limit: u32, offset: u32) -> Result<Vec<ChatSessionMeta>, String> {
        Ok(with_sample(|r| {
            let mut rows = r.clone();
            rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            rows.into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .map(|s| ChatSessionMeta {
                    session_id: s.session_id,
                    started_at: s.started_at,
                    updated_at: s.updated_at,
                    title: s.title,
                    message_count: s.message_count,
                })
                .collect()
        }))
    }
    fn update_turns(&self, id: &str, turns: &[Turn]) -> Result<(), String> {
        with_sample(|r| {
            if let Some(s) = r.iter_mut().find(|s| s.session_id == id) {
                s.updated_at = now();
                s.message_count = turns.len() as i64;
                s.messages = encode(turns);
            }
        });
        Ok(())
    }
    fn rename(&self, id: &str, title: &str) -> Result<(), String> {
        with_sample(|r| {
            if let Some(s) = r.iter_mut().find(|s| s.session_id == id) {
                s.title = title.to_owned();
            }
        });
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        with_sample(|r| r.retain(|s| s.session_id != id));
        Ok(())
    }
}

/// One read-modify-write at a time, so two turns never lose each other's turn.
static STORE_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn with_store<T>(
    fixture: bool,
    f: impl FnOnce(&dyn Store) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if fixture {
        return f(&Sample);
    }
    let db = garmin_api::open_default_db().map_err(|e| e.to_string())?;
    f(&db)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Add a running turn to the thread. Only a new thread is inserted, with the
/// title from the first question, so a rename stays.
pub(crate) fn begin_turn(fixture: bool, session_id: &str, question: &str) -> Result<Turn, String> {
    with_store(fixture, |store| {
        let existing = store.get(session_id)?;
        let mut turns = existing
            .as_ref()
            .map_or_else(Vec::new, |r| decode(&r.messages));
        let turn = Turn {
            id: format!("{session_id}:{}", turns.len()),
            question: question.to_owned(),
            answer: String::new(),
            steps: Vec::new(),
            status: Status::Running,
            error: None,
        };
        turns.push(turn.clone());
        if existing.is_some() {
            store.update_turns(session_id, &turns)?;
        } else {
            store.put(&ChatSession {
                session_id: session_id.to_owned(),
                started_at: now(),
                updated_at: now(),
                title: make_title(question),
                message_count: turns.len() as i64,
                messages: encode(&turns),
            })?;
        }
        Ok(turn)
    })
}

/// Store the final state of a turn. A thread or turn that is gone stays gone.
pub(crate) fn finish_turn(fixture: bool, session_id: &str, turn: &Turn) -> Result<(), String> {
    with_store(fixture, |store| {
        let Some(row) = store.get(session_id)? else {
            return Ok(());
        };
        let mut turns = decode(&row.messages);
        let Some(slot) = turns.iter_mut().find(|t| t.id == turn.id) else {
            return Ok(());
        };
        *slot = turn.clone();
        store.update_turns(session_id, &turns)
    })
}

/// A turn that says "running" while its session is not live died with the app.
fn settle(mut turns: Vec<Turn>, live: bool) -> Vec<Turn> {
    let last = turns.len().saturating_sub(1);
    for (i, t) in turns.iter_mut().enumerate() {
        if t.status == Status::Running && !(live && i == last) {
            t.status = Status::Stopped;
        }
    }
    turns
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct List {
    limit: Option<u32>,
    offset: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rename {
    session_id: String,
    title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Id {
    session_id: String,
}

/// Handle a `chat_*` command, or return `None` for any other command.
pub(crate) fn handle(cmd: &str, args: &Value, fixture: bool) -> Option<Result<String, FfiError>> {
    Some(match cmd {
        "chat_list" => list(args, fixture),
        "chat_get" => get(args, fixture),
        "chat_rename" => rename(args, fixture),
        "chat_delete" => delete(args, fixture),
        _ => return None,
    })
}

fn parse<T: serde::de::DeserializeOwned>(args: &Value) -> Result<T, FfiError> {
    serde_json::from_value(args.clone()).map_err(bad)
}

fn list(args: &Value, fixture: bool) -> Result<String, FfiError> {
    let a: List = parse(args)?;
    let rows = with_store(fixture, |s| {
        s.list(a.limit.unwrap_or(50), a.offset.unwrap_or(0))
    })
    .map_err(failed)?;
    let live = running_session();
    let threads: Vec<Value> = rows
        .into_iter()
        .map(|m| {
            json!({
                "running": live.as_deref() == Some(m.session_id.as_str()),
                "sessionId": m.session_id,
                "title": m.title,
                "updatedAt": m.updated_at,
                "turnCount": m.message_count,
            })
        })
        .collect();
    Ok(json!({ "threads": threads }).to_string())
}

fn get(args: &Value, fixture: bool) -> Result<String, FfiError> {
    let a: Id = parse(args)?;
    let row = with_store(fixture, |s| s.get(&a.session_id)).map_err(failed)?;
    let Some(row) = row else {
        return Ok("null".into());
    };
    let running = running_session().as_deref() == Some(a.session_id.as_str());
    let mut turns = decode(&row.messages);
    if let Some(live) = live_turn(&a.session_id) {
        match turns.iter_mut().find(|t| t.id == live.id) {
            Some(slot) => *slot = live,
            None => turns.push(live),
        }
    }
    let turns = settle(turns, running);
    Ok(json!({
        "sessionId": row.session_id,
        "title": row.title,
        "updatedAt": row.updated_at,
        "running": running,
        "turns": turns,
    })
    .to_string())
}

fn rename(args: &Value, fixture: bool) -> Result<String, FfiError> {
    let a: Rename = parse(args)?;
    let title = a.title.trim();
    if title.is_empty() {
        return Err(bad("title is empty"));
    }
    with_store(fixture, |s| s.rename(&a.session_id, title)).map_err(failed)?;
    Ok("{}".into())
}

fn delete(args: &Value, fixture: bool) -> Result<String, FfiError> {
    let a: Id = parse(args)?;
    stop_session(&a.session_id);
    with_store(fixture, |s| s.delete(&a.session_id)).map_err(failed)?;
    Ok("{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(status: Status) -> Turn {
        Turn {
            id: "s:0".into(),
            question: "q".into(),
            answer: "a".into(),
            steps: vec![Step {
                call_id: "c".into(),
                label: "l".into(),
                running: false,
                ok: Some(true),
            }],
            status,
            error: None,
        }
    }

    #[test]
    fn the_envelope_round_trips_with_the_v1_shape() {
        let turns = vec![turn(Status::Done), turn(Status::Error)];
        let json: Value = serde_json::from_str(&encode(&turns)).unwrap();
        assert_eq!(json["v"], 1);
        assert_eq!(json["turns"][0]["steps"][0]["callId"], "c");
        assert_eq!(json["turns"][0]["status"], "done");
        assert!(json["turns"][0]["error"].is_null());
        assert_eq!(decode(&encode(&turns)), turns);
        assert!(decode("not json").is_empty());
    }

    #[test]
    fn a_long_title_is_cut_on_a_word() {
        assert_eq!(make_title("  Short one  "), "Short one");
        let long = "word ".repeat(40);
        let title = make_title(&long);
        assert!(title.ends_with("word…") && title.chars().count() <= TITLE_MAX);
        // One long word has no boundary, so it is cut hard.
        let one = make_title(&"x".repeat(200));
        assert_eq!(one.chars().count(), TITLE_MAX);
    }

    #[test]
    fn a_stale_running_turn_reads_as_stopped() {
        let turns = vec![turn(Status::Running), turn(Status::Running)];
        let idle = settle(turns.clone(), false);
        assert!(idle.iter().all(|t| t.status == Status::Stopped));
        let live = settle(turns, true);
        assert_eq!(live[0].status, Status::Stopped);
        assert_eq!(live[1].status, Status::Running);
    }

    #[test]
    fn the_sample_store_has_two_threads_and_no_stale_turn() {
        let rows = seed();
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .flat_map(|r| decode(&r.messages))
            .all(|t| t.status == Status::Done));
    }

    #[test]
    fn the_list_is_newest_first_and_a_missing_thread_is_null() {
        for (id, at) in [
            ("order-a", "2030-01-01T00:00:00.000Z"),
            ("order-b", "2031-01-01T00:00:00.000Z"),
        ] {
            with_store(true, |s| {
                s.put(&ChatSession {
                    session_id: id.into(),
                    started_at: at.into(),
                    updated_at: at.into(),
                    title: id.into(),
                    message_count: 0,
                    messages: encode(&[]),
                })
            })
            .unwrap();
        }
        let out: Value =
            serde_json::from_str(&list(&json!({ "limit": 100 }), true).unwrap()).unwrap();
        let ids: Vec<&str> = out["threads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["sessionId"].as_str().unwrap())
            .collect();
        let pos = |id: &str| ids.iter().position(|i| *i == id).unwrap();
        assert!(pos("order-b") < pos("order-a"));
        assert_eq!(out["threads"][0]["running"], false);
        assert_eq!(get(&json!({ "sessionId": "nope" }), true).unwrap(), "null");
    }
}
