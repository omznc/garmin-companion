//! The headline of the Today screen: a short verdict and one sentence below it.
//!
//! The text is built here so that the desktop and the mobile app draw the same
//! words. It reads training readiness, resting heart rate, HRV, sleep and the
//! recent load. It names a sport only for a session that the cache holds, so a
//! user with no runs never reads about a run.

use crate::text::hours_minutes;
use serde::{Deserialize, Serialize};

/// Readiness below this is low, and from [`READY_FROM`] up it is good.
const READINESS_LOW: f64 = 40.0;
const READY_FROM: f64 = 70.0;
/// A resting heart rate this far above the baseline is a strain signal.
const RHR_HIGH_BPM: f64 = 4.0;
/// A resting heart rate within this of the baseline is normal.
const RHR_NORMAL_BPM: f64 = 1.0;
/// Sleep under this is short, and from [`SLEEP_GOOD_SECS`] up it is enough.
const SLEEP_SHORT_SECS: f64 = 6.0 * 3600.0;
const SLEEP_GOOD_SECS: f64 = 7.0 * 3600.0;
/// An acute to chronic load ratio above this is a spike.
const LOAD_SPIKE: f64 = 1.5;

/// How hard the day should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeadlineTone {
    /// Two signals or more point to strain.
    Rest,
    /// One signal points to strain.
    Easy,
    /// Two good signals or more and none against.
    Go,
    /// One good signal and none against.
    Steady,
    /// Nothing to read.
    NoData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Headline {
    pub tone: HeadlineTone,
    /// A few words, for example "Take it easy today."
    pub verdict: String,
    /// One sentence, or two, that says why.
    pub detail: String,
}

/// The HRV of last night against the balanced band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HrvRead {
    pub value: f64,
    pub balanced_low: Option<f64>,
    pub balanced_high: Option<f64>,
}

/// What the headline reads. Every part may be missing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeadlineInput {
    pub readiness: Option<f64>,
    /// Resting heart rate minus its baseline, in bpm.
    pub resting_delta: Option<f64>,
    pub hrv: Option<HrvRead>,
    pub sleep_secs: Option<f64>,
    /// The acute to chronic load ratio.
    pub load_ratio: Option<f64>,
    /// The name of yesterday's hard session, for example "Strength training".
    /// `None` when yesterday had none.
    pub hard_yesterday: Option<String>,
}

impl HeadlineInput {
    /// Whether any reading is present. A hard session alone is not one.
    fn has_reading(&self) -> bool {
        self.readiness.is_some()
            || self.resting_delta.is_some()
            || self.hrv.is_some()
            || self.sleep_secs.is_some()
            || self.load_ratio.is_some()
    }
}

/// One signal, with the words that name it.
struct Signal {
    against: bool,
    text: String,
}

fn against(text: impl Into<String>) -> Option<Signal> {
    Some(Signal {
        against: true,
        text: text.into(),
    })
}

fn good(text: impl Into<String>) -> Option<Signal> {
    Some(Signal {
        against: false,
        text: text.into(),
    })
}

fn readiness_signal(r: f64) -> Option<Signal> {
    let text = format!("Garmin puts your readiness at {r:.0}");
    if r < READINESS_LOW {
        against(text)
    } else if r >= READY_FROM {
        good(text)
    } else {
        None
    }
}

fn resting_signal(delta: f64) -> Option<Signal> {
    if delta >= RHR_HIGH_BPM {
        against(format!(
            "your resting heart rate is {delta:.0} bpm above your baseline"
        ))
    } else if delta <= RHR_NORMAL_BPM {
        good("your resting heart rate is at your baseline or below")
    } else {
        None
    }
}

fn hrv_signal(h: HrvRead) -> Option<Signal> {
    match (h.balanced_low, h.balanced_high) {
        (Some(low), _) if h.value < low => against("your HRV is below its balanced range"),
        (_, Some(high)) if h.value > high => good("your HRV is above its balanced range"),
        (Some(_), _) => good("your HRV is in its balanced range"),
        _ => None,
    }
}

fn sleep_signal(secs: f64) -> Option<Signal> {
    let text = format!("you slept {}", hours_minutes(secs));
    if secs < SLEEP_SHORT_SECS {
        against(text)
    } else if secs >= SLEEP_GOOD_SECS {
        good(text)
    } else {
        None
    }
}

fn load_signal(ratio: f64) -> Option<Signal> {
    (ratio > LOAD_SPIKE).then(|| Signal {
        against: true,
        text: "your load this week is high against your usual".into(),
    })
}

fn signals(i: &HeadlineInput) -> Vec<Signal> {
    [
        i.readiness.and_then(readiness_signal),
        i.resting_delta.and_then(resting_signal),
        i.hrv.and_then(hrv_signal),
        i.sleep_secs.and_then(sleep_signal),
        i.load_ratio.and_then(load_signal),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// `a`, `a and b`, `a, b and c`, with a capital and a full stop.
fn sentence(parts: &[&str]) -> String {
    let joined = match parts {
        [] => return String::new(),
        [one] => (*one).to_owned(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    };
    let mut chars = joined.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>());
    format!("{}{}.", first.unwrap_or_default(), chars.as_str())
}

fn texts(signals: &[Signal], against: bool) -> Vec<&str> {
    signals
        .iter()
        .filter(|s| s.against == against)
        .map(|s| s.text.as_str())
        .collect()
}

fn verdict(tone: HeadlineTone) -> &'static str {
    match tone {
        HeadlineTone::Rest => "Take it easy today.",
        HeadlineTone::Easy => "Go lighter today.",
        HeadlineTone::Go => "Recovered. Go ahead as planned.",
        HeadlineTone::Steady => "Nothing holds you back today.",
        HeadlineTone::NoData => "Not enough data yet.",
    }
}

/// Build the headline from what the cache knows.
pub fn headline(input: &HeadlineInput) -> Headline {
    let all = signals(input);
    let (bad, ok) = (texts(&all, true), texts(&all, false));
    let tone = match (bad.len(), ok.len()) {
        (0, 0) if input.has_reading() => HeadlineTone::Steady,
        (0, 0) => HeadlineTone::NoData,
        (0, 1) => HeadlineTone::Steady,
        (0, _) => HeadlineTone::Go,
        (1, _) => HeadlineTone::Easy,
        _ => HeadlineTone::Rest,
    };
    let mut detail = if tone == HeadlineTone::NoData {
        "Sync your watch, then check again. Your numbers appear here once they are in.".to_owned()
    } else if all.is_empty() {
        "Your numbers are in their usual range.".to_owned()
    } else if bad.is_empty() {
        sentence(&ok)
    } else {
        // What is against comes first. One good sign is worth a second sentence.
        [
            sentence(&bad),
            sentence(&ok.iter().take(1).copied().collect::<Vec<_>>()),
        ]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
    };
    if let (Some(sport), true) = (&input.hard_yesterday, tone != HeadlineTone::NoData) {
        detail.push_str(&format!(" Yesterday's {} was hard.", sport.to_lowercase()));
    }
    Headline {
        tone,
        verdict: verdict(tone).into(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f64 = 3600.0;

    fn read(input: HeadlineInput) -> Headline {
        let h = headline(&input);
        // The words are plain for every user: no markup, one verdict per tone.
        assert!(h.verdict.ends_with('.') && h.detail.ends_with('.'), "{h:?}");
        h
    }

    fn has_word(h: &Headline, word: &str) -> bool {
        format!("{} {}", h.verdict, h.detail)
            .to_lowercase()
            .contains(word)
    }

    #[test]
    fn a_runner_with_a_high_heart_rate_and_short_sleep_is_told_to_rest() {
        let h = read(HeadlineInput {
            readiness: Some(35.0),
            resting_delta: Some(6.0),
            hrv: Some(HrvRead {
                value: 50.0,
                balanced_low: Some(45.0),
                balanced_high: Some(65.0),
            }),
            sleep_secs: Some(5.5 * H),
            load_ratio: Some(1.2),
            hard_yesterday: Some("Running".into()),
        });
        assert_eq!(h.tone, HeadlineTone::Rest);
        assert_eq!(h.verdict, "Take it easy today.");
        assert_eq!(
            h.detail,
            "Garmin puts your readiness at 35, your resting heart rate is 6 bpm above your baseline and you slept 5h 30m. Your HRV is in its balanced range. Yesterday's running was hard."
        );
    }

    #[test]
    fn a_gym_user_who_is_recovered_reads_no_run_word() {
        let h = read(HeadlineInput {
            readiness: Some(82.0),
            resting_delta: Some(-1.0),
            hrv: Some(HrvRead {
                value: 70.0,
                balanced_low: Some(45.0),
                balanced_high: Some(65.0),
            }),
            sleep_secs: Some(7.5 * H),
            load_ratio: Some(1.0),
            hard_yesterday: Some("Strength training".into()),
        });
        assert_eq!(h.tone, HeadlineTone::Go);
        assert_eq!(h.verdict, "Recovered. Go ahead as planned.");
        assert!(h
            .detail
            .starts_with("Garmin puts your readiness at 82, your resting heart rate"));
        assert!(h
            .detail
            .ends_with("Yesterday's strength training was hard."));
        assert!(!has_word(&h, "run") && !has_word(&h, "pace"));
    }

    #[test]
    fn a_golfer_with_one_bad_sign_is_told_to_go_lighter_and_no_run_word_shows() {
        let h = read(HeadlineInput {
            sleep_secs: Some(5.0 * H),
            resting_delta: Some(0.0),
            hard_yesterday: Some("Golf".into()),
            ..Default::default()
        });
        assert_eq!(h.tone, HeadlineTone::Easy);
        assert_eq!(h.verdict, "Go lighter today.");
        assert_eq!(h.detail, "You slept 5h 00m. Your resting heart rate is at your baseline or below. Yesterday's golf was hard.");
        assert!(!has_word(&h, "run"));
    }

    #[test]
    fn a_user_with_sleep_only_and_no_session_reads_about_sleep_alone() {
        let h = read(HeadlineInput {
            sleep_secs: Some(8.0 * H),
            ..Default::default()
        });
        assert_eq!(h.tone, HeadlineTone::Steady);
        assert_eq!(h.verdict, "Nothing holds you back today.");
        assert_eq!(h.detail, "You slept 8h 00m.");
        for word in ["run", "session", "workout", "yesterday"] {
            assert!(!has_word(&h, word), "{word}");
        }
    }

    #[test]
    fn a_user_with_nothing_cached_is_asked_to_sync_and_a_load_spike_counts_against() {
        let h = read(HeadlineInput::default());
        assert_eq!(h.tone, HeadlineTone::NoData);
        assert!(h.detail.starts_with("Sync your watch"));
        // A hard session means nothing when there is nothing else to read.
        let hard = read(HeadlineInput {
            hard_yesterday: Some("Golf".into()),
            ..Default::default()
        });
        assert!(!has_word(&hard, "golf"));

        let spike = read(HeadlineInput {
            load_ratio: Some(1.8),
            readiness: Some(75.0),
            ..Default::default()
        });
        assert_eq!(spike.tone, HeadlineTone::Easy);
        assert_eq!(
            spike.detail,
            "Your load this week is high against your usual. Garmin puts your readiness at 75."
        );
    }

    #[test]
    fn the_hrv_band_decides_and_a_middle_reading_says_nothing() {
        let band = |value| HrvRead {
            value,
            balanced_low: Some(45.0),
            balanced_high: Some(65.0),
        };
        let tone = |i: HeadlineInput| headline(&i).tone;
        assert_eq!(
            tone(HeadlineInput {
                hrv: Some(band(40.0)),
                ..Default::default()
            }),
            HeadlineTone::Easy
        );
        assert_eq!(
            tone(HeadlineInput {
                hrv: Some(band(70.0)),
                ..Default::default()
            }),
            HeadlineTone::Steady
        );
        // A reading with no verdict is a steady day, not a missing one.
        let middle = HeadlineInput {
            readiness: Some(55.0),
            sleep_secs: Some(6.5 * H),
            resting_delta: Some(2.5),
            ..Default::default()
        };
        assert_eq!(headline(&middle).tone, HeadlineTone::Steady);
        assert_eq!(
            headline(&middle).detail,
            "Your numbers are in their usual range."
        );
    }
}
