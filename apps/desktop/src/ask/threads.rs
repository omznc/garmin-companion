//! The saved threads of the coach: the glue between the Ask screen and
//! `garmin_api::chat`.
//!
//! The thread format is the one that the mobile app writes, so a cache that
//! moves between devices shows the same threads. A turn is saved from the
//! thread of the engine, not from the UI: the answer reaches the store even
//! when the user leaves the thread while the coach still writes.

use std::path::Path;
use std::sync::Mutex;

use garmin_api::chat::{self, DraftEntry, Status, Step, StepKind};
use garmin_coach::Event;
use garmin_core::db::Db;

use super::{DraftCard, Save, Source, Turn, Who};

/// The key in the cache that names the thread that was open at the last quit.
const CURRENT_KEY: &str = "desktop_chat_thread";

/// Where the threads live. Only the real cache keeps them. A demo keeps them in
/// memory, so a demo never writes the data folder.
pub fn store_for(source: &Source) -> garmin_api::Source {
    match source {
        Source::Db(_) => garmin_api::Source::Live,
        Source::Demo | Source::Copy(_) => garmin_api::Source::Sample,
    }
}

/// A new thread id, in the form of the mobile app: `s-<time>-<random>`.
pub fn new_session_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let ms = chrono::Utc::now().timestamp_millis().unsigned_abs();
    let random = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    format!("s-{}-{}", base36(ms), &base36(random)[..6])
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The thread that was open at the last quit. A cache that cannot open, or a
/// value that is empty, gives `None`.
pub fn load_current(path: &Path) -> Option<String> {
    let db = Db::open(path).ok()?;
    db.sync_state(CURRENT_KEY)
        .ok()
        .flatten()
        .filter(|id| !id.is_empty())
}

/// Remember the open thread. `None` stores an empty value: a new chat.
pub fn save_current(path: &Path, session: Option<&str>) {
    let saved = Db::open(path).and_then(|db| db.set_sync_state(CURRENT_KEY, session.unwrap_or("")));
    if let Err(e) = saved {
        eprintln!("could not save the open thread: {e:#}");
    }
}

/// Saves the turn that runs: it applies each event to the stored form of the
/// turn and writes it when the turn ends or a workout draft arrives.
pub struct TurnSaver {
    source: garmin_api::Source,
    session: String,
    turn: Mutex<chat::Turn>,
}

impl TurnSaver {
    /// Add the running turn to the thread. A store that fails gives `None`: the
    /// turn still runs, and only the save is lost.
    pub fn begin(source: garmin_api::Source, session: &str, question: &str) -> Option<Self> {
        match chat::begin_turn(source, session, question) {
            Ok(turn) => Some(Self {
                source,
                session: session.to_owned(),
                turn: Mutex::new(turn),
            }),
            Err(e) => {
                eprintln!("could not save the question: {e}");
                None
            }
        }
    }

    pub fn on_event(&self, event: &Event, stopped: bool) {
        let mut turn = self.turn.lock().unwrap_or_else(|e| e.into_inner());
        if apply_event(&mut turn, event, stopped) {
            if let Err(e) = chat::finish_turn(self.source, &self.session, &turn) {
                eprintln!("could not save the answer: {e}");
            }
        }
    }
}

/// Apply one event to the stored turn. True when the turn must be written now:
/// it ended, or a workout draft arrived and the card needs a stored draft.
/// The desktop shows no notes: the text of a round that ended in tool calls is
/// dropped, as the screen drops it.
pub fn apply_event(turn: &mut chat::Turn, event: &Event, stopped: bool) -> bool {
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
        Event::Note { .. } => turn.answer.clear(),
        Event::Draft { call_id, draft } => turn.drafts.push(DraftEntry {
            call_id: call_id.clone(),
            draft: draft.clone(),
            saved_workout_id: None,
        }),
        Event::Done { .. } => {
            turn.status = if stopped {
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
    turn.status != Status::Running || matches!(event, Event::Draft { .. })
}

/// Save the draft of a turn to Garmin, and keep the workout id in the thread.
/// A draft that was saved before returns its id and does not call Garmin again.
pub fn create_and_record(
    source: garmin_api::Source,
    (session, turn, call): &(String, String, String),
    draft: &garmin_core::workout::WorkoutDraft,
) -> Result<i64, garmin_api::ApiError> {
    let ids = (session.as_str(), turn.as_str(), call.as_str());
    if let Some(DraftEntry {
        saved_workout_id: Some(id),
        ..
    }) = chat::find_draft(source, ids)?
    {
        return Ok(id);
    }
    let id = garmin_api::workouts::create_workout(draft)?;
    if let Err(e) = chat::set_saved(source, ids, id) {
        eprintln!("could not record the saved workout: {e}");
    }
    Ok(id)
}

/// The turns of a saved thread as the screen draws them: your question, then
/// the answer. A note of a mobile thread has no row here, only the tool steps.
pub fn from_saved(turns: Vec<chat::Turn>) -> Vec<Turn> {
    turns
        .into_iter()
        .flat_map(|t| {
            let mut coach = Turn::new(Who::Coach, t.answer);
            coach.id = t.id;
            coach.error = t.error;
            coach.steps = t
                .steps
                .into_iter()
                .filter(|s| s.kind == StepKind::Tool)
                .map(|s| super::Step {
                    call_id: s.call_id,
                    label: s.label,
                    running: false,
                    ok: s.ok.unwrap_or(false),
                })
                .collect();
            coach.drafts = t
                .drafts
                .into_iter()
                .map(|d| DraftCard {
                    call_id: d.call_id,
                    save: match d.saved_workout_id {
                        Some(_) => Save::Saved,
                        None => Save::Idle,
                    },
                    draft: d.draft,
                })
                .collect();
            [Turn::new(Who::You, t.question), coach]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use garmin_api::chat::{get_thread, list_threads, ListArgs};
    use garmin_core::workout::parse_draft;

    use super::*;

    /// A data folder for the tests of this file. The first call sets it. A
    /// test never reaches the real data folder: the check stops a run that did.
    fn home() -> &'static Path {
        static HOME: OnceLock<tempfile::TempDir> = OnceLock::new();
        let dir = HOME.get_or_init(|| tempfile::tempdir().unwrap());
        garmin_api::init(dir.path());
        assert!(
            garmin_core::paths::base_dir()
                .unwrap()
                .starts_with(dir.path()),
            "the data folder is not the temp folder"
        );
        dir.path()
    }

    fn event(text: &str) -> Event {
        Event::Delta { text: text.into() }
    }

    fn draft() -> garmin_core::workout::WorkoutDraft {
        parse_draft(&serde_json::json!({
            "name": "Easy", "sport": "running",
            "steps": [{ "kind": "interval", "end": { "seconds": 600 } }]
        }))
        .unwrap()
    }

    #[test]
    fn a_turn_saves_and_loads_again_in_the_cache() {
        home();
        let live = garmin_api::Source::Live;
        let saver = TurnSaver::begin(live, "s-test-1", "How did I sleep?").unwrap();
        saver.on_event(&event("Well."), false);
        saver.on_event(
            &Event::Tool {
                call_id: "c1".into(),
                label: "Reading sleep".into(),
                running: false,
                ok: true,
            },
            false,
        );
        saver.on_event(
            &Event::Draft {
                call_id: "d1".into(),
                draft: draft(),
            },
            false,
        );
        saver.on_event(&Event::Done { sources: vec![] }, false);

        let thread = get_thread(live, "s-test-1", false, None).unwrap().unwrap();
        assert_eq!(thread.title, "How did I sleep?");
        let ui = from_saved(thread.turns);
        assert_eq!(ui.len(), 2);
        assert_eq!(
            (ui[0].who.clone(), ui[0].text.as_str()),
            (Who::You, "How did I sleep?")
        );
        assert_eq!(ui[1].text, "Well.");
        assert_eq!(ui[1].id, "s-test-1:0");
        assert_eq!(ui[1].steps.len(), 1);
        assert_eq!(ui[1].drafts[0].save, Save::Idle);

        // A second question goes into the same thread.
        TurnSaver::begin(live, "s-test-1", "And last week?").unwrap();
        let list = list_threads(
            live,
            &ListArgs {
                limit: None,
                offset: None,
            },
            None,
        )
        .unwrap();
        let mine = list.threads.iter().find(|t| t.session_id == "s-test-1");
        assert_eq!(mine.map(|t| t.turn_count), Some(2));
    }

    #[test]
    fn a_saved_draft_keeps_its_state_after_the_turn_saves_again() {
        home();
        let live = garmin_api::Source::Live;
        let saver = TurnSaver::begin(live, "s-test-2", "Plan a run").unwrap();
        let add = Event::Draft {
            call_id: "d1".into(),
            draft: draft(),
        };
        saver.on_event(&add, false);
        chat::set_saved(live, ("s-test-2", "s-test-2:0", "d1"), 77).unwrap();
        // The turn in memory does not know the id, and the end of the turn must keep it.
        saver.on_event(&Event::Done { sources: vec![] }, false);
        let thread = get_thread(live, "s-test-2", false, None).unwrap().unwrap();
        assert_eq!(thread.turns[0].drafts[0].saved_workout_id, Some(77));
        assert_eq!(from_saved(thread.turns)[1].drafts[0].save, Save::Saved);
        // A second press finds the id in the thread and does not call Garmin.
        let ids = (
            "s-test-2".to_owned(),
            "s-test-2:0".to_owned(),
            "d1".to_owned(),
        );
        assert_eq!(create_and_record(live, &ids, &draft()).unwrap(), 77);
    }

    #[test]
    fn a_stopped_turn_and_an_error_end_with_their_status() {
        let mut turn = chat::Turn {
            id: "s:0".into(),
            question: "q".into(),
            answer: String::new(),
            steps: vec![],
            status: Status::Running,
            error: None,
            drafts: vec![],
        };
        assert!(!apply_event(&mut turn, &event("a"), false));
        assert!(apply_event(
            &mut turn,
            &Event::Done { sources: vec![] },
            true
        ));
        assert_eq!(turn.status, Status::Stopped);
        assert!(apply_event(
            &mut turn,
            &Event::Error { text: "no".into() },
            false
        ));
        assert_eq!(
            (turn.status, turn.error.as_deref()),
            (Status::Error, Some("no"))
        );
    }

    #[test]
    fn the_open_thread_is_remembered_and_cleared() {
        let path = home().join("current.sqlite3");
        assert_eq!(load_current(&path), None);
        save_current(&path, Some("s-test-3"));
        assert_eq!(load_current(&path).as_deref(), Some("s-test-3"));
        save_current(&path, None);
        assert_eq!(load_current(&path), None);
    }

    #[test]
    fn a_demo_keeps_its_threads_out_of_the_cache() {
        assert_eq!(store_for(&Source::Demo), garmin_api::Source::Sample);
        assert_eq!(
            store_for(&Source::Copy("x".into())),
            garmin_api::Source::Sample
        );
        assert_eq!(store_for(&Source::Db("x".into())), garmin_api::Source::Live);
    }

    #[test]
    fn a_session_id_has_the_mobile_form() {
        let id = new_session_id();
        let parts: Vec<_> = id.split('-').collect();
        assert_eq!((parts[0], parts.len(), parts[2].len()), ("s", 3, 6));
        assert_ne!(id, new_session_id());
    }
}
