//! The saved threads of the Ask screen.
//!
//! A thread is one `chat_sessions` row. Its `messages` column holds
//! `{"v":1,"turns":[...]}`. The threads stay on the device: the cloud worker
//! never sees them. With [`Source::Sample`] the rows stay in memory, because a
//! real save would create the cache and end sample mode.
//!
//! The desktop app calls these functions. The mobile bridge calls them too,
//! and adds the state of the turn that runs now.

use std::sync::{Mutex, MutexGuard};

use chrono::SecondsFormat;
use garmin_core::db::{ChatSession, ChatSessionMeta, Db};
use garmin_core::workout::WorkoutDraft;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{ApiError, Source};

const SAMPLE_THREADS: &str = include_str!("../fixtures/chat.sample.json");
const TITLE_MAX: usize = 80;
/// The longest title that a rename keeps. The rename dialogs use the same number.
pub const RENAME_MAX: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export, rename = "ChatStep"))]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// A row from before notes existed has no kind: it is a tool.
    #[serde(default, skip_serializing_if = "is_tool")]
    pub kind: StepKind,
    pub call_id: String,
    /// The tool label, or the text of a note.
    pub label: String,
    pub running: bool,
    /// `None` while the call runs.
    pub ok: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts-rs",
    derive(ts_rs::TS),
    ts(export, rename = "ChatStepKind")
)]
#[serde(rename_all = "lowercase")]
pub enum StepKind {
    #[default]
    Tool,
    /// What the model said between two tool rounds.
    Note,
    /// A kind from a newer build. Decode must not fail on it.
    #[serde(other)]
    Other,
}

fn is_tool(kind: &StepKind) -> bool {
    *kind == StepKind::Tool
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts-rs",
    derive(ts_rs::TS),
    ts(export, rename = "TurnStatus")
)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Running,
    Done,
    Error,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub steps: Vec<Step>,
    pub status: Status,
    pub error: Option<String>,
    /// The workouts that the coach proposed in this turn. A row from before
    /// drafts existed has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drafts: Vec<DraftEntry>,
}

/// One proposed workout. `saved_workout_id` is set once the person saved it to
/// Garmin, so a second press does not create a second workout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DraftEntry {
    pub call_id: String,
    pub draft: WorkoutDraft,
    #[serde(default)]
    pub saved_workout_id: Option<i64>,
}

/// A saved thread in the list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ThreadMeta {
    pub session_id: String,
    pub title: String,
    pub updated_at: String,
    pub turn_count: i64,
    /// A turn of this thread runs now.
    pub running: bool,
}

/// The result of `chat_list`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ThreadList {
    pub threads: Vec<ThreadMeta>,
}

/// One saved thread with its turns. The result of `chat_get`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub session_id: String,
    pub title: String,
    pub updated_at: String,
    pub running: bool,
    pub turns: Vec<Turn>,
}

/// The arguments of `chat_list`.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ListArgs {
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    pub limit: Option<u32>,
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    pub offset: Option<u32>,
}

/// The arguments of `chat_get` and `chat_delete`.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SessionArgs {
    pub session_id: String,
}

/// The arguments of `chat_rename`.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RenameArgs {
    pub session_id: String,
    pub title: String,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    v: u32,
    turns: Vec<Turn>,
}

pub fn encode(turns: &[Turn]) -> String {
    serde_json::to_string(&json!({ "v": 1, "turns": turns })).unwrap_or_default()
}

/// A body that does not parse reads as an empty thread, so the list still opens.
pub fn decode(messages: &str) -> Vec<Turn> {
    serde_json::from_str::<Envelope>(messages).map_or_else(|_| Vec::new(), |e| e.turns)
}

/// The first question, cut on a word boundary to fit `TITLE_MAX` characters.
pub fn make_title(question: &str) -> String {
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
trait Store {
    fn get(&self, id: &str) -> Result<Option<ChatSession>, ApiError>;
    fn put(&self, row: &ChatSession) -> Result<(), ApiError>;
    fn list(&self, limit: u32, offset: u32) -> Result<Vec<ChatSessionMeta>, ApiError>;
    /// Change only the turns of a thread that exists. A missing thread stays missing.
    fn update_turns(&self, id: &str, turns: &[Turn]) -> Result<(), ApiError>;
    fn rename(&self, id: &str, title: &str) -> Result<(), ApiError>;
    fn delete(&self, id: &str) -> Result<(), ApiError>;
}

impl Store for Db {
    fn get(&self, id: &str) -> Result<Option<ChatSession>, ApiError> {
        Ok(self.chat_session(id)?)
    }
    fn put(&self, row: &ChatSession) -> Result<(), ApiError> {
        Ok(self.save_chat_session(row)?)
    }
    fn list(&self, limit: u32, offset: u32) -> Result<Vec<ChatSessionMeta>, ApiError> {
        Ok(self.chat_sessions(limit, offset)?)
    }
    fn update_turns(&self, id: &str, turns: &[Turn]) -> Result<(), ApiError> {
        Ok(self.update_chat_messages(id, &now(), turns.len() as i64, &encode(turns))?)
    }
    fn rename(&self, id: &str, title: &str) -> Result<(), ApiError> {
        Ok(self.rename_chat_session(id, title)?)
    }
    fn delete(&self, id: &str) -> Result<(), ApiError> {
        Ok(self.delete_chat_session(id)?)
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
    fn get(&self, id: &str) -> Result<Option<ChatSession>, ApiError> {
        Ok(with_sample(|r| {
            r.iter().find(|s| s.session_id == id).cloned()
        }))
    }
    fn put(&self, row: &ChatSession) -> Result<(), ApiError> {
        with_sample(
            |r| match r.iter_mut().find(|s| s.session_id == row.session_id) {
                Some(slot) => *slot = row.clone(),
                None => r.push(row.clone()),
            },
        );
        Ok(())
    }
    fn list(&self, limit: u32, offset: u32) -> Result<Vec<ChatSessionMeta>, ApiError> {
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
    fn update_turns(&self, id: &str, turns: &[Turn]) -> Result<(), ApiError> {
        with_sample(|r| {
            if let Some(s) = r.iter_mut().find(|s| s.session_id == id) {
                s.updated_at = now();
                s.message_count = turns.len() as i64;
                s.messages = encode(turns);
            }
        });
        Ok(())
    }
    fn rename(&self, id: &str, title: &str) -> Result<(), ApiError> {
        with_sample(|r| {
            if let Some(s) = r.iter_mut().find(|s| s.session_id == id) {
                s.title = title.to_owned();
            }
        });
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), ApiError> {
        with_sample(|r| r.retain(|s| s.session_id != id));
        Ok(())
    }
}

/// One read-modify-write at a time, so two turns never lose each other's turn.
static STORE_LOCK: Mutex<()> = Mutex::new(());

fn with_store<T>(
    source: Source,
    f: impl FnOnce(&dyn Store) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match source {
        Source::Sample => f(&Sample),
        Source::Live => f(&crate::open_default_db()?),
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Save a thread row as it is. A test helper: a turn adds rows through [`begin_turn`].
#[doc(hidden)]
pub fn put_row(source: Source, row: &ChatSession) -> Result<(), ApiError> {
    with_store(source, |s| s.put(row))
}

/// Add a running turn to the thread. Only a new thread is inserted, with the
/// title from the first question, so a rename stays.
pub fn begin_turn(source: Source, session_id: &str, question: &str) -> Result<Turn, ApiError> {
    with_store(source, |store| {
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
            drafts: Vec::new(),
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

/// Store the state of a turn. The turn in memory can be older than the stored
/// one: a draft may be saved to Garmin while the turn still runs. So the saved
/// ids of the stored turn stay. A thread or turn that is gone stays gone.
pub fn finish_turn(source: Source, session_id: &str, turn: &Turn) -> Result<(), ApiError> {
    with_store(source, |store| {
        let Some(row) = store.get(session_id)? else {
            return Ok(());
        };
        let mut turns = decode(&row.messages);
        let Some(slot) = turns.iter_mut().find(|t| t.id == turn.id) else {
            return Ok(());
        };
        let mut next = turn.clone();
        for d in &mut next.drafts {
            let stored = slot.drafts.iter().find(|s| s.call_id == d.call_id);
            d.saved_workout_id = d
                .saved_workout_id
                .or(stored.and_then(|s| s.saved_workout_id));
        }
        *slot = next;
        store.update_turns(session_id, &turns)
    })
}

/// Set the saved workout id of one draft in the stored thread. Returns the
/// draft, or `None` when the thread, turn or draft is gone.
pub fn set_saved(
    source: Source,
    (session_id, turn_id, call_id): (&str, &str, &str),
    workout_id: i64,
) -> Result<Option<DraftEntry>, ApiError> {
    with_store(source, |store| {
        let Some(row) = store.get(session_id)? else {
            return Ok(None);
        };
        let mut turns = decode(&row.messages);
        let found = turns
            .iter_mut()
            .filter(|t| t.id == turn_id)
            .flat_map(|t| t.drafts.iter_mut())
            .find(|d| d.call_id == call_id);
        let Some(entry) = found else {
            return Ok(None);
        };
        entry.saved_workout_id = Some(workout_id);
        let saved = entry.clone();
        store.update_turns(session_id, &turns)?;
        Ok(Some(saved))
    })
}

/// The stored draft of one turn.
pub fn find_draft(
    source: Source,
    (session_id, turn_id, call_id): (&str, &str, &str),
) -> Result<Option<DraftEntry>, ApiError> {
    with_store(source, |store| {
        let row = store.get(session_id)?;
        let turns = row.map_or_else(Vec::new, |r| decode(&r.messages));
        Ok(turns
            .into_iter()
            .filter(|t| t.id == turn_id)
            .flat_map(|t| t.drafts)
            .find(|d| d.call_id == call_id))
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

/// The saved threads, newest first. `running` is the session of the turn that
/// runs now, if any: only the host knows it.
pub fn list_threads(
    source: Source,
    args: &ListArgs,
    running: Option<&str>,
) -> Result<ThreadList, ApiError> {
    let rows = with_store(source, |s| {
        s.list(args.limit.unwrap_or(50), args.offset.unwrap_or(0))
    })?;
    let threads = rows
        .into_iter()
        .map(|m| ThreadMeta {
            running: running == Some(m.session_id.as_str()),
            session_id: m.session_id,
            title: m.title,
            updated_at: m.updated_at,
            turn_count: m.message_count,
        })
        .collect();
    Ok(ThreadList { threads })
}

/// One thread, or `None` when it is gone. `running` says that the turn of this
/// thread runs now, and `live` is that turn as it is now in memory.
pub fn get_thread(
    source: Source,
    session_id: &str,
    running: bool,
    live: Option<Turn>,
) -> Result<Option<Thread>, ApiError> {
    let Some(row) = with_store(source, |s| s.get(session_id))? else {
        return Ok(None);
    };
    let mut turns = decode(&row.messages);
    if let Some(live) = live {
        match turns.iter_mut().find(|t| t.id == live.id) {
            Some(slot) => *slot = live,
            None => turns.push(live),
        }
    }
    Ok(Some(Thread {
        session_id: row.session_id,
        title: row.title,
        updated_at: row.updated_at,
        running,
        turns: settle(turns, running),
    }))
}

/// Rename a thread. The title is trimmed and cut to [`RENAME_MAX`] characters.
pub fn rename_thread(source: Source, session_id: &str, title: &str) -> Result<(), ApiError> {
    let title: String = title.trim().chars().take(RENAME_MAX).collect();
    if title.is_empty() {
        return Err(ApiError::BadArguments("title is empty".into()));
    }
    with_store(source, |s| s.rename(session_id, &title))
}

/// Delete a thread. The host must also stop a turn that runs in it.
pub fn delete_thread(source: Source, session_id: &str) -> Result<(), ApiError> {
    with_store(source, |s| s.delete(session_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MODE: Source = Source::Sample;

    fn turn(status: Status) -> Turn {
        Turn {
            id: "s:0".into(),
            question: "q".into(),
            answer: "a".into(),
            steps: vec![Step {
                kind: StepKind::Tool,
                call_id: "c".into(),
                label: "l".into(),
                running: false,
                ok: Some(true),
            }],
            status,
            error: None,
            drafts: Vec::new(),
        }
    }

    #[test]
    fn a_draft_entry_round_trips_and_an_old_row_has_none() {
        let draft = garmin_core::workout::parse_draft(&json!({
            "name": "Easy", "sport": "running",
            "steps": [{ "kind": "interval", "end": { "seconds": 600 } }]
        }))
        .unwrap();
        let mut t = turn(Status::Done);
        t.drafts.push(DraftEntry {
            call_id: "d1".into(),
            draft,
            saved_workout_id: Some(42),
        });
        let json: serde_json::Value = serde_json::from_str(&encode(&[t.clone()])).unwrap();
        assert_eq!(json["turns"][0]["drafts"][0]["callId"], "d1");
        assert_eq!(json["turns"][0]["drafts"][0]["savedWorkoutId"], 42);
        assert_eq!(json["turns"][0]["drafts"][0]["draft"]["name"], "Easy");
        assert_eq!(decode(&encode(&[t])).remove(0).drafts.len(), 1);
        // A turn without drafts writes no key, and an old row reads as empty.
        let plain: serde_json::Value =
            serde_json::from_str(&encode(&[turn(Status::Done)])).unwrap();
        assert!(plain["turns"][0].get("drafts").is_none());
        let old = r#"{"v":1,"turns":[{"id":"s:0","question":"q","answer":"a","steps":[],"status":"done","error":null}]}"#;
        assert!(decode(old)[0].drafts.is_empty());
    }

    #[test]
    fn the_envelope_round_trips_with_the_v1_shape() {
        let turns = vec![turn(Status::Done), turn(Status::Error)];
        let json: serde_json::Value = serde_json::from_str(&encode(&turns)).unwrap();
        assert_eq!(json["v"], 1);
        assert_eq!(json["turns"][0]["steps"][0]["callId"], "c");
        assert_eq!(json["turns"][0]["status"], "done");
        assert!(json["turns"][0]["error"].is_null());
        assert_eq!(decode(&encode(&turns)), turns);
        assert!(decode("not json").is_empty());
    }

    #[test]
    fn an_old_step_has_no_kind_and_reads_as_a_tool() {
        let old = r#"{"callId":"c","label":"l","running":false,"ok":true}"#;
        let step: Step = serde_json::from_str(old).unwrap();
        assert_eq!(step.kind, StepKind::Tool);
        assert!(!serde_json::to_string(&step).unwrap().contains("kind"));
        let note = Step {
            kind: StepKind::Note,
            ..step
        };
        assert!(serde_json::to_string(&note)
            .unwrap()
            .contains(r#""kind":"note""#));
    }

    #[test]
    fn a_step_kind_from_a_newer_build_still_decodes() {
        let json = r#"{"kind":"chart","callId":"c","label":"l","running":false,"ok":null}"#;
        let step: Step = serde_json::from_str(json).unwrap();
        assert_eq!(step.kind, StepKind::Other);
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

    fn put_body(id: &str, at: &str, body: &str) {
        put_row(
            SAMPLE_MODE,
            &ChatSession {
                session_id: id.into(),
                started_at: at.into(),
                updated_at: at.into(),
                title: id.into(),
                message_count: 0,
                messages: body.into(),
            },
        )
        .unwrap();
    }

    #[test]
    fn the_list_is_newest_first_and_a_missing_thread_is_none() {
        put_body("order-a", "2030-01-01T00:00:00.000Z", &encode(&[]));
        put_body("order-b", "2031-01-01T00:00:00.000Z", &encode(&[]));
        let args = ListArgs {
            limit: Some(100),
            offset: None,
        };
        let out = list_threads(SAMPLE_MODE, &args, None).unwrap();
        let ids: Vec<&str> = out.threads.iter().map(|t| t.session_id.as_str()).collect();
        let pos = |id: &str| ids.iter().position(|i| *i == id).unwrap();
        assert!(pos("order-b") < pos("order-a"));
        assert!(!out.threads[0].running);
        assert!(get_thread(SAMPLE_MODE, "nope", false, None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_rename_is_cut_to_the_cap_on_a_char() {
        put_body("cap", "2030-01-01T00:00:00.000Z", "");
        let long = "é".repeat(RENAME_MAX + 50);
        rename_thread(SAMPLE_MODE, "cap", &format!("  {long}  ")).unwrap();
        let title = get_thread(SAMPLE_MODE, "cap", false, None)
            .unwrap()
            .unwrap()
            .title;
        assert_eq!(title.chars().count(), RENAME_MAX);
        assert!(matches!(
            rename_thread(SAMPLE_MODE, "cap", "  "),
            Err(ApiError::BadArguments(_))
        ));
    }
}
