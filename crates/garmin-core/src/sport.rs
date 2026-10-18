//! One table that sorts a Garmin type key into a sport family.
//!
//! The family decides which section, unit and judgement a screen shows for a
//! session. It builds on [`crate::analysis::discipline`], which decides what a
//! zone reading can say about the session. Garmin adds type keys over time, so
//! an unknown key falls to [`Family::Mixed`] and never to a run-only judgement.

use crate::analysis::{discipline, Discipline};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Family {
    /// Continuous distance sports: run, walk, hike, ride, row, elliptical.
    Endurance,
    /// Strength, HIIT, jump rope, climbing, indoor cardio.
    Gym,
    Golf,
    /// Swim, paddle, surf.
    Water,
    /// Yoga, pilates, breathwork, stretching.
    MindBody,
    /// Any other key, and a session with no key.
    Mixed,
    /// A ride or a drive that is not exercise. Left out of load and counts.
    NonExercise,
}

/// First match wins, so a narrow key goes above a wide one.
const TABLE: &[(&str, Family)] = &[
    ("motorcycl", Family::NonExercise),
    ("driving", Family::NonExercise),
    ("atv", Family::NonExercise),
    ("golf", Family::Golf),
    ("swim", Family::Water),
    ("paddl", Family::Water),
    ("surf", Family::Water),
    ("yoga", Family::MindBody),
    ("pilates", Family::MindBody),
    ("breath", Family::MindBody),
    ("stretch", Family::MindBody),
    ("running", Family::Endurance),
    ("walk", Family::Endurance),
    ("hik", Family::Endurance),
    ("cycling", Family::Endurance),
    ("biking", Family::Endurance),
    ("rowing", Family::Endurance),
    ("elliptical", Family::Endurance),
    ("stair", Family::Endurance),
    ("strength", Family::Gym),
    ("hiit", Family::Gym),
    ("jump_rope", Family::Gym),
    ("crossfit", Family::Gym),
    ("climbing", Family::Gym),
    ("bouldering", Family::Gym),
    ("cardio", Family::Gym),
];

/// The family of a type key. Unknown and missing keys are `Mixed`.
pub fn family(type_key: Option<&str>) -> Family {
    family_with(type_key, &[])
}

/// Same as [`family`], with a per-key override from the user's settings. An
/// exact key in `overrides` wins over the table. No setting calls this yet.
pub fn family_with(type_key: Option<&str>, overrides: &[(&str, Family)]) -> Family {
    let k = type_key.unwrap_or("");
    if let Some((_, f)) = overrides.iter().find(|(key, _)| *key == k) {
        return *f;
    }
    TABLE
        .iter()
        .find(|(part, _)| k.contains(part))
        .map_or(Family::Mixed, |(_, f)| *f)
}

/// Whether the session counts as exercise.
pub fn is_exercise(type_key: Option<&str>) -> bool {
    family(type_key) != Family::NonExercise
}

/// Whether a zone verdict ("above Z2") says anything true about the session.
pub fn shows_zone_verdict(type_key: Option<&str>) -> bool {
    matches!(
        discipline(type_key),
        Discipline::Paced | Discipline::Endurance
    ) && is_exercise(type_key)
}

/// A readable name for a type key: `treadmill_running` is "Treadmill running"
/// and `motorcycling_v2` is "Motorcycling". No key gives "Activity".
pub fn sport_label(type_key: Option<&str>) -> String {
    let key = type_key.filter(|k| !k.is_empty()).unwrap_or("activity");
    let base = match key.rsplit_once("_v") {
        Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => key,
    };
    let text = base.replace('_', " ");
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_keys_map_to_their_family() {
        let cases = [
            ("running", Family::Endurance),
            ("treadmill_running", Family::Endurance),
            ("trail_running", Family::Endurance),
            ("walking", Family::Endurance),
            ("hiking", Family::Endurance),
            ("cycling", Family::Endurance),
            ("indoor_cycling", Family::Endurance),
            ("mountain_biking", Family::Endurance),
            ("indoor_rowing", Family::Endurance),
            ("elliptical", Family::Endurance),
            ("stair_climbing", Family::Endurance),
            ("strength_training", Family::Gym),
            ("hiit", Family::Gym),
            ("jump_rope", Family::Gym),
            ("indoor_climbing", Family::Gym),
            ("bouldering", Family::Gym),
            ("indoor_cardio", Family::Gym),
            ("golf", Family::Golf),
            ("lap_swimming", Family::Water),
            ("open_water_swimming", Family::Water),
            ("stand_up_paddleboarding_v2", Family::Water),
            ("yoga", Family::MindBody),
            ("pilates", Family::MindBody),
            ("motorcycling_v2", Family::NonExercise),
            ("tennis_v2", Family::Mixed),
            ("other", Family::Mixed),
        ];
        for (key, want) in cases {
            assert_eq!(family(Some(key)), want, "{key}");
        }
    }

    #[test]
    fn an_override_wins_over_the_table() {
        let over = [("motorcycling_v2", Family::Mixed)];
        assert_eq!(family_with(Some("motorcycling_v2"), &over), Family::Mixed);
        assert_eq!(family_with(Some("golf"), &over), Family::Golf);
    }

    #[test]
    fn unknown_and_missing_keys_are_mixed() {
        assert_eq!(family(Some("quidditch_v9")), Family::Mixed);
        assert_eq!(family(None), Family::Mixed);
        assert!(is_exercise(Some("quidditch_v9")));
    }

    #[test]
    fn only_continuous_exercise_gets_a_zone_verdict() {
        assert!(shows_zone_verdict(Some("running")));
        assert!(shows_zone_verdict(Some("cycling")));
        assert!(!shows_zone_verdict(Some("strength_training")));
        assert!(!shows_zone_verdict(Some("golf")));
        assert!(!shows_zone_verdict(Some("motorcycling_v2")));
        assert!(!shows_zone_verdict(None));
    }

    #[test]
    fn labels_are_readable() {
        assert_eq!(sport_label(Some("treadmill_running")), "Treadmill running");
        assert_eq!(sport_label(Some("motorcycling_v2")), "Motorcycling");
        assert_eq!(sport_label(Some("golf")), "Golf");
        assert_eq!(sport_label(Some("video_game")), "Video game");
        assert_eq!(sport_label(None), "Activity");
    }
}
