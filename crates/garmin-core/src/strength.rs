//! Strength sessions, set by set.
//!
//! What the watch actually records is narrower than it first looks, and this
//! module is built around that rather than around what a lifting log usually
//! shows.
//!
//! **Reliable:** how many sets, how many reps in each, how long each set took,
//! how long the rest between them was, and the order.
//!
//! **Not reliable:** the load. `weight` comes back null on every set unless it
//! was typed into Garmin Connect by hand, so there is no volume in kilograms
//! and no per-lift progression by weight. Nothing here invents one.
//!
//! **A guess:** which exercise it was. Garmin infers that from wrist motion and
//! returns several candidates with probabilities — `UNKNOWN` frequently wins.
//! A guess is only carried through when it is confident enough to be worth
//! showing, and it is labelled as a guess wherever it surfaces.
//!
//! What's left is still worth having. Reps, time under tension and rest
//! discipline are most of what separates one strength session from another, and
//! none of them were visible in this app before.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How sure the watch has to be about an exercise before its guess is carried
/// through. Below this the set is simply unlabelled — which is honest, and
/// which most sets are.
const GUESS_FLOOR_PCT: f64 = 50.0;

/// How far clear of the runner-up the best candidate has to be.
///
/// Not a nicety. The watch routinely returns two movements at the *identical*
/// probability — a real session here has BENCH_PRESS and SHOULDER_PRESS both at
/// 74.609375 — which means it could not tell them apart. Picking one is a coin
/// flip dressed up as a label, so a tie is reported as no guess at all.
const GUESS_MARGIN_PCT: f64 = 0.01;

/// One entry in a strength session: either work or the rest after it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExerciseSet {
    pub activity_id: i64,
    /// Position in the session, as Garmin numbers it. Rest sets are interleaved
    /// with work sets in this ordering.
    pub set_index: i64,
    /// True for a work set, false for the rest that follows one.
    pub active: bool,
    pub duration_s: Option<f64>,
    /// Null on rest sets, and on work sets the watch couldn't count.
    pub reps: Option<i64>,
    /// Garmin's category for the movement — `BENCH_PRESS`, `CURL`, … — only
    /// when it was at least [`GUESS_FLOOR_PCT`] sure and didn't say `UNKNOWN`.
    /// Always a guess; never present it as recorded fact.
    pub exercise: Option<String>,
    /// How sure the watch was, 0–100.
    pub exercise_confidence: Option<f64>,
    /// Kilograms, if the load was entered by hand in Garmin Connect. Null on
    /// every set this account has ever recorded.
    pub weight_kg: Option<f64>,
    pub start_time: Option<String>,
}

/// Parse the `exerciseSets` payload for one activity.
pub fn parse_sets(activity_id: i64, v: &Value) -> Vec<ExerciseSet> {
    v["exerciseSets"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, s)| {
            // `UNKNOWN` is dropped before ranking, so a confident UNKNOWN
            // doesn't beat a plausible real guess. What survives is sorted, and
            // the top one only counts if it beat the runner-up by more than a
            // rounding error — see `GUESS_MARGIN_PCT`.
            let mut candidates: Vec<(&str, f64)> = s["exercises"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| Some((e["category"].as_str()?, e["probability"].as_f64()?)))
                .filter(|(c, p)| *c != "UNKNOWN" && *p >= GUESS_FLOOR_PCT)
                .collect();
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1));

            let best = match candidates.as_slice() {
                [only] => Some(*only),
                [top, next, ..] if top.1 - next.1 > GUESS_MARGIN_PCT => Some(*top),
                _ => None,
            };

            ExerciseSet {
                activity_id,
                set_index: s["messageIndex"].as_i64().unwrap_or(i as i64),
                active: s["setType"].as_str() != Some("REST"),
                duration_s: s["duration"].as_f64(),
                reps: s["repetitionCount"].as_i64(),
                exercise: best.map(|(c, _)| c.to_string()),
                exercise_confidence: best.map(|(_, p)| p),
                weight_kg: s["weight"].as_f64().map(|g| g / 1000.0),
                start_time: s["startTime"].as_str().map(str::to_owned),
            }
        })
        .collect()
}

/// One strength session, summarised from its sets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrengthSession {
    pub activity_id: i64,
    pub name: Option<String>,
    pub date: Option<String>,
    pub duration_min: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub calories: Option<f64>,
    /// Work sets. Rest entries are not counted as sets.
    pub work_sets: usize,
    pub total_reps: i64,
    /// Seconds spent working, summed across the work sets. The closest thing to
    /// time under tension this data supports.
    pub work_s: f64,
    pub rest_s: f64,
    /// Work divided by rest. Below ~0.3 is long, strength-style rests; above
    /// ~1.0 is circuit pacing.
    pub work_rest_ratio: Option<f64>,
    /// Median seconds of rest between work sets. Median rather than mean
    /// because one four-minute phone break skews a mean and says nothing about
    /// how the session was actually paced.
    pub median_rest_s: Option<f64>,
    pub avg_reps_per_set: Option<f64>,
    /// Movements the watch was confident enough to name, commonest first, with
    /// how many sets it put in each. Frequently empty — that is the normal
    /// case, not a failure.
    pub guessed_exercises: Vec<ExerciseCount>,
    /// How many work sets carried no usable guess.
    pub unlabelled_sets: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExerciseCount {
    pub exercise: String,
    pub sets: usize,
    pub reps: i64,
    /// Mean confidence across those sets, 0–100. Shown so a 51% guess doesn't
    /// read the same as a 95% one.
    pub confidence: f64,
}

/// Summarise one session's sets.
pub fn summarise(sets: &[ExerciseSet]) -> StrengthSession {
    let (work, rest): (Vec<&ExerciseSet>, Vec<&ExerciseSet>) = sets.iter().partition(|s| s.active);

    let work_s: f64 = work.iter().filter_map(|s| s.duration_s).sum();
    let rest_s: f64 = rest.iter().filter_map(|s| s.duration_s).sum();
    let total_reps: i64 = work.iter().filter_map(|s| s.reps).sum();

    let mut rests: Vec<f64> = rest.iter().filter_map(|s| s.duration_s).collect();
    rests.sort_by(f64::total_cmp);
    let median_rest_s = (!rests.is_empty()).then(|| {
        let mid = rests.len() / 2;
        if rests.len().is_multiple_of(2) {
            (rests[mid - 1] + rests[mid]) / 2.0
        } else {
            rests[mid]
        }
    });

    let mut counts: Vec<ExerciseCount> = Vec::new();
    for s in &work {
        let Some(name) = s.exercise.as_deref() else {
            continue;
        };
        match counts.iter_mut().find(|c| c.exercise == name) {
            Some(c) => {
                c.reps += s.reps.unwrap_or(0);
                // Running mean, so the count doesn't need a second pass.
                c.confidence = (c.confidence * c.sets as f64
                    + s.exercise_confidence.unwrap_or(0.0))
                    / (c.sets + 1) as f64;
                c.sets += 1;
            }
            None => counts.push(ExerciseCount {
                exercise: name.to_string(),
                sets: 1,
                reps: s.reps.unwrap_or(0),
                confidence: s.exercise_confidence.unwrap_or(0.0),
            }),
        }
    }
    counts.sort_by(|a, b| {
        b.sets
            .cmp(&a.sets)
            .then_with(|| a.exercise.cmp(&b.exercise))
    });

    StrengthSession {
        activity_id: sets.first().map(|s| s.activity_id).unwrap_or_default(),
        work_sets: work.len(),
        total_reps,
        work_s,
        rest_s,
        work_rest_ratio: (rest_s > 0.0).then(|| work_s / rest_s),
        median_rest_s,
        avg_reps_per_set: (!work.is_empty()).then(|| total_reps as f64 / work.len() as f64),
        unlabelled_sets: work.iter().filter(|s| s.exercise.is_none()).count(),
        guessed_exercises: counts,
        ..Default::default()
    }
}

/// Whether an activity type is one that carries exercise sets.
///
/// Garmin files these under a handful of keys; `indoor_cardio` and the rest
/// never have sets, so asking for them would be one wasted request per activity
/// on every sync.
pub fn is_strength(type_key: Option<&str>) -> bool {
    let Some(k) = type_key else { return false };
    k.contains("strength") || k == "indoor_climbing" || k.contains("pilates") || k.contains("yoga")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Value {
        json!({
            "activityId": 23888334352_i64,
            "exerciseSets": [
                { "messageIndex": 0, "setType": "ACTIVE", "duration": 36.577,
                  "repetitionCount": 10, "weight": null, "startTime": "2026-08-07T14:25:06.0",
                  "exercises": [
                      { "category": "BENCH_PRESS", "probability": 74.6 },
                      { "category": "SHOULDER_PRESS", "probability": 74.6 },
                      { "category": "UNKNOWN", "probability": 24.6 }
                  ]},
                { "messageIndex": 1, "setType": "REST", "duration": 239.845,
                  "repetitionCount": null, "exercises": [] },
                { "messageIndex": 2, "setType": "ACTIVE", "duration": 55.724,
                  "repetitionCount": 2, "exercises": [
                      { "category": "UNKNOWN", "probability": 94.5 },
                      { "category": "BENCH_PRESS", "probability": 4.6 }
                  ]},
                { "messageIndex": 3, "setType": "REST", "duration": 274.817,
                  "repetitionCount": null, "exercises": [] }
            ]
        })
    }

    #[test]
    fn rest_entries_are_not_counted_as_sets() {
        let sets = parse_sets(23888334352, &payload());
        assert_eq!(sets.len(), 4);
        let s = summarise(&sets);
        assert_eq!(s.work_sets, 2);
        assert_eq!(s.total_reps, 12);
    }

    #[test]
    fn a_low_confidence_guess_is_dropped_rather_than_shown() {
        let sets = parse_sets(1, &payload());
        // Set 2's best real candidate is 4.6% — well under the floor. UNKNOWN
        // never counts however confident it is.
        assert_eq!(sets[2].exercise, None);
    }

    #[test]
    fn a_tie_between_two_movements_is_reported_as_no_guess() {
        // The watch gave BENCH_PRESS and SHOULDER_PRESS the same probability,
        // which means it could not tell them apart. Neither is the answer.
        let sets = parse_sets(1, &payload());
        assert_eq!(sets[0].exercise, None);

        let s = summarise(&sets);
        assert!(s.guessed_exercises.is_empty());
        assert_eq!(s.unlabelled_sets, 2);
    }

    #[test]
    fn a_clear_winner_is_carried_through_with_its_confidence() {
        let v = json!({ "exerciseSets": [
            { "messageIndex": 0, "setType": "ACTIVE", "duration": 40.0, "repetitionCount": 8,
              "exercises": [
                  { "category": "CURL", "probability": 88.0 },
                  { "category": "BENCH_PRESS", "probability": 12.0 }
              ]}
        ]});
        let sets = parse_sets(1, &v);
        assert_eq!(sets[0].exercise.as_deref(), Some("CURL"));
        assert_eq!(sets[0].exercise_confidence, Some(88.0));

        let s = summarise(&sets);
        assert_eq!(s.guessed_exercises[0].sets, 1);
        assert_eq!(s.guessed_exercises[0].reps, 8);
        assert_eq!(s.unlabelled_sets, 0);
    }

    #[test]
    fn weight_stays_null_when_garmin_sends_none() {
        let sets = parse_sets(1, &payload());
        assert!(sets.iter().all(|s| s.weight_kg.is_none()));
    }

    #[test]
    fn rest_is_a_median_not_a_mean() {
        let sets = parse_sets(1, &payload());
        let s = summarise(&sets);
        // Two rests: (239.845 + 274.817) / 2.
        assert!((s.median_rest_s.unwrap() - 257.331).abs() < 0.01);
        assert!(s.work_rest_ratio.unwrap() < 0.2);
    }

    #[test]
    fn an_empty_payload_summarises_to_nothing_rather_than_panicking() {
        let sets = parse_sets(1, &json!({}));
        assert!(sets.is_empty());
        let s = summarise(&sets);
        assert_eq!(s.work_sets, 0);
        assert_eq!(s.median_rest_s, None);
        assert_eq!(s.work_rest_ratio, None);
    }
}
