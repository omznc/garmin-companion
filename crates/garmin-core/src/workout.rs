//! Structured workouts: the app's own shape, and the payload Garmin wants.
//!
//! Garmin's workout format is a segment holding a list of steps, where a step
//! is either something you do or a group repeating steps you do. Every field on
//! it is a `{ id, key }` pair drawn from a fixed vocabulary — `stepTypeId: 3`
//! and `stepTypeKey: "interval"` are one fact written twice, and the API wants
//! both.
//!
//! [`WorkoutDraft`] is the shape everything else in this app speaks: the model
//! emits one, the confirmation card edits one, and only [`WorkoutDraft::payload`]
//! knows about the id pairs. That boundary is the point. The draft is small
//! enough for a language model to get right and for a form to render, and it
//! cannot express a workout Garmin would reject — nesting is one level deep by
//! construction, and every vocabulary field is an enum rather than a string.
//!
//! Nothing here talks to the network. Building a draft and sending one are
//! deliberately separate: [`validate`](WorkoutDraft::validate) runs locally,
//! against a draft nobody has agreed to yet.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Longest a single step may run. Twelve hours is not a sensible session; it is
/// the point past which a number is a units mistake rather than a plan.
const MAX_STEP_SECS: f64 = 12.0 * 3600.0;
const MAX_STEP_METRES: f64 = 200_000.0;
const MAX_STEPS: usize = 50;
const MAX_ITERATIONS: u32 = 99;

/* ------------------------------------------------------------ vocabulary --- */

/// The sports a workout can be built for.
///
/// Not every sport Garmin knows — the ones this athlete's account actually uses
/// and this app can describe. An unknown sport would round-trip as a valid
/// payload and then appear on the watch as something nobody asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sport {
    Running,
    Cycling,
    Cardio,
    StrengthTraining,
}

impl Sport {
    fn ids(self) -> (u32, &'static str) {
        match self {
            Self::Running => (1, "running"),
            Self::Cycling => (2, "cycling"),
            Self::Cardio => (6, "cardio_training"),
            Self::StrengthTraining => (5, "strength_training"),
        }
    }

    fn json(self) -> Value {
        let (id, key) = self.ids();
        json!({ "sportTypeId": id, "sportTypeKey": key, "displayOrder": id })
    }
}

/// What a step is for. Garmin shows these on the watch as the step's colour and
/// label, and `Repeat` is the one that isn't a thing you do — it belongs to a
/// group, which is why [`Step`] carries it rather than this enum being used for
/// both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Warmup,
    Interval,
    Recovery,
    Rest,
    Cooldown,
}

impl StepKind {
    fn ids(self) -> (u32, &'static str) {
        match self {
            Self::Warmup => (1, "warmup"),
            Self::Cooldown => (2, "cooldown"),
            Self::Interval => (3, "interval"),
            Self::Recovery => (4, "recovery"),
            Self::Rest => (5, "rest"),
        }
    }

    fn json(self) -> Value {
        let (id, key) = self.ids();
        json!({ "stepTypeId": id, "stepTypeKey": key, "displayOrder": id })
    }
}

/// When a step ends.
///
/// `LapButton` is the honest option for a step whose length is "until it feels
/// done" — better than inventing a duration the athlete will ignore.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EndCondition {
    Time { seconds: f64 },
    Distance { metres: f64 },
    LapButton,
}

impl EndCondition {
    fn ids(self) -> (u32, &'static str) {
        match self {
            Self::LapButton => (1, "lap.button"),
            Self::Time { .. } => (2, "time"),
            Self::Distance { .. } => (3, "distance"),
        }
    }

    fn json(self) -> Value {
        let (id, key) = self.ids();
        json!({ "conditionTypeId": id, "conditionTypeKey": key, "displayOrder": id })
    }

    /// The scalar that goes alongside the condition. Null for the lap button,
    /// which is what Garmin expects rather than a zero.
    fn value(self) -> Value {
        match self {
            Self::Time { seconds } => json!(seconds),
            Self::Distance { metres } => json!(metres),
            Self::LapButton => Value::Null,
        }
    }
}

/// What the step asks the athlete to hold.
///
/// Heart-rate zones are given as a zone number rather than a bpm range: the
/// zones live on the Garmin account, so a number stays correct when they're
/// retuned and a bpm range silently doesn't. `Bpm` exists for the case where a
/// specific ceiling is the actual instruction — "keep it under 140" is a
/// sentence about a number, not about a zone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Target {
    None,
    HrZone { zone: u32 },
    Bpm { low: f64, high: f64 },
}

impl Target {
    fn ids(self) -> (u32, &'static str) {
        match self {
            Self::None => (1, "no.target"),
            Self::HrZone { .. } | Self::Bpm { .. } => (4, "heart.rate.zone"),
        }
    }

    /// Merged into the step object rather than nested, because that is where
    /// Garmin reads `zoneNumber` and the target values from.
    fn apply(self, step: &mut Value) {
        let (id, key) = self.ids();
        step["targetType"] = json!({
            "workoutTargetTypeId": id,
            "workoutTargetTypeKey": key,
            "displayOrder": id,
        });
        match self {
            Self::None => {
                step["zoneNumber"] = Value::Null;
                step["targetValueOne"] = Value::Null;
                step["targetValueTwo"] = Value::Null;
            }
            Self::HrZone { zone } => {
                step["zoneNumber"] = json!(zone);
                step["targetValueOne"] = Value::Null;
                step["targetValueTwo"] = Value::Null;
            }
            Self::Bpm { low, high } => {
                step["zoneNumber"] = Value::Null;
                step["targetValueOne"] = json!(low);
                step["targetValueTwo"] = json!(high);
            }
        }
    }
}

/* ----------------------------------------------------------------- draft --- */

/// One thing the athlete does, for as long as the end condition says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecStep {
    pub kind: StepKind,
    pub end: EndCondition,
    #[serde(default = "target_none")]
    pub target: Target,
    /// Shown on the watch. The place for "keep it conversational", which is the
    /// part of a coaching instruction no id pair can carry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn target_none() -> Target {
    Target::None
}

/// A step at the top level of a workout: either one thing, or a block repeated.
///
/// A repeat holds [`ExecStep`]s, not [`Step`]s. Garmin's own format nests
/// arbitrarily and its editor does not, and neither does any session this app
/// is for — "4 × (3 min hard / 2 min easy)" is the shape that matters, and it
/// fits in one level. Making that a type rather than a validation rule means a
/// model cannot emit a workout that recurses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Step {
    Exec(ExecStep),
    Repeat { times: u32, steps: Vec<ExecStep> },
}

/// A workout as proposed, before anyone has agreed to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkoutDraft {
    pub name: String,
    pub sport: Sport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub steps: Vec<Step>,
}

impl WorkoutDraft {
    /// Whether this is a workout Garmin would accept and a person would want.
    ///
    /// The errors are written to be read by the model that produced the draft:
    /// they say which step and what would fix it, because a rejected draft goes
    /// back into the conversation as a tool result and the next attempt is only
    /// as good as the complaint.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(anyhow!("the workout needs a name"));
        }
        if self.name.chars().count() > 80 {
            return Err(anyhow!("the name is over 80 characters"));
        }
        if self.steps.is_empty() {
            return Err(anyhow!("the workout has no steps"));
        }
        if self.flat_count() > MAX_STEPS {
            return Err(anyhow!(
                "the workout has more than {MAX_STEPS} steps once repeats are \
                 expanded — use a repeat group instead of listing every rep"
            ));
        }

        for (i, step) in self.steps.iter().enumerate() {
            let at = i + 1;
            match step {
                Step::Exec(e) => check_exec(e, &format!("step {at}"))?,
                Step::Repeat { times, steps } => {
                    if *times < 2 {
                        return Err(anyhow!(
                            "step {at} repeats {times} time(s); a repeat needs at \
                             least 2, or make it a plain step"
                        ));
                    }
                    if *times > MAX_ITERATIONS {
                        return Err(anyhow!(
                            "step {at} repeats {times} times, over the {MAX_ITERATIONS} limit"
                        ));
                    }
                    if steps.is_empty() {
                        return Err(anyhow!("step {at} is a repeat with nothing in it"));
                    }
                    for (j, e) in steps.iter().enumerate() {
                        check_exec(e, &format!("step {at}.{}", j + 1))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Steps as the watch will count them, repeats expanded.
    pub fn flat_count(&self) -> usize {
        self.steps
            .iter()
            .map(|s| match s {
                Step::Exec(_) => 1,
                Step::Repeat { times, steps } => *times as usize * steps.len(),
            })
            .sum()
    }

    /// Total time, where every step is time-bounded. `None` as soon as one
    /// isn't — a workout with a distance step has no duration until it's run,
    /// and guessing one from an assumed pace would put a number on the screen
    /// that nothing measured.
    pub fn est_duration_secs(&self) -> Option<f64> {
        let mut total = 0.0;
        for step in &self.steps {
            match step {
                Step::Exec(e) => total += secs(e)?,
                Step::Repeat { times, steps } => {
                    let inner: Option<f64> = steps.iter().map(secs).sum();
                    total += inner? * f64::from(*times);
                }
            }
        }
        Some(total)
    }

    /// The `POST /workout-service/workout` body.
    ///
    /// Validation is not repeated here — [`validate`](Self::validate) is the
    /// gate, and it runs before anyone is asked to confirm. This is the
    /// translation only.
    pub fn payload(&self) -> Value {
        let sport = self.sport.json();
        let mut steps = Vec::with_capacity(self.steps.len());

        // Garmin orders steps by an explicit field rather than by array
        // position, and the counter runs across the whole segment — a repeat
        // group and the steps inside it draw from the same sequence.
        let mut order = 1u32;
        for step in &self.steps {
            match step {
                Step::Exec(e) => {
                    steps.push(exec_json(e, order));
                    order += 1;
                }
                Step::Repeat {
                    times,
                    steps: inner,
                } => {
                    let group_order = order;
                    order += 1;
                    let children: Vec<Value> = inner
                        .iter()
                        .map(|e| {
                            let v = exec_json(e, order);
                            order += 1;
                            v
                        })
                        .collect();
                    steps.push(json!({
                        "type": "RepeatGroupDTO",
                        "stepOrder": group_order,
                        "stepType": { "stepTypeId": 6, "stepTypeKey": "repeat", "displayOrder": 6 },
                        "numberOfIterations": times,
                        "smartRepeat": false,
                        "endCondition": {
                            "conditionTypeId": 7,
                            "conditionTypeKey": "iterations",
                            "displayOrder": 7,
                        },
                        "endConditionValue": times,
                        "workoutSteps": children,
                    }));
                }
            }
        }

        json!({
            "sportType": sport,
            "workoutName": self.name.trim(),
            "description": self.description.as_deref().map(str::trim),
            "workoutSegments": [{
                "segmentOrder": 1,
                "sportType": sport,
                "workoutSteps": steps,
            }],
        })
    }
}

fn secs(e: &ExecStep) -> Option<f64> {
    match e.end {
        EndCondition::Time { seconds } => Some(seconds),
        _ => None,
    }
}

fn check_exec(e: &ExecStep, at: &str) -> Result<()> {
    match e.end {
        // A range check rather than comparisons, which also disposes of NaN and
        // infinity: neither is contained, so both land in the error arm.
        EndCondition::Time { seconds } => {
            if !(1.0..=MAX_STEP_SECS).contains(&seconds) {
                return Err(anyhow!(
                    "{at} lasts {seconds}s, which is outside 1s–12h — durations \
                     are in seconds, so 10 minutes is 600"
                ));
            }
        }
        EndCondition::Distance { metres } => {
            if !(10.0..=MAX_STEP_METRES).contains(&metres) {
                return Err(anyhow!(
                    "{at} covers {metres}m, which is outside 10m–200km — \
                     distances are in metres, so 5k is 5000"
                ));
            }
        }
        EndCondition::LapButton => {}
    }

    match e.target {
        Target::HrZone { zone } if !(1..=5).contains(&zone) => {
            return Err(anyhow!("{at} targets HR zone {zone}; the zones are 1–5"));
        }
        Target::Bpm { low, high } => {
            if !(low.is_finite() && high.is_finite()) || low < 40.0 || high > 230.0 {
                return Err(anyhow!("{at} targets {low}–{high} bpm, outside 40–230"));
            }
            if low >= high {
                return Err(anyhow!(
                    "{at} targets {low}–{high} bpm; the low bound must be under the high one"
                ));
            }
        }
        _ => {}
    }

    if e.note.as_ref().is_some_and(|n| n.chars().count() > 200) {
        return Err(anyhow!("{at} has a note over 200 characters"));
    }
    Ok(())
}

fn exec_json(e: &ExecStep, order: u32) -> Value {
    let mut v = json!({
        "type": "ExecutableStepDTO",
        "stepOrder": order,
        "stepType": e.kind.json(),
        "endCondition": e.end.json(),
        "endConditionValue": e.end.value(),
        "description": e.note.as_deref().map(str::trim),
    });
    e.target.apply(&mut v);
    v
}

/* ------------------------------------------------------------------ tests --- */

#[cfg(test)]
mod tests {
    use super::*;

    fn step(kind: StepKind, seconds: f64, target: Target) -> ExecStep {
        ExecStep {
            kind,
            end: EndCondition::Time { seconds },
            target,
            note: None,
        }
    }

    /// The session this whole feature exists for: warm up, repeat a hard/easy
    /// block, cool down.
    fn intervals() -> WorkoutDraft {
        WorkoutDraft {
            name: "4 × 3min".into(),
            sport: Sport::Running,
            description: Some("Z2 either side of the reps".into()),
            steps: vec![
                Step::Exec(step(StepKind::Warmup, 600.0, Target::HrZone { zone: 2 })),
                Step::Repeat {
                    times: 4,
                    steps: vec![
                        step(StepKind::Interval, 180.0, Target::HrZone { zone: 4 }),
                        step(StepKind::Recovery, 120.0, Target::HrZone { zone: 1 }),
                    ],
                },
                Step::Exec(step(StepKind::Cooldown, 300.0, Target::HrZone { zone: 2 })),
            ],
        }
    }

    #[test]
    fn a_repeat_group_carries_its_steps_and_keeps_one_order_sequence() {
        let p = intervals().payload();
        let steps = p["workoutSegments"][0]["workoutSteps"].as_array().unwrap();
        assert_eq!(steps.len(), 3, "a repeat is one top-level step, not four");

        assert_eq!(steps[1]["type"], "RepeatGroupDTO");
        assert_eq!(steps[1]["numberOfIterations"], 4);
        // Garmin wants the iteration count in both places.
        assert_eq!(steps[1]["endConditionValue"], 4);
        assert_eq!(steps[1]["endCondition"]["conditionTypeKey"], "iterations");

        let inner = steps[1]["workoutSteps"].as_array().unwrap();
        assert_eq!(inner.len(), 2);

        // The counter runs through the group and out the other side: 1 for the
        // warmup, 2 for the group, 3 and 4 inside it, 5 for the cooldown. A
        // group whose children restart at 1 is the mistake this pins down.
        assert_eq!(steps[0]["stepOrder"], 1);
        assert_eq!(steps[1]["stepOrder"], 2);
        assert_eq!(inner[0]["stepOrder"], 3);
        assert_eq!(inner[1]["stepOrder"], 4);
        assert_eq!(steps[2]["stepOrder"], 5);
    }

    #[test]
    fn every_vocabulary_field_is_written_as_both_id_and_key() {
        let p = intervals().payload();
        assert_eq!(p["sportType"]["sportTypeId"], 1);
        assert_eq!(p["sportType"]["sportTypeKey"], "running");

        let warmup = &p["workoutSegments"][0]["workoutSteps"][0];
        assert_eq!(warmup["stepType"]["stepTypeId"], 1);
        assert_eq!(warmup["stepType"]["stepTypeKey"], "warmup");
        assert_eq!(warmup["endCondition"]["conditionTypeId"], 2);
        assert_eq!(warmup["endCondition"]["conditionTypeKey"], "time");
        assert_eq!(warmup["endConditionValue"], 600.0);
        assert_eq!(warmup["targetType"]["workoutTargetTypeId"], 4);
        assert_eq!(
            warmup["targetType"]["workoutTargetTypeKey"],
            "heart.rate.zone"
        );
        assert_eq!(warmup["zoneNumber"], 2);
    }

    #[test]
    fn a_zone_target_and_a_bpm_target_fill_different_fields() {
        let d = WorkoutDraft {
            name: "Easy".into(),
            sport: Sport::Running,
            description: None,
            steps: vec![
                Step::Exec(step(StepKind::Interval, 600.0, Target::HrZone { zone: 2 })),
                Step::Exec(step(
                    StepKind::Interval,
                    600.0,
                    Target::Bpm {
                        low: 118.0,
                        high: 136.0,
                    },
                )),
                Step::Exec(step(StepKind::Cooldown, 60.0, Target::None)),
            ],
        };
        let s = d.payload();
        let s = s["workoutSegments"][0]["workoutSteps"].as_array().unwrap();

        assert_eq!(s[0]["zoneNumber"], 2);
        assert!(s[0]["targetValueOne"].is_null());

        assert!(s[1]["zoneNumber"].is_null());
        assert_eq!(s[1]["targetValueOne"], 118.0);
        assert_eq!(s[1]["targetValueTwo"], 136.0);

        assert_eq!(s[2]["targetType"]["workoutTargetTypeKey"], "no.target");
        assert!(s[2]["zoneNumber"].is_null());
    }

    /// The lap button has no scalar. Sending 0 would be a step that ends
    /// immediately, which is a worse failure than a rejected payload.
    #[test]
    fn the_lap_button_sends_a_null_rather_than_a_zero() {
        let d = WorkoutDraft {
            name: "Open".into(),
            sport: Sport::Running,
            description: None,
            steps: vec![Step::Exec(ExecStep {
                kind: StepKind::Interval,
                end: EndCondition::LapButton,
                target: Target::None,
                note: None,
            })],
        };
        let p = d.payload();
        let s = &p["workoutSegments"][0]["workoutSteps"][0];
        assert_eq!(s["endCondition"]["conditionTypeKey"], "lap.button");
        assert!(s["endConditionValue"].is_null());
    }

    #[test]
    fn duration_expands_repeats_and_gives_up_on_a_distance_step() {
        // 600 + 4 × (180 + 120) + 300
        assert_eq!(intervals().est_duration_secs(), Some(2100.0));
        assert_eq!(intervals().flat_count(), 10);

        let mut d = intervals();
        d.steps.push(Step::Exec(ExecStep {
            kind: StepKind::Interval,
            end: EndCondition::Distance { metres: 5000.0 },
            target: Target::None,
            note: None,
        }));
        assert_eq!(d.est_duration_secs(), None);
    }

    #[test]
    fn validation_rejects_what_garmin_or_a_person_would() {
        assert!(intervals().validate().is_ok());

        let bad = |f: fn(&mut WorkoutDraft)| {
            let mut d = intervals();
            f(&mut d);
            d.validate().unwrap_err().to_string()
        };

        assert!(bad(|d| d.name = "  ".into()).contains("name"));
        assert!(bad(|d| d.steps.clear()).contains("no steps"));
        // A one-iteration repeat is a plain step wearing a group, and Garmin's
        // own editor won't build one.
        assert!(bad(|d| d.steps[1] = Step::Repeat {
            times: 1,
            steps: vec![]
        })
        .contains("at least 2"));
        assert!(bad(|d| d.steps[1] = Step::Repeat {
            times: 3,
            steps: vec![]
        })
        .contains("nothing in it"));

        // The units mistake worth catching: minutes typed where seconds go is
        // survivable, but hours are not.
        let long = bad(|d| d.steps[0] = Step::Exec(step(StepKind::Warmup, 90_000.0, Target::None)));
        assert!(long.contains("step 1") && long.contains("seconds"));

        let zone = bad(|d| {
            d.steps[0] = Step::Exec(step(StepKind::Warmup, 600.0, Target::HrZone { zone: 9 }))
        });
        assert!(zone.contains("zones are 1–5"));

        // A fault inside a repeat is reported with the position inside it.
        let inner = bad(|d| {
            d.steps[1] = Step::Repeat {
                times: 4,
                steps: vec![
                    step(StepKind::Interval, 180.0, Target::None),
                    step(StepKind::Recovery, 0.0, Target::None),
                ],
            }
        });
        assert!(inner.contains("step 2.2"), "got: {inner}");

        // Reversed bpm bounds pass every individual range check and still make
        // a step nobody can hold.
        let bpm = bad(|d| {
            d.steps[0] = Step::Exec(step(
                StepKind::Warmup,
                600.0,
                Target::Bpm {
                    low: 160.0,
                    high: 120.0,
                },
            ))
        });
        assert!(bpm.contains("under the high one"));
    }

    /// A model that writes out every rep instead of using a repeat group builds
    /// something the watch can technically run and nobody can read.
    #[test]
    fn validation_rejects_an_unrolled_interval_session() {
        let d = WorkoutDraft {
            name: "Unrolled".into(),
            sport: Sport::Running,
            description: None,
            steps: (0..60)
                .map(|_| Step::Exec(step(StepKind::Interval, 60.0, Target::None)))
                .collect(),
        };
        assert!(d
            .validate()
            .unwrap_err()
            .to_string()
            .contains("repeat group"));
    }

    /// The draft crosses to the UI as JSON and comes back edited, so the
    /// tagged-enum shapes have to survive the round trip unchanged.
    #[test]
    fn a_draft_round_trips_through_json() {
        let d = intervals();
        let text = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<WorkoutDraft>(&text).unwrap(), d);

        // And the shape a model would actually emit, written by hand — the
        // field names here are the ones the tool schema promises.
        let from_model: WorkoutDraft = serde_json::from_value(json!({
            "name": "Easy 30",
            "sport": "running",
            "steps": [
                { "type": "exec", "kind": "warmup", "end": { "type": "time", "seconds": 300 } },
                {
                    "type": "repeat",
                    "times": 3,
                    "steps": [
                        {
                            "kind": "interval",
                            "end": { "type": "distance", "metres": 1000 },
                            "target": { "type": "hr_zone", "zone": 2 },
                            "note": "conversational"
                        },
                        { "kind": "recovery", "end": { "type": "lap_button" } }
                    ]
                }
            ]
        }))
        .expect("the documented shape parses");
        assert!(from_model.validate().is_ok());
        // `target` defaults to none where a model leaves it out.
        assert_eq!(from_model.steps.len(), 2);
    }
}
