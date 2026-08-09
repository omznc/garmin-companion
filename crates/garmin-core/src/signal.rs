//! What the sensors can actually support, session by session.
//!
//! Everything else in this crate treats what Garmin sends as measurement. Most
//! of it is. But a wrist-worn optical sensor and a wrist-worn accelerometer
//! each have regimes where they produce a number that is confidently wrong
//! rather than absent, and this app's whole argument — zone splits, Z5 drift,
//! resting-HR trend — is built on exactly those numbers.
//!
//! Nothing here throws data away. A flagged session keeps its zone split and
//! stays in every aggregate; what it gains is a field saying how far to trust
//! it. Dropping suspect sessions would quietly reshape the trends this app
//! exists to report, and the athlete is better served by a number with a
//! caveat than by a gap they can't see.
//!
//! The three regimes, and why each is checkable from data already cached:
//!
//! - **Cadence lock.** The optical sensor picks up the blood-flow pulse the
//!   arm swing creates and reports that frequency as a heartbeat. The classic
//!   tell is heart rate tracking step rate, and both are stored per session and
//!   per sample. It matters most here: at 170 spm a locked reading reports HR
//!   170, which lands in this athlete's Z4 — so the hard-effort drift the coach
//!   is built around is precisely what a lock would counterfeit, and the
//!   counterfeit gets more convincing as the cadence goal is met.
//! - **Optical lag on short efforts.** The sensor smooths and delays rapid
//!   changes, so a short hard session's peaks are understated or missed. This
//!   athlete's stated tendency is 5–13 minute hard sessions.
//! - **Estimated indoor pace.** Treadmill speed comes off the arm
//!   accelerometer, not from GPS or the belt. Cadence and heart rate indoors
//!   are fine; distance and pace are an estimate, and comparing them across
//!   sessions compares two estimates.

use serde::{Deserialize, Serialize};

/* ---------------------------------------------------------------- thresholds --- */

/// How close heart rate has to sit to step rate to look like a lock.
///
/// Wide enough to catch a lock that drifts a beat or two, narrow enough that a
/// genuine coincidence — a real 170 bpm at 170 spm — has to be a near-exact one
/// before it's raised.
const LOCK_BPM: f64 = 6.0;

/// Share of a session's samples that must sit inside `LOCK_BPM` before a lock
/// is called likely rather than possible. Half, because a lock that comes and
/// goes across a session is still a lock and still ruins the zone split.
const LOCK_SAMPLE_SHARE: f64 = 0.5;

/// Minimum samples before the per-sample check is worth running at all.
const LOCK_MIN_SAMPLES: usize = 30;

/// Under this many minutes, optical lag is a material share of the session.
const SHORT_EFFORT_MINS: f64 = 15.0;

/// Garmin's own rule: under four hours of recorded sleep, resting heart rate
/// falls back to a daytime estimate rather than the overnight low.
const SLEEP_FOR_RHR_SECS: f64 = 4.0 * 3600.0;

/* ------------------------------------------------------------------- sports --- */

/// Sessions where the belt, not a satellite or the wrist, sets the pace.
pub fn is_indoor(type_key: Option<&str>) -> bool {
    let k = type_key.unwrap_or("");
    k.contains("treadmill") || k.contains("indoor")
}

/// Sports where the wrist sensor has been found not to give a usable heart
/// rate at all — resistance work and anything else built from short bursts
/// against a moving, loaded wrist.
pub fn wrist_hr_unreliable(type_key: Option<&str>) -> bool {
    let k = type_key.unwrap_or("");
    ["strength", "hiit", "cardio", "rope", "climb"]
        .iter()
        .any(|t| k.contains(t))
}

fn is_run(type_key: Option<&str>) -> bool {
    type_key.unwrap_or("").contains("running")
}

/* -------------------------------------------------------------- cadence lock --- */

/// How much the heart-rate trace looks like the cadence trace.
///
/// Deliberately four states rather than a bool. The averages alone can only
/// ever say "these two numbers are close", which is suggestive and not proof —
/// a real heart rate genuinely can sit on top of a real step rate. Only the
/// per-sample trace can show the two moving together, and only some sessions
/// have one cached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CadenceLock {
    /// No cadence, no heart rate, or not a sport where this happens.
    NotChecked,
    /// Checked and the two traces don't agree.
    Unlikely,
    /// The session averages sit on top of each other. Suggestive only: this is
    /// the weak check, and a coincidence looks identical to it.
    Possible,
    /// Most of the session's samples have heart rate within a few beats of
    /// step rate. A real heart rate does not shadow cadence for that long.
    Likely,
}

impl CadenceLock {
    pub fn suspected(self) -> bool {
        matches!(self, Self::Possible | Self::Likely)
    }

    /// See [`RestingHrSource::as_str`] for why these are written out.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotChecked => "notChecked",
            Self::Unlikely => "unlikely",
            Self::Possible => "possible",
            Self::Likely => "likely",
        }
    }
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Caution => "caution",
            Self::Poor => "poor",
        }
    }
}

/// The averages-only check, available for every cached activity.
///
/// Runs only for running: cadence lock is an artefact of arm swing at stride
/// frequency, and a cycling cadence of 90 sitting near a heart rate of 90 means
/// nothing at all.
pub fn cadence_lock_from_averages(
    type_key: Option<&str>,
    avg_hr: Option<f64>,
    avg_cadence: Option<f64>,
) -> CadenceLock {
    if !is_run(type_key) {
        return CadenceLock::NotChecked;
    }
    let (Some(hr), Some(cad)) = (avg_hr, avg_cadence) else {
        return CadenceLock::NotChecked;
    };
    if hr <= 0.0 || cad <= 0.0 {
        return CadenceLock::NotChecked;
    }
    if (hr - cad).abs() <= LOCK_BPM {
        CadenceLock::Possible
    } else {
        CadenceLock::Unlikely
    }
}

/// The per-sample check, for sessions whose trace is cached.
///
/// Returns the verdict and the share of paired samples that agreed, because the
/// share is the evidence and a verdict without it is an assertion.
pub fn cadence_lock_from_samples(
    type_key: Option<&str>,
    hr: &[Option<f64>],
    cadence: &[Option<f64>],
) -> (CadenceLock, Option<f64>) {
    if !is_run(type_key) {
        return (CadenceLock::NotChecked, None);
    }

    let paired: Vec<(f64, f64)> = hr
        .iter()
        .zip(cadence)
        .filter_map(|(h, c)| match (h, c) {
            // A zero cadence is a standing sample, not a step rate of nought,
            // and pairing it with a real heart rate would manufacture
            // disagreement out of a pause.
            (Some(h), Some(c)) if *h > 0.0 && *c > 0.0 => Some((*h, *c)),
            _ => None,
        })
        .collect();

    if paired.len() < LOCK_MIN_SAMPLES {
        return (CadenceLock::NotChecked, None);
    }

    let close = paired
        .iter()
        .filter(|(h, c)| (h - c).abs() <= LOCK_BPM)
        .count();
    let share = close as f64 / paired.len() as f64;

    let verdict = if share >= LOCK_SAMPLE_SHARE {
        CadenceLock::Likely
    } else {
        CadenceLock::Unlikely
    };
    (verdict, Some((share * 1000.0).round() / 10.0))
}

/* --------------------------------------------------------- heart-rate trust --- */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    /// Nothing about the session argues against reading the zone split as it
    /// stands.
    Good,
    /// Readable, with something worth saying alongside it.
    Caution,
    /// The zone split for this session should not carry an argument on its own.
    Poor,
}

/// How far to trust one session's heart-rate trace, and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HrConfidence {
    pub level: Confidence,
    pub cadence_lock: CadenceLock,
    /// Beats between average heart rate and average step rate. Small is the
    /// thing that raises the flag, so the figure travels with the verdict.
    pub cadence_gap_bpm: Option<f64>,
    /// Percentage of paired samples with heart rate inside `LOCK_BPM` of step
    /// rate, when a trace was available to check.
    pub cadence_agreement_pct: Option<f64>,
    /// Short enough that optical smoothing is a material share of it.
    pub short_effort: bool,
    /// A sport the wrist sensor is not valid for regardless of anything else.
    pub wrist_unreliable_sport: bool,
    /// One sentence per reason, written to be quoted to the athlete.
    pub notes: Vec<String>,
}

impl HrConfidence {
    /// Whether anything here argues against taking the zone split at face
    /// value. The single field a caller can branch on without reading the rest.
    pub fn caveated(&self) -> bool {
        !matches!(self.level, Confidence::Good)
    }
}

/// A session's paired heart-rate and cadence traces, sample for sample.
///
/// Both carry gaps, and the gaps don't line up — the sensor drops a beat where
/// the accelerometer doesn't, and a pause blanks cadence while heart rate keeps
/// being recorded. Pairing them is the caller's problem to hand over and this
/// module's to solve.
pub type Traces<'a> = (&'a [Option<f64>], &'a [Option<f64>]);

/// Judge one session's heart-rate trace.
///
/// `samples` is the per-sample `(hr, cadence)` trace where one is cached; the
/// averages-only check runs when it isn't. `has_hr` is false for a session that
/// recorded no heart rate at all, which is a different thing from an untrusted
/// one and is already reported separately.
pub fn hr_confidence(
    type_key: Option<&str>,
    duration_s: Option<f64>,
    avg_hr: Option<f64>,
    avg_cadence: Option<f64>,
    has_hr: bool,
    samples: Option<Traces<'_>>,
) -> HrConfidence {
    let mut notes = Vec::new();

    let (lock, agreement) = match samples {
        Some((hr, cad)) => match cadence_lock_from_samples(type_key, hr, cad) {
            // A trace too short or too sparse to judge still deserves the
            // averages check rather than nothing.
            (CadenceLock::NotChecked, _) => (
                cadence_lock_from_averages(type_key, avg_hr, avg_cadence),
                None,
            ),
            found => found,
        },
        None => (
            cadence_lock_from_averages(type_key, avg_hr, avg_cadence),
            None,
        ),
    };

    let gap = match (avg_hr, avg_cadence) {
        (Some(h), Some(c)) if is_run(type_key) && h > 0.0 && c > 0.0 => {
            Some(((h - c).abs() * 10.0).round() / 10.0)
        }
        _ => None,
    };

    match lock {
        CadenceLock::Likely => notes.push(format!(
            "Heart rate shadowed step rate for {}% of this session. That is the \
             signature of the wrist sensor locking onto arm swing rather than \
             pulse, and it makes the zone split for this run unsafe to argue \
             from. A chest strap is the only way to settle it.",
            agreement.unwrap_or_default(),
        )),
        CadenceLock::Possible => notes.push(format!(
            "Average heart rate and average cadence are {} bpm apart, close \
             enough to be the wrist sensor tracking arm swing instead of pulse. \
             It may equally be coincidence — one session can't tell the two \
             apart — but it is worth knowing before reading much into this \
             zone split.",
            gap.unwrap_or_default(),
        )),
        _ => {}
    }

    let short_effort =
        has_hr && is_run(type_key) && duration_s.is_some_and(|s| s / 60.0 < SHORT_EFFORT_MINS);
    if short_effort {
        notes.push(
            "Short session. Optical heart rate lags and smooths rapid changes, \
             so the peaks of a hard effort this brief are understated as often \
             as they are overstated — read the zone split as the shape of the \
             session rather than as minutes anyone could stand behind."
                .into(),
        );
    }

    let wrist_unreliable_sport = has_hr && wrist_hr_unreliable(type_key);
    if wrist_unreliable_sport {
        notes.push(
            "The wrist sensor has been found neither valid nor reliable for \
             average or maximum heart rate in resistance work, where the wrist \
             is loaded and moving. Whatever the zones say here, they are not a \
             measure of how hard this was."
                .into(),
        );
    }

    // `short_effort` deliberately does not move the level, though it is the
    // most common note here by far.
    //
    // Run against the real cache, the first version of this raised `caution` on
    // twenty of twenty-three runs, every one of them for the same sentence.
    // That is not a finding, it is this athlete's training: 5–13 minute hard
    // sessions are the style, and a flag that fires on nearly every session
    // discriminates nothing and teaches the eye to skip the flag that matters.
    //
    // So the level answers a narrower question — how far to discount *this*
    // session against the others — and only the two artefacts that genuinely
    // corrupt a number answer it. The short-effort note still travels in
    // `notes`, where the model can raise it when the peaks are the point;
    // the screen shows notes only on a caveated session, so it stays quiet
    // there.
    let level = if !has_hr {
        Confidence::Good
    } else if lock == CadenceLock::Likely || wrist_unreliable_sport {
        Confidence::Poor
    } else if lock == CadenceLock::Possible {
        Confidence::Caution
    } else {
        Confidence::Good
    };

    HrConfidence {
        level,
        cadence_lock: lock,
        cadence_gap_bpm: gap,
        cadence_agreement_pct: agreement,
        short_effort,
        wrist_unreliable_sport,
        notes,
    }
}

/* --------------------------------------------------------------- resting HR --- */

/// Where a day's resting heart rate came from.
///
/// Garmin reports one number under one name for two different measurements.
/// Worn overnight it is the lowest 30-minute average, which lands in deep sleep
/// and sits within about a beat of an ECG. Not worn overnight it is a rough
/// estimate from the lowest one-minute average of the waking day, which is a
/// weaker measurement of a different thing. Plotting both on one trend line —
/// which is what a resting-HR trend has been doing — mixes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RestingHrSource {
    /// Enough sleep recorded for Garmin's overnight figure.
    Overnight,
    /// Sleep was recorded but fell short of Garmin's four-hour minimum, so the
    /// reading is the daytime estimate.
    DaytimeEstimate,
    /// No sleep recorded at all. Most likely the watch was off the wrist
    /// overnight, which means the daytime estimate — but a sleep record that
    /// simply never synced looks the same from here, so this says what it
    /// knows and no more.
    Unverified,
}

impl RestingHrSource {
    /// Whether this reading belongs on the same trend line as an overnight one.
    pub fn comparable(self) -> bool {
        matches!(self, Self::Overnight)
    }

    /// The wire form. Spelled out here rather than derived so the MCP crate can
    /// carry it without this crate taking a `schemars` dependency — that one is
    /// pinned by `rmcp`, and the desktop app has no use for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overnight => "overnight",
            Self::DaytimeEstimate => "daytimeEstimate",
            Self::Unverified => "unverified",
        }
    }
}

pub fn resting_hr_source(sleep_secs: Option<f64>) -> RestingHrSource {
    match sleep_secs {
        Some(s) if s >= SLEEP_FOR_RHR_SECS => RestingHrSource::Overnight,
        Some(_) => RestingHrSource::DaytimeEstimate,
        None => RestingHrSource::Unverified,
    }
}

/* --------------------------------------------------------------- HRV window --- */

/// Whether a window holds enough nights for Garmin's HRV status to mean
/// anything.
///
/// Garmin needs roughly three weeks to establish a personal range and at least
/// four nights of wear a week to keep it approximately right. Below that the
/// status is still reported — it just isn't a statement about this athlete's
/// baseline any more, and "Balanced" from a watch worn twice is not the green
/// light it looks like.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HrvCoverage {
    /// Nights in the window carrying an overnight HRV reading.
    pub nights: usize,
    /// Days the window covers.
    pub window_days: usize,
    /// Nights per week, averaged across the window.
    pub nights_per_week: f64,
    /// True when the window clears Garmin's four-nights-a-week guidance.
    pub sufficient: bool,
}

/// Minimum nights of wear per week for a usable HRV baseline.
const HRV_NIGHTS_PER_WEEK: f64 = 4.0;

pub fn hrv_coverage(nights: usize, window_days: usize) -> HrvCoverage {
    let per_week = if window_days == 0 {
        0.0
    } else {
        (nights as f64 / window_days as f64) * 7.0
    };
    HrvCoverage {
        nights,
        window_days,
        nights_per_week: (per_week * 10.0).round() / 10.0,
        sufficient: per_week >= HRV_NIGHTS_PER_WEEK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(vals: &[f64]) -> Vec<Option<f64>> {
        vals.iter().copied().map(Some).collect()
    }

    /// The artefact this module exists for, at the cadence the athlete is being
    /// coached towards: a locked reading of 170 lands in their Z4, so the
    /// coaching thread's central worry is exactly what a lock counterfeits.
    #[test]
    fn heart_rate_shadowing_step_rate_is_called_a_likely_lock() {
        let hr = some(&[169.0; 60]);
        let cad = some(&[170.0; 60]);
        let (lock, share) = cadence_lock_from_samples(Some("treadmill_running"), &hr, &cad);
        assert_eq!(lock, CadenceLock::Likely);
        assert_eq!(share, Some(100.0));
    }

    /// A real heart rate crosses its cadence and keeps going; it doesn't sit on
    /// it. Sixty samples of genuine effort against a steady step rate must not
    /// read as an artefact.
    #[test]
    fn a_real_trace_that_merely_crosses_cadence_is_not_a_lock() {
        let hr = some(&(0..60).map(|i| 120.0 + i as f64).collect::<Vec<_>>());
        let cad = some(&[170.0; 60]);
        let (lock, share) = cadence_lock_from_samples(Some("treadmill_running"), &hr, &cad);
        assert_eq!(lock, CadenceLock::Unlikely);
        assert!(share.unwrap() < 50.0);
    }

    /// Cycling cadence near a cycling heart rate is arithmetic, not an
    /// artefact — the mechanism is arm swing at stride frequency.
    #[test]
    fn the_check_only_runs_for_running() {
        assert_eq!(
            cadence_lock_from_averages(Some("cycling"), Some(92.0), Some(90.0)),
            CadenceLock::NotChecked
        );
        assert_eq!(
            cadence_lock_from_averages(Some("treadmill_running"), Some(92.0), Some(90.0)),
            CadenceLock::Possible
        );
    }

    /// A pause samples a real heart rate against a cadence of zero. Counting
    /// those would manufacture disagreement and hide a lock.
    #[test]
    fn standing_samples_are_excluded_rather_than_counted_as_disagreement() {
        let mut hr = some(&[170.0; 40]);
        let mut cad = some(&[171.0; 40]);
        hr.extend(some(&[120.0; 40]));
        cad.extend(some(&[0.0; 40]));
        let (lock, share) = cadence_lock_from_samples(Some("running"), &hr, &cad);
        assert_eq!(lock, CadenceLock::Likely, "the walk break must not mask it");
        assert_eq!(share, Some(100.0));
    }

    /// Garmin's own four-hour rule, which is what separates the overnight
    /// figure from the daytime estimate wearing its name.
    #[test]
    fn resting_hr_provenance_follows_the_recorded_sleep() {
        assert_eq!(
            resting_hr_source(Some(7.5 * 3600.0)),
            RestingHrSource::Overnight
        );
        assert_eq!(
            resting_hr_source(Some(2.0 * 3600.0)),
            RestingHrSource::DaytimeEstimate
        );
        assert_eq!(resting_hr_source(None), RestingHrSource::Unverified);
        assert!(!RestingHrSource::Unverified.comparable());
    }

    #[test]
    fn hrv_coverage_is_measured_against_four_nights_a_week() {
        assert!(hrv_coverage(12, 21).sufficient, "4 a week clears it");
        assert!(!hrv_coverage(6, 21).sufficient, "2 a week does not");
        assert_eq!(hrv_coverage(12, 21).nights_per_week, 4.0);
    }

    /// A short hard run carries its note but keeps a clean level.
    ///
    /// This athlete's runs are nearly all 5–13 minutes, so letting the short
    /// session raise `caution` marked twenty of twenty-three real runs and made
    /// the flag worthless. The note is still there for the model; the level
    /// stays for the artefacts that actually corrupt a number.
    #[test]
    fn a_short_run_carries_its_note_without_spending_the_flag() {
        let c = hr_confidence(
            Some("treadmill_running"),
            Some(8.0 * 60.0),
            Some(168.0),
            Some(148.0),
            true,
            None,
        );
        assert_eq!(c.level, Confidence::Good, "the common case must stay quiet");
        assert!(c.short_effort);
        assert_eq!(c.cadence_lock, CadenceLock::Unlikely);
        assert_eq!(c.notes.len(), 1, "the note still travels to the model");
        assert!(!c.caveated());
    }

    /// The two that do move it, so the level can't quietly become decorative.
    #[test]
    fn only_the_artefacts_that_corrupt_a_number_move_the_level() {
        let locked = hr_confidence(
            Some("treadmill_running"),
            Some(40.0 * 60.0),
            Some(170.0),
            Some(169.0),
            true,
            None,
        );
        assert_eq!(
            locked.level,
            Confidence::Caution,
            "averages agree: possible"
        );

        let strength = hr_confidence(
            Some("strength_training"),
            Some(50.0 * 60.0),
            Some(120.0),
            None,
            true,
            None,
        );
        assert_eq!(strength.level, Confidence::Poor);
        assert!(strength.caveated());
    }
}
