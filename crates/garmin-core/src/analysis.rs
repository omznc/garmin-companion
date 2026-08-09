//! One session, read closely.
//!
//! Everything on the activity screen below the summary numbers comes from here:
//! the map geometry, the charts, the splits, and the list of things worth
//! noticing. The written summary comes from here too — the model is handed this
//! struct and asked to talk about it, so the prose and the pins on the map can
//! never disagree about what happened.
//!
//! That is the whole reason this is one function rather than analysis on the
//! screen and a separate prompt-shaped bundle for the model. A highlight is
//! computed once, from the samples, and then rendered twice.
//!
//! Nothing here invents a measurement. Every field is either something Garmin
//! sent or an arithmetic consequence of it, and anything that needs data the
//! session doesn't carry is simply absent — a treadmill run has no coordinates
//! and no elevation, and the honest output for one is a struct that says so.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::CachedActivity;

/* ------------------------------------------------------------------ shapes --- */

/// The sampled series, all aligned to the same index.
///
/// One array per column rather than an array of structs: the charts and the map
/// each want a single column end to end, and this crosses to the frontend as
/// JSON where four arrays of 400 numbers are a great deal smaller than 400
/// objects of four keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    /// Seconds from the start of the session, per sample.
    pub elapsed_s: Vec<Option<f64>>,
    pub hr: Vec<Option<f64>>,
    /// Minutes per kilometre. Null where the athlete was standing still, which
    /// is a pause rather than a 300 min/km lap.
    pub pace_min_km: Vec<Option<f64>>,
    pub cadence: Vec<Option<f64>>,
    pub elevation_m: Vec<Option<f64>>,
    /// Cumulative distance, which is what the kilometre marks on the map hang
    /// off — counting samples would put them wherever the sampling was densest.
    pub distance_m: Vec<Option<f64>>,
    /// Empty on any session Garmin recorded without a position fix. The map
    /// checks this before it draws anything.
    pub lat: Vec<Option<f64>>,
    pub lon: Vec<Option<f64>>,
}

impl Series {
    pub fn len(&self) -> usize {
        self.elapsed_s.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elapsed_s.is_empty()
    }

    /// Whether there are at least two fixes to draw a line between.
    pub fn has_track(&self) -> bool {
        self.lat.iter().filter(|v| v.is_some()).count() >= 2
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lap {
    pub index: usize,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    /// Minutes per kilometre, when the lap covered enough ground to have one.
    pub pace_min_km: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub elevation_gain_m: Option<f64>,
}

/// The five zone floors, in bpm, as Garmin had them for this session.
///
/// Per activity rather than per account on purpose: these move when the athlete
/// edits their max HR, and a session recorded under the old ladder should keep
/// being read against the ladder it was recorded under.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneProfile {
    /// Lower bound of Z1…Z5.
    pub floors: [f64; 5],
    pub secs: [f64; 5],
    pub percent: [f64; 5],
    /// Whether the floors came from Garmin or from the fallback ladder below.
    /// The screen never shows this; the model is told, so it can hedge.
    pub measured: bool,
    /// The same split, recomputed here by classifying every heart-rate sample
    /// against `floors`. `None` when the session has no usable trace.
    ///
    /// This app holds two independent answers to one question — Garmin's
    /// `secs`, and its own classifier — and until now compared them never. They
    /// should agree to within rounding. When they don't, either the ladder is
    /// wrong for this session or the trace and the totals describe different
    /// things, and both are worth knowing before quoting either to three
    /// significant figures.
    pub recomputed_percent: Option<[f64; 5]>,
    /// Largest per-zone gap between `percent` and `recomputed_percent`, in
    /// percentage points. Small is the expected case.
    pub max_disagreement_pct: Option<f64>,
}

/// How a highlight should read. Not a severity — a session can be worth
/// remarking on without anything being wrong with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    /// Went well, and the athlete should know which part.
    Good,
    /// Worth knowing, neither praise nor a warning.
    Note,
    /// A pattern that costs something if it keeps happening.
    Watch,
}

/// One thing worth saying about the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    /// Stable slug, so the screen can pick an icon and the model can be told
    /// what a highlight *is* without parsing its prose.
    pub kind: String,
    pub tone: Tone,
    pub title: String,
    pub detail: String,
    /// Where on the timeline this happened, in elapsed seconds. Present only
    /// when it happened somewhere in particular — "63% above Z2" is about the
    /// whole session and pins to nothing.
    pub at_s: Option<f64>,
    /// The end of the stretch, for highlights that cover one.
    pub until_s: Option<f64>,
}

impl Highlight {
    fn new(kind: &str, tone: Tone, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            tone,
            title: title.into(),
            detail: detail.into(),
            at_s: None,
            until_s: None,
        }
    }

    fn at(mut self, at_s: f64) -> Self {
        self.at_s = Some(at_s);
        self
    }

    fn span(mut self, from_s: f64, to_s: f64) -> Self {
        self.at_s = Some(from_s);
        self.until_s = Some(to_s);
        self
    }
}

/// This session against the recent ones like it.
///
/// Deltas are signed against the average, and every field is optional because
/// the comparison is only worth as much as the sessions behind it — a first
/// ever ride has nothing to be compared to and says so by being `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    /// How many earlier sessions of the same sport went into the averages.
    pub sessions: usize,
    pub avg_pace_min_km: Option<f64>,
    pub avg_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub avg_percent_above_z2: Option<f64>,
    pub avg_duration_s: Option<f64>,
    /// This session minus the average. Negative pace is faster.
    pub pace_delta: Option<f64>,
    pub hr_delta: Option<f64>,
    pub cadence_delta: Option<f64>,
    pub percent_above_z2_delta: Option<f64>,
}

/// Everything the activity screen and the summary prompt are built from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityAnalysis {
    pub activity_id: i64,
    pub name: Option<String>,
    pub type_key: Option<String>,
    /// What the type key means for how the session can be read. Nothing on the
    /// screen shows this; it exists so the model is told which of these numbers
    /// are a target and which are a side effect of the sport.
    pub discipline: Discipline,
    pub start_time_local: Option<String>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub moving_duration_s: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub elevation_gain_m: Option<f64>,
    pub calories: Option<f64>,
    pub aerobic_te: Option<f64>,
    pub anaerobic_te: Option<f64>,
    /// Minutes per kilometre over the whole session.
    pub pace_min_km: Option<f64>,
    /// The same, over moving time only. On a run/walk session the two are
    /// different questions and this is the one about the running.
    pub moving_pace_min_km: Option<f64>,
    /// How far the zone split below can be trusted. This is the one place the
    /// per-sample check can run — the trace is right here — so this verdict is
    /// stronger than the one a list view carries.
    pub hr_confidence: crate::signal::HrConfidence,
    /// True when pace and distance came off the arm accelerometer rather than
    /// GPS. Not the same as `indoor`, which is about whether there is a track
    /// to draw: an outdoor run with the GPS off is estimated too.
    pub pace_estimated: bool,
    pub zones: ZoneProfile,
    pub series: Series,
    pub laps: Vec<Lap>,
    pub highlights: Vec<Highlight>,
    pub comparison: Option<Comparison>,
    pub tags: Vec<String>,
    /// True when Garmin recorded no position at all — a treadmill, a rower, a
    /// strength session. The map says so instead of drawing an empty box.
    pub indoor: bool,
    pub computed_at: String,
}

/* ------------------------------------------------------------- thresholds --- */

/// Zone floors used when Garmin's per-activity ladder can't be fetched.
///
/// These are the account's own zones as configured, not a formula — a guessed
/// ladder that happens to be wrong would put every per-sample zone judgement
/// quietly out by one. `ZoneProfile::measured` records which of the two a
/// reading came from so nothing downstream has to assume.
const FALLBACK_FLOORS: [f64; 5] = [98.0, 118.0, 137.0, 157.0, 176.0];

/// Below this speed, in m/s, the athlete is standing still rather than moving
/// very slowly. Garmin keeps sampling through a pause and the resulting pace is
/// meaningless, so those samples carry no pace at all.
const STOPPED_MS: f64 = 0.4;

/// Above this, in m/s, a run is a run. Below it and still moving, it's a walk
/// break — the run/walk intervals this athlete is deliberately using, which is
/// worth counting rather than smoothing away. ~9:15/km.
const WALK_MS: f64 = 1.8;

/// The shortest slow stretch that counts as a walk break rather than a traffic
/// light or a lost fix, in seconds.
const MIN_WALK_BREAK_S: f64 = 20.0;

/// The shortest stretch at easy effort worth reporting as one, in seconds.
const MIN_EASY_STRETCH_S: f64 = 120.0;

/// Cadence a running stride is worth aiming at, in steps per minute. Quicker,
/// lighter steps matter more the heavier the athlete, which is the reason this
/// is called out at all.
const CADENCE_TARGET: f64 = 170.0;

/// Pace fade between halves, as a fraction, past which the session is worth
/// describing as having faded rather than having drifted.
const FADE_FRACTION: f64 = 0.08;

/// Aerobic decoupling past which the second half cost more heartbeats per unit
/// of speed than the first in a way that means something. Five per cent is the
/// conventional line.
const DECOUPLE_FRACTION: f64 = 0.05;

/// How many earlier sessions of the same sport the comparison averages over.
const COMPARE_SESSIONS: usize = 8;

/* ------------------------------------------------------------------- main --- */

/// Build the analysis.
///
/// `details` and `splits` are Garmin's raw payloads, `zone_buckets` is
/// `hrTimeInZones`; each may be absent, and the result degrades to whatever the
/// remaining inputs support. `recent` is earlier activities of any sport — the
/// same-sport filtering happens here, so the caller doesn't have to know what
/// counts as the same sport.
pub fn analyse(
    activity: &CachedActivity,
    details: Option<&Value>,
    splits: Option<&Value>,
    zone_buckets: Option<&Value>,
    recent: &[CachedActivity],
    tags: Vec<String>,
    now: &str,
) -> ActivityAnalysis {
    let series = details.map(extract_series).unwrap_or_default();
    let laps = splits.map(extract_laps).unwrap_or_default();
    let mut zones = zone_profile(activity, zone_buckets);
    // The second opinion on the split, from the same trace the screen draws.
    if let Some(recomputed) = recompute_zones(&series, &zones.floors) {
        let worst = (0..5)
            .map(|i| (recomputed[i] - zones.percent[i]).abs())
            .fold(0.0f64, f64::max);
        zones.max_disagreement_pct = Some((worst * 10.0).round() / 10.0);
        zones.recomputed_percent = Some(recomputed.map(|p| (p * 10.0).round() / 10.0));
    }
    let comparison = compare(activity, recent);

    let highlights = highlights(activity, &series, &laps, &zones, comparison.as_ref());

    ActivityAnalysis {
        activity_id: activity.activity_id,
        name: activity.name.clone(),
        type_key: activity.type_key.clone(),
        discipline: discipline(activity.type_key.as_deref()),
        start_time_local: activity.start_time_local.clone(),
        distance_m: activity.distance_m,
        duration_s: activity.duration_s,
        moving_duration_s: activity.moving_duration_s,
        avg_hr: activity.avg_hr,
        max_hr: activity.max_hr,
        avg_cadence: activity.avg_cadence,
        elevation_gain_m: activity.elevation_gain,
        calories: activity.calories,
        aerobic_te: activity.aerobic_te,
        anaerobic_te: activity.anaerobic_te,
        pace_min_km: activity.pace_min_per_km(),
        moving_pace_min_km: activity.moving_pace_min_per_km(),
        // The strong check: the whole heart-rate and cadence trace, rather
        // than the two averages a list view has to settle for.
        hr_confidence: crate::signal::hr_confidence(
            activity.type_key.as_deref(),
            activity.duration_s,
            activity.avg_hr,
            activity.avg_cadence,
            activity.zone_total_secs() > 0.0,
            Some((&series.hr, &series.cadence)),
        ),
        // A session with no position fix had its distance estimated whatever
        // its type key says, which catches an outdoor run recorded with the
        // GPS switched off as well as every treadmill session.
        pace_estimated: crate::signal::is_indoor(activity.type_key.as_deref())
            || !series.has_track(),
        indoor: !series.has_track(),
        zones,
        series,
        laps,
        highlights,
        comparison,
        tags,
        computed_at: now.to_string(),
    }
}

/// Anything Garmin classifies as running or walking — the sports a pace, a
/// cadence and a walk break all mean something for.
pub fn is_paced(type_key: Option<&str>) -> bool {
    let k = type_key.unwrap_or("");
    k.contains("running") || k.contains("walk") || k.contains("hik")
}

fn is_running(type_key: Option<&str>) -> bool {
    type_key.unwrap_or("").contains("running")
}

/// What kind of session this is, and so what it can honestly be judged by.
///
/// Garmin's type key says what the sport was called. This says which of the
/// readings on the page mean anything about it. Every zone verdict below —
/// the share of time at Z2, when the effort first left it, how long the longest
/// easy stretch ran — is a statement about continuous aerobic work. Applied to
/// a set of squats it measures the ninety seconds between them and reports a
/// well-executed easy run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Discipline {
    /// Running, walking, hiking: pace, cadence and zone discipline all hold.
    Paced,
    /// Cycling, swimming, rowing, the cardio machines. Continuous aerobic work
    /// with no pace worth comparing, so the zone reading holds and everything
    /// built on minutes per kilometre does not.
    Endurance,
    /// Strength, jump rope, HIIT, circuits. Work and rest alternating by
    /// design: the heart rate is meant to climb and fall, and the zone split is
    /// a description of the ratio between the two rather than a target hit or
    /// missed.
    Interval,
    /// Everything else — a tactical session, a motorcycle ride, whatever Garmin
    /// files under "other". Judged against the athlete's own recent sessions of
    /// the same kind, and on nothing borrowed from running.
    Other,
}

/// The sport, sorted into the four things that can be said about one.
pub fn discipline(type_key: Option<&str>) -> Discipline {
    let k = type_key.unwrap_or("");
    if is_paced(Some(k)) {
        Discipline::Paced
    } else if [
        "cycling",
        "biking",
        "swim",
        "rowing",
        "elliptical",
        "stair",
        "cardio",
    ]
    .iter()
    .any(|f| k.contains(f))
    {
        Discipline::Endurance
    } else if [
        "strength",
        "jump_rope",
        "hiit",
        "crossfit",
        "bouldering",
        "climbing",
    ]
    .iter()
    .any(|f| k.contains(f))
    {
        Discipline::Interval
    } else {
        Discipline::Other
    }
}

/// Whether continuous-aerobic reasoning — time at Z2, cardiac drift, the
/// longest easy stretch — describes this sport at all.
fn is_continuous(d: Discipline) -> bool {
    matches!(d, Discipline::Paced | Discipline::Endurance)
}

/* --------------------------------------------------------------- payloads --- */

/// Column index for whichever of `keys` Garmin actually sent.
fn column(details: &Value, keys: &[&str]) -> Option<usize> {
    details
        .get("metricDescriptors")?
        .as_array()?
        .iter()
        .find(|d| {
            d.get("key")
                .and_then(Value::as_str)
                .is_some_and(|k| keys.contains(&k))
        })
        .and_then(|d| d.get("metricsIndex"))
        .and_then(Value::as_u64)
        .map(|i| i as usize)
}

/// Garmin's `/details` payload is a column-oriented table: `metricDescriptors`
/// names each column and `activityDetailMetrics[].metrics` holds the rows. Pull
/// out the seven columns anything here reads, aligned by row.
fn extract_series(details: &Value) -> Series {
    let Some(rows) = details
        .get("activityDetailMetrics")
        .and_then(Value::as_array)
    else {
        return Series::default();
    };

    // A column that is present but entirely null is the same as an absent one,
    // and returning it would have the charts draw an empty axis.
    let read = |keys: &[&str], scale: fn(f64) -> Option<f64>| -> Vec<Option<f64>> {
        let Some(idx) = column(details, keys) else {
            return vec![None; rows.len()];
        };
        let out: Vec<Option<f64>> = rows
            .iter()
            .map(|row| {
                row.get("metrics")
                    .and_then(Value::as_array)
                    .and_then(|m| m.get(idx))
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite())
                    .and_then(scale)
            })
            .collect();
        if out.iter().all(Option::is_none) {
            vec![None; rows.len()]
        } else {
            out
        }
    };

    let keep = |v: f64| Some(v);

    Series {
        elapsed_s: read(&["sumElapsedDuration", "sumDuration"], keep),
        hr: read(&["directHeartRate"], |v| (v > 0.0).then_some(v)),
        // Garmin reports speed in m/s; every run here is read in minutes per
        // kilometre. A near-zero speed is a pause, not a very slow lap.
        pace_min_km: read(&["directSpeed"], |v| {
            (v > STOPPED_MS).then(|| 1000.0 / v / 60.0)
        }),
        cadence: read(&["directRunCadence", "directDoubleCadence"], |v| {
            (v > 0.0).then_some(v)
        }),
        elevation_m: read(&["directElevation"], keep),
        distance_m: read(&["sumDistance"], keep),
        lat: read(&["directLatitude"], |v| {
            // A fix at exactly (0, 0) is the Atlantic, and no session starts
            // there. Garmin emits it for a sample taken before lock.
            (v != 0.0 && v.abs() <= 90.0).then_some(v)
        }),
        lon: read(&["directLongitude"], |v| {
            (v != 0.0 && v.abs() <= 180.0).then_some(v)
        }),
    }
}

fn extract_laps(splits: &Value) -> Vec<Lap> {
    let Some(laps) = splits.get("lapDTOs").and_then(Value::as_array) else {
        return Vec::new();
    };
    let num = |l: &Value, k: &str| l.get(k).and_then(Value::as_f64).filter(|v| v.is_finite());

    laps.iter()
        .enumerate()
        .filter(|(_, l)| num(l, "duration").is_some_and(|d| d > 0.0))
        .map(|(i, l)| {
            let distance_m = num(l, "distance");
            let duration_s = num(l, "duration");
            Lap {
                index: l
                    .get("lapIndex")
                    .and_then(Value::as_u64)
                    .map(|v| v as usize)
                    .unwrap_or(i + 1),
                distance_m,
                duration_s,
                pace_min_km: match (distance_m, duration_s) {
                    // Below ten metres a "pace" is a rounding artefact of the
                    // final part-lap, not a speed anybody ran.
                    (Some(d), Some(t)) if d >= 10.0 => Some((t / 60.0) / (d / 1000.0)),
                    _ => None,
                },
                avg_hr: num(l, "averageHR"),
                max_hr: num(l, "maxHR"),
                avg_cadence: num(l, "averageRunCadence").or_else(|| num(l, "averageBikeCadence")),
                elevation_gain_m: num(l, "elevationGain"),
            }
        })
        .collect()
}

/// The zone ladder for this session, preferring Garmin's own boundaries.
fn zone_profile(activity: &CachedActivity, buckets: Option<&Value>) -> ZoneProfile {
    let mut floors = FALLBACK_FLOORS;
    let mut measured = false;

    if let Some(list) = buckets.and_then(Value::as_array) {
        let mut found = [None; 5];
        for b in list {
            let zone = b.get("zoneNumber").and_then(Value::as_u64).unwrap_or(0);
            let low = b
                .get("zoneLowBoundary")
                .and_then(Value::as_f64)
                .filter(|v| *v > 0.0);
            if (1..=5).contains(&zone) {
                found[zone as usize - 1] = low;
            }
        }
        // All five or none: a partial ladder would classify samples against a
        // mix of Garmin's boundaries and the fallback's, which is worse than
        // using one of them consistently.
        if found.iter().all(Option::is_some) {
            floors = found.map(Option::unwrap);
            measured = true;
        }
    }

    ZoneProfile {
        floors,
        secs: activity.zone_secs,
        percent: activity.zone_percentages(),
        measured,
        recomputed_percent: None,
        max_disagreement_pct: None,
    }
}

/// Classify every heart-rate sample against the ladder and report the split.
///
/// Weighted by the gap to the next sample rather than by sample count: Garmin's
/// sampling is irregular, and counting samples would let a densely-sampled
/// minute outvote a sparsely-sampled five.
fn recompute_zones(series: &Series, floors: &[f64; 5]) -> Option<[f64; 5]> {
    let n = series.len();
    if n < 2 || series.hr.iter().all(Option::is_none) {
        return None;
    }

    let mut secs = [0.0f64; 5];
    for i in 0..n {
        let (Some(hr), Some(t)) = (series.hr[i], series.elapsed_s[i]) else {
            continue;
        };
        if hr <= 0.0 {
            continue;
        }
        // The final sample has no interval after it and is dropped. One
        // sampling gap out of a whole session cannot move a percentage
        // meaningfully, and inventing a duration for it would.
        let dt = match series.elapsed_s.get(i + 1).and_then(|v| *v) {
            Some(next) if next > t => next - t,
            _ => continue,
        };
        secs[zone_of(hr, floors) - 1] += dt;
    }

    let total: f64 = secs.iter().sum();
    (total > 0.0).then(|| secs.map(|s| s / total * 100.0))
}

/// Which zone a heart rate falls in, 1-indexed. Below the Z1 floor is still Z1 —
/// the alternative is a "zone 0" that nothing else in the app knows about.
fn zone_of(hr: f64, floors: &[f64; 5]) -> usize {
    let mut z = 1;
    for (i, floor) in floors.iter().enumerate() {
        if hr >= *floor {
            z = i + 1;
        }
    }
    z
}

/* ------------------------------------------------------------ comparison --- */

/// Whether two activities are the same kind of session for comparison.
///
/// Matched on the sport rather than the exact type key so that a treadmill run
/// and an outdoor run compare — they are the same training stimulus, and this
/// athlete has almost nothing but treadmill runs to compare against.
fn same_sport(a: Option<&str>, b: Option<&str>) -> bool {
    let family = |k: Option<&str>| {
        let k = k.unwrap_or("");
        [
            "running", "walk", "hik", "cycling", "swim", "strength", "cardio",
        ]
        .into_iter()
        .find(|f| k.contains(f))
    };
    match (family(a), family(b)) {
        (Some(x), Some(y)) => x == y,
        // A sport with no family — jump rope, HIIT, whatever the watch has a
        // profile for and this list doesn't — is compared against itself by the
        // key. `other` is excluded on purpose: it is Garmin's word for
        // unclassified, and two unclassified sessions are not evidence about
        // each other.
        (None, None) => match (a, b) {
            (Some(x), Some(y)) => x == y && x != "other",
            _ => false,
        },
        _ => false,
    }
}

fn mean(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64)
}

fn compare(activity: &CachedActivity, recent: &[CachedActivity]) -> Option<Comparison> {
    let peers: Vec<&CachedActivity> = recent
        .iter()
        .filter(|r| {
            r.activity_id != activity.activity_id
                && same_sport(r.type_key.as_deref(), activity.type_key.as_deref())
        })
        // `recent` arrives newest-first, and an activity should be read against
        // what came before it rather than against sessions run afterwards.
        .filter(
            |r| match (&r.start_time_local, &activity.start_time_local) {
                (Some(theirs), Some(ours)) => theirs < ours,
                _ => true,
            },
        )
        .take(COMPARE_SESSIONS)
        .collect();

    if peers.is_empty() {
        return None;
    }

    let above_z2 = |a: &CachedActivity| {
        let p = a.zone_percentages();
        (a.zone_total_secs() > 0.0).then(|| p[2] + p[3] + p[4])
    };

    let paces: Vec<f64> = peers.iter().filter_map(|p| p.pace_min_per_km()).collect();
    let hrs: Vec<f64> = peers.iter().filter_map(|p| p.avg_hr).collect();
    let cadences: Vec<f64> = peers.iter().filter_map(|p| p.avg_cadence).collect();
    let hards: Vec<f64> = peers.iter().filter_map(|p| above_z2(p)).collect();
    let durations: Vec<f64> = peers.iter().filter_map(|p| p.duration_s).collect();

    let avg_pace = mean(&paces);
    let avg_hr = mean(&hrs);
    let avg_cadence = mean(&cadences);
    let avg_hard = mean(&hards);

    Some(Comparison {
        sessions: peers.len(),
        pace_delta: match (activity.pace_min_per_km(), avg_pace) {
            (Some(mine), Some(theirs)) => Some(mine - theirs),
            _ => None,
        },
        hr_delta: match (activity.avg_hr, avg_hr) {
            (Some(mine), Some(theirs)) => Some(mine - theirs),
            _ => None,
        },
        cadence_delta: match (activity.avg_cadence, avg_cadence) {
            (Some(mine), Some(theirs)) => Some(mine - theirs),
            _ => None,
        },
        percent_above_z2_delta: match (above_z2(activity), avg_hard) {
            (Some(mine), Some(theirs)) => Some(mine - theirs),
            _ => None,
        },
        avg_pace_min_km: avg_pace,
        avg_hr,
        avg_cadence,
        avg_percent_above_z2: avg_hard,
        avg_duration_s: mean(&durations),
    })
}

/* ------------------------------------------------------------- highlights --- */

/// "8:42" from decimal minutes.
fn pace_label(min_per_km: f64) -> String {
    let m = min_per_km.floor();
    let s = ((min_per_km - m) * 60.0).round();
    if s >= 60.0 {
        format!("{}:00", m as i64 + 1)
    } else {
        format!("{}:{:02}", m as i64, s as i64)
    }
}

/// "12:30" or "1:04:10" from seconds.
fn clock(secs: f64) -> String {
    let total = secs.max(0.0).round() as i64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Values paired with the elapsed second they were sampled at.
type Timed = Vec<(f64, f64)>;

/// Samples paired with the elapsed time they were taken at, dropping any that
/// carry neither. Several of the passes below need exactly this.
fn timed(series: &Series, values: &[Option<f64>]) -> Timed {
    series
        .elapsed_s
        .iter()
        .zip(values)
        .filter_map(|(t, v)| Some(((*t)?, (*v)?)))
        .collect()
}

/// Split a timed series in half by *time*, not by sample count — the two are
/// only the same when sampling is uniform, and Garmin's downsampling isn't.
fn halves(points: &[(f64, f64)]) -> Option<(Vec<f64>, Vec<f64>)> {
    let first = points.first()?.0;
    let last = points.last()?.0;
    if last - first < 240.0 {
        // Under four minutes there is no meaningful "second half" — a fade
        // measured over ninety seconds is noise with a name.
        return None;
    }
    let mid = (first + last) / 2.0;
    let (first, second): (Timed, Timed) = points.iter().copied().partition(|(t, _)| *t < mid);
    let a: Vec<f64> = first.into_iter().map(|(_, v)| v).collect();
    let b: Vec<f64> = second.into_iter().map(|(_, v)| v).collect();
    (!a.is_empty() && !b.is_empty()).then_some((a, b))
}

fn highlights(
    activity: &CachedActivity,
    series: &Series,
    laps: &[Lap],
    zones: &ZoneProfile,
    comparison: Option<&Comparison>,
) -> Vec<Highlight> {
    let mut out = Vec::new();
    let type_key = activity.type_key.as_deref();
    let discipline = discipline(type_key);

    // The split that matters: a session of continuous aerobic work is judged by
    // where the heart rate sat, and a session of sets and rests is not. Running
    // one of these over the other is how a strength session was told it looked
    // like a well-executed easy run.
    if is_continuous(discipline) {
        zone_highlights(activity, series, zones, &mut out);
        // Decoupling is heartbeats per unit of speed. On a session with no
        // speed worth the name — a gym floor, a tactical exercise wandering at
        // 57 min/km — the ratio is noise wearing the word "drift".
        drift_highlight(series, &mut out);
    } else {
        effort_highlights(activity, zones, laps, &mut out);
    }
    if is_paced(type_key) {
        pacing_highlights(series, laps, &mut out);
        walk_breaks(series, &mut out);
    }
    if is_running(type_key) {
        cadence_highlights(activity, series, &mut out);
    }
    climb_highlight(activity, series, &mut out);
    comparison_highlight(comparison, activity.duration_s, discipline, &mut out);
    if is_running(type_key) && !series.has_track() {
        out.push(Highlight::new(
            "no_gps",
            Tone::Note,
            "No GPS on this one",
            "Recorded without a position fix, so it can't contribute to VO2 max. \
             Garmin needs outdoor runs for that number to start moving."
                .to_string(),
        ));
    }

    out
}

/// Where the effort actually sat, and when it left Z2.
fn zone_highlights(
    activity: &CachedActivity,
    series: &Series,
    zones: &ZoneProfile,
    out: &mut Vec<Highlight>,
) {
    let total = activity.zone_total_secs();
    if total <= 0.0 {
        return;
    }
    let pct = zones.percent;
    let hard = pct[2] + pct[3] + pct[4];
    let z5 = pct[4];

    let tone = if hard < 20.0 {
        Tone::Good
    } else if hard < 45.0 {
        Tone::Note
    } else {
        Tone::Watch
    };
    let detail = if hard < 20.0 {
        format!(
            "{:.0}% of tracked time sat at Z2 or below. This is what an easy run \
             is supposed to look like.",
            100.0 - hard
        )
    } else if z5 >= 20.0 {
        format!(
            "{z5:.0}% of the session was in Z5 and {hard:.0}% above Z2. That is a \
             hard session, whatever it was labelled as."
        )
    } else {
        format!("{hard:.0}% of tracked time was above Z2, {z5:.0}% of it in Z5.")
    };
    out.push(Highlight::new(
        "zone_balance",
        tone,
        format!("{hard:.0}% above Z2"),
        detail,
    ));

    // When the effort left Z2 for the first time. Only interesting when it
    // happened early and the session went on for a while afterwards — that is
    // the going-out-too-hard pattern, and it doesn't apply to a session that
    // was meant to be hard from the gun.
    let hr = timed(series, &series.hr);
    if let (Some(first), Some(last)) = (hr.first(), hr.last()) {
        let span = last.0 - first.0;
        if span > 300.0 {
            if let Some((at, _)) = hr
                .iter()
                .find(|(_, v)| zone_of(*v, &zones.floors) >= 3)
                .copied()
            {
                let fraction = (at - first.0) / span;
                if fraction < 0.25 && hard > 25.0 {
                    out.push(
                        Highlight::new(
                            "early_hard",
                            Tone::Watch,
                            format!("Into Z3 by {}", clock(at - first.0)),
                            format!(
                                "Heart rate crossed into Z3 {} in, {:.0}% of the way through \
                                 the session. Going out that hard is what makes the rest of \
                                 the run cost more than it should.",
                                clock(at - first.0),
                                fraction * 100.0
                            ),
                        )
                        .at(at),
                    );
                }
            }
        }
    }

    // The longest unbroken stretch at easy effort. This is the thing being
    // built towards, so it gets named even when it's short.
    if let Some((from, to)) = longest_easy(series, zones) {
        let length = to - from;
        if length >= MIN_EASY_STRETCH_S {
            let tone = if length >= 1200.0 {
                Tone::Good
            } else {
                Tone::Note
            };
            // The span is on the series' own clock so the map can find it; the
            // prose counts from the start of the session, which is the only
            // clock the athlete has.
            let base = hr.first().map(|(t, _)| *t).unwrap_or(0.0);
            out.push(
                Highlight::new(
                    "longest_easy",
                    tone,
                    format!("{} unbroken in Z2", clock(length)),
                    format!(
                        "The longest stretch at Z2 or below ran from {} to {}. Building \
                         that number is the point of an easy run.",
                        clock(from - base),
                        clock(to - base)
                    ),
                )
                .span(from, to),
            );
        }
    }
}

/// Where the effort sat, for a session built out of work and rest.
///
/// Deliberately without a verdict. The endurance version of this reads a large
/// share of Z1 and Z2 as a well-judged easy session; between sets that share is
/// the rest, and nobody can act on being told their rest periods were nicely
/// paced. What replaces it is the structure the session did have — how many
/// rounds, how long they ran, and whether the rests kept up with them.
fn effort_highlights(
    activity: &CachedActivity,
    zones: &ZoneProfile,
    laps: &[Lap],
    out: &mut Vec<Highlight>,
) {
    if activity.zone_total_secs() > 0.0 {
        let pct = zones.percent;
        let hard = pct[2] + pct[3] + pct[4];
        let top = pct[3] + pct[4];
        out.push(Highlight::new(
            "effort_shape",
            Tone::Note,
            format!("{hard:.0}% above Z2"),
            format!(
                "Heart rate was above Z2 for {hard:.0}% of the tracked time and at Z4 or \
                 higher for {top:.0}%. In a session of work and rest that ratio is the \
                 shape of the sets, not a pace that was held or missed."
            ),
        ));
    }

    // A lap on this kind of session is a round or a set. One lap means the watch
    // was never lapped, which says nothing about whether the session had any.
    let mut rounds: Vec<f64> = laps
        .iter()
        .filter_map(|l| l.duration_s)
        .filter(|d| *d > 1.0)
        .collect();
    if rounds.len() >= 3 {
        rounds.sort_by(|a, b| a.total_cmp(b));
        let median = rounds[rounds.len() / 2];
        out.push(Highlight::new(
            "rounds",
            Tone::Note,
            format!("{} rounds", rounds.len()),
            format!(
                "The watch was lapped {} times: median {}, shortest {}, longest {}. \
                 Whatever they were, that is the structure this session actually had.",
                rounds.len(),
                clock(median),
                clock(rounds[0]),
                clock(rounds[rounds.len() - 1]),
            ),
        ));
    }

    // Whether the rests kept up with the work. Thirds rather than halves: the
    // middle of an interval session is heart rate on its way somewhere, and the
    // question being asked is about the two ends.
    let hrs: Vec<f64> = laps
        .iter()
        .filter_map(|l| l.avg_hr)
        .filter(|h| *h > 0.0)
        .collect();
    if hrs.len() >= 6 {
        let n = hrs.len() / 3;
        if let (Some(first), Some(last)) = (mean(&hrs[..n]), mean(&hrs[hrs.len() - n..])) {
            let rise = last - first;
            if rise >= 8.0 {
                out.push(Highlight::new(
                    "rounds_climbing",
                    Tone::Watch,
                    format!("Rounds ran {rise:.0} bpm hotter by the end"),
                    format!(
                        "The last {n} rounds averaged {last:.0} bpm against {first:.0} for \
                         the first {n}. The rests stopped clearing what the work was \
                         putting in — either they shortened or the rounds lengthened."
                    ),
                ));
            } else if rise.abs() < 4.0 {
                out.push(Highlight::new(
                    "rounds_holding",
                    Tone::Good,
                    "Rounds held their heart rate",
                    format!(
                        "The last rounds averaged {last:.0} bpm against {first:.0} for the \
                         first — the rests were long enough to keep paying for the work."
                    ),
                ));
            }
        }
    }
}

/// The longest run of consecutive samples at or below the Z2 ceiling.
///
/// Returned on the series' own clock, like every other `at_s` here — the map
/// looks a highlight up by matching it against `elapsed_s`, so a span measured
/// from the start of the session instead would pin in the wrong place on any
/// activity whose first sample isn't at zero.
fn longest_easy(series: &Series, zones: &ZoneProfile) -> Option<(f64, f64)> {
    let hr = timed(series, &series.hr);
    if hr.len() < 3 {
        return None;
    }

    // Every easy stretch, then the longest of them. Collecting first is cheaper
    // to read than threading a running best through the loop, and there are at
    // most as many stretches as there are samples.
    let mut stretches: Vec<(f64, f64)> = Vec::new();
    let mut start: Option<f64> = None;
    let mut prev = hr.first()?.0;

    for (t, v) in &hr {
        if zone_of(*v, &zones.floors) <= 2 {
            start.get_or_insert(*t);
        } else if let Some(from) = start.take() {
            stretches.push((from, prev));
        }
        prev = *t;
    }
    // A session that ends easy never hits the `else` above.
    if let Some(from) = start {
        stretches.push((from, prev));
    }

    // The longest, and the earliest of those when two tie — `max_by` would hand
    // back the last, which on a session of even intervals means the report
    // wanders to a different stretch for no reason the athlete can see.
    stretches.into_iter().fold(None, |best, span| match best {
        Some(b) if b.1 - b.0 >= span.1 - span.0 => Some(b),
        _ => Some(span),
    })
}

/// Whether the session held together, sped up, or fell apart.
fn pacing_highlights(series: &Series, laps: &[Lap], out: &mut Vec<Highlight>) {
    let paced = timed(series, &series.pace_min_km);
    if let Some((first, second)) = halves(&paced) {
        if let (Some(a), Some(b)) = (mean(&first), mean(&second)) {
            let change = (b - a) / a;
            if change <= -0.03 {
                out.push(Highlight::new(
                    "negative_split",
                    Tone::Good,
                    "Finished faster than you started",
                    format!(
                        "Second half averaged {}/km against {}/km for the first — \
                         {:.0}% quicker. That is the shape a well-judged session has.",
                        pace_label(b),
                        pace_label(a),
                        -change * 100.0
                    ),
                ));
            } else if change >= FADE_FRACTION {
                out.push(Highlight::new(
                    "fade",
                    Tone::Watch,
                    format!("{:.0}% slower in the second half", change * 100.0),
                    format!(
                        "First half averaged {}/km, second {}/km. A fade that size \
                         usually means the opening pace was borrowed rather than earned.",
                        pace_label(a),
                        pace_label(b)
                    ),
                ));
            }
        }
    }

    // Fastest and slowest full laps, which is the split people actually look
    // for. Only when there are enough of them for a spread to mean anything.
    let timed_laps: Vec<&Lap> = laps.iter().filter(|l| l.pace_min_km.is_some()).collect();
    if timed_laps.len() >= 3 {
        let fastest = timed_laps
            .iter()
            .min_by(|a, b| a.pace_min_km.unwrap().total_cmp(&b.pace_min_km.unwrap()))
            .unwrap();
        let slowest = timed_laps
            .iter()
            .max_by(|a, b| a.pace_min_km.unwrap().total_cmp(&b.pace_min_km.unwrap()))
            .unwrap();
        let spread = slowest.pace_min_km.unwrap() - fastest.pace_min_km.unwrap();
        // Under fifteen seconds a kilometre the laps are effectively even, and
        // naming a "fastest" one implies a variation that isn't there.
        if spread >= 0.25 {
            out.push(Highlight::new(
                "lap_spread",
                if spread > 1.5 {
                    Tone::Watch
                } else {
                    Tone::Note
                },
                format!("Lap {} was your quickest", fastest.index),
                format!(
                    "Lap {} ran {}/km and lap {} ran {}/km — {} a kilometre between \
                     your best and worst split.",
                    fastest.index,
                    pace_label(fastest.pace_min_km.unwrap()),
                    slowest.index,
                    pace_label(slowest.pace_min_km.unwrap()),
                    pace_label(spread)
                ),
            ));
        }
    }
}

/// Stretches spent walking, which for a run/walk session is the structure
/// rather than a failure.
fn walk_breaks(series: &Series, out: &mut Vec<Highlight>) {
    let speed: Vec<(f64, f64)> = series
        .elapsed_s
        .iter()
        .zip(&series.pace_min_km)
        .filter_map(|(t, p)| Some(((*t)?, (*p)?)))
        .collect();
    if speed.len() < 5 {
        return;
    }
    // Pace is in min/km; the walk threshold is a speed, so convert once.
    let walk_pace = 1000.0 / WALK_MS / 60.0;

    let mut breaks: Vec<(f64, f64)> = Vec::new();
    let mut start: Option<f64> = None;
    let mut prev = speed[0].0;
    for (t, p) in &speed {
        if *p > walk_pace {
            start.get_or_insert(*t);
        } else if let Some(from) = start.take() {
            if prev - from >= MIN_WALK_BREAK_S {
                breaks.push((from, prev));
            }
        }
        prev = *t;
    }
    if let Some(from) = start {
        if prev - from >= MIN_WALK_BREAK_S {
            breaks.push((from, prev));
        }
    }

    if breaks.is_empty() {
        return;
    }
    let total: f64 = breaks.iter().map(|(a, b)| b - a).sum();
    let base = speed[0].0;
    let longest = breaks
        .iter()
        .max_by(|x, y| (x.1 - x.0).total_cmp(&(y.1 - y.0)))
        .copied()
        .unwrap();

    out.push(
        Highlight::new(
            "walk_breaks",
            Tone::Note,
            format!(
                "{} walk {}",
                breaks.len(),
                if breaks.len() == 1 { "break" } else { "breaks" }
            ),
            format!(
                "{} of walking across {} {}, the longest {} starting at {}. Run/walk \
                 is a legitimate way to hold an easy heart rate for longer.",
                clock(total),
                breaks.len(),
                if breaks.len() == 1 {
                    "stretch"
                } else {
                    "stretches"
                },
                clock(longest.1 - longest.0),
                clock(longest.0 - base)
            ),
        )
        .span(longest.0, longest.1),
    );
}

/// Cadence against the target, and whether it held.
fn cadence_highlights(activity: &CachedActivity, series: &Series, out: &mut Vec<Highlight>) {
    let Some(avg) = activity.avg_cadence.filter(|c| *c > 0.0) else {
        return;
    };

    if avg < CADENCE_TARGET - 10.0 {
        out.push(Highlight::new(
            "cadence_low",
            Tone::Watch,
            format!("{avg:.0} spm average"),
            format!(
                "About {:.0} steps a minute short of the ~{CADENCE_TARGET:.0} worth \
                 aiming at. Quicker, lighter steps cut the load each one puts \
                 through the joints, which matters more the heavier you are.",
                CADENCE_TARGET - avg
            ),
        ));
    } else if avg >= CADENCE_TARGET - 5.0 {
        out.push(Highlight::new(
            "cadence_good",
            Tone::Good,
            format!("{avg:.0} spm average"),
            "Cadence is where you want it — quick steps, less time on the ground.".to_string(),
        ));
    }

    // A cadence that falls away through the session is a form fade, and it
    // shows up before pace does.
    let cadence = timed(series, &series.cadence);
    if let Some((first, second)) = halves(&cadence) {
        if let (Some(a), Some(b)) = (mean(&first), mean(&second)) {
            if a - b >= 6.0 {
                out.push(Highlight::new(
                    "cadence_fade",
                    Tone::Watch,
                    format!("Cadence dropped {:.0} spm", a - b),
                    format!(
                        "Averaged {a:.0} spm over the first half and {b:.0} over the \
                         second. Form going first is a sign the session ran past what \
                         you were fuelled or conditioned for."
                    ),
                ));
            }
        }
    }
}

/// Aerobic decoupling: whether the same speed cost more heartbeats later on.
fn drift_highlight(series: &Series, out: &mut Vec<Highlight>) {
    // Both columns, on the samples that carry both — a ratio built from a heart
    // rate at one moment and a speed at another measures nothing.
    let pairs: Vec<(f64, f64)> = series
        .elapsed_s
        .iter()
        .zip(&series.hr)
        .zip(&series.pace_min_km)
        .filter_map(|((t, h), p)| {
            let (t, h, p) = ((*t)?, (*h)?, (*p)?);
            (h > 0.0 && p > 0.0).then_some((t, (1.0 / p) / h))
        })
        .collect();

    let Some((first, second)) = halves(&pairs) else {
        return;
    };
    let (Some(a), Some(b)) = (mean(&first), mean(&second)) else {
        return;
    };
    if a <= 0.0 {
        return;
    }

    // Efficiency falling means each unit of speed cost more heartbeats.
    let decoupling = (a - b) / a;
    if decoupling >= DECOUPLE_FRACTION {
        out.push(Highlight::new(
            "cardiac_drift",
            Tone::Watch,
            format!("{:.0}% cardiac drift", decoupling * 100.0),
            format!(
                "The second half cost {:.0}% more heart rate for the same speed. Over \
                 that gap it's usually heat, fluid, or a starting pace above what the \
                 aerobic system could hold.",
                decoupling * 100.0
            ),
        ));
    } else if decoupling <= 0.02 && pairs.len() > 20 {
        out.push(Highlight::new(
            "coupled",
            Tone::Good,
            "Heart rate held with the pace",
            "Speed cost the same heartbeats at the end as at the start — the effort \
             was inside your aerobic ceiling the whole way."
                .to_string(),
        ));
    }
}

/// The biggest sustained climb, on sessions that actually climbed.
fn climb_highlight(activity: &CachedActivity, series: &Series, out: &mut Vec<Highlight>) {
    let Some(gain) = activity.elevation_gain.filter(|g| *g >= 50.0) else {
        return;
    };
    let elevation = timed(series, &series.elevation_m);
    if elevation.len() < 5 {
        return;
    }

    // Longest monotonic-ish rise: a run of samples with no drop worth more than
    // a metre, which is roughly the noise floor of a barometric altimeter.
    let mut best: Option<(f64, f64, f64)> = None; // (from, to, metres)
    let mut from = elevation[0].0;
    let mut low = elevation[0].1;
    let mut peak = elevation[0].1;

    for (t, e) in &elevation {
        if *e >= peak - 1.0 {
            peak = peak.max(*e);
            let rise = peak - low;
            if best.is_none_or(|(_, _, m)| rise > m) {
                best = Some((from, *t, rise));
            }
        } else {
            from = *t;
            low = *e;
            peak = *e;
        }
    }

    if let Some((start, end, rise)) = best {
        if rise >= 20.0 && end > start {
            let base = elevation[0].0;
            out.push(
                Highlight::new(
                    "climb",
                    Tone::Note,
                    format!("{rise:.0} m in one climb"),
                    format!(
                        "The longest continuous rise gained {rise:.0} m between {} and {}, \
                         out of {gain:.0} m for the session. Heart rate on a climb is \
                         reading the hill as much as the effort.",
                        clock(start - base),
                        clock(end - base)
                    ),
                )
                .span(start, end),
            );
        }
    }
}

/// This session set against the recent ones like it.
fn comparison_highlight(
    comparison: Option<&Comparison>,
    duration_s: Option<f64>,
    discipline: Discipline,
    out: &mut Vec<Highlight>,
) {
    let Some(c) = comparison else { return };

    // Only the largest single difference is reported. Four deltas listed in a
    // row is a table, and a table is what the numbers above the fold already are.
    let mut best: Option<(f64, Highlight)> = None;
    let mut offer = |weight: f64, h: Highlight| {
        if best.as_ref().is_none_or(|(w, _)| weight > *w) {
            best = Some((weight, h));
        }
    };

    if let (Some(delta), Some(avg)) = (c.percent_above_z2_delta, c.avg_percent_above_z2) {
        if delta.abs() >= 12.0 {
            // Above Z2 is drift on an easy run and it is the point of a set of
            // intervals, so the same delta is a warning in one sport and a
            // description in the other.
            let tone = match (is_continuous(discipline), delta < 0.0) {
                (true, true) => Tone::Good,
                (true, false) => Tone::Watch,
                (false, _) => Tone::Note,
            };
            let detail = if is_continuous(discipline) {
                format!(
                    "Your last {} sessions of this sport averaged {avg:.0}% above Z2. \
                     This one {} that.",
                    c.sessions,
                    if delta < 0.0 {
                        "came in under"
                    } else {
                        "went past"
                    }
                )
            } else {
                format!(
                    "Your last {} sessions of this sport averaged {avg:.0}% above Z2, so \
                     this one was {} than your usual — which is the only sense in which \
                     that number means anything here.",
                    c.sessions,
                    if delta < 0.0 { "easier" } else { "harder" }
                )
            };
            offer(
                delta.abs() / 12.0,
                Highlight::new(
                    "vs_recent_zones",
                    tone,
                    format!(
                        "{:.0} points {} above Z2 than usual",
                        delta.abs(),
                        if delta < 0.0 { "less" } else { "more" }
                    ),
                    detail,
                ),
            );
        }
    }

    // How long it ran, against how long these usually run. Offered only where
    // there is no pace and no zone target to judge by — on a run the pace row
    // and the zone row have already said more than this could.
    if !is_continuous(discipline) {
        if let (Some(dur), Some(avg)) = (duration_s, c.avg_duration_s) {
            // Twenty per cent, and never less than two minutes: a fifth of a
            // six-minute skipping session is seventy seconds, which is a warm-up
            // that ran long rather than a session that was cut short.
            let threshold = (avg * 0.2).max(120.0);
            let delta = dur - avg;
            if avg > 0.0 && delta.abs() >= threshold {
                offer(
                    delta.abs() / threshold,
                    Highlight::new(
                        "vs_recent_duration",
                        Tone::Note,
                        format!(
                            "{} {} than usual",
                            clock(delta.abs()),
                            if delta < 0.0 { "shorter" } else { "longer" }
                        ),
                        format!(
                            "Your last {} sessions of this sport averaged {}. This one ran {}.",
                            c.sessions,
                            clock(avg),
                            clock(dur)
                        ),
                    ),
                );
            }
        }
    }

    if let (Some(delta), Some(avg)) = (c.pace_delta, c.avg_pace_min_km) {
        if delta.abs() >= 0.4 {
            offer(
                delta.abs() / 0.4,
                Highlight::new(
                    "vs_recent_pace",
                    Tone::Note,
                    format!(
                        "{}/km {} than your recent average",
                        pace_label(delta.abs()),
                        if delta < 0.0 { "quicker" } else { "slower" }
                    ),
                    format!(
                        "The last {} sessions of this sport averaged {}/km.",
                        c.sessions,
                        pace_label(avg)
                    ),
                ),
            );
        }
    }

    if let (Some(delta), Some(avg)) = (c.cadence_delta, c.avg_cadence) {
        if delta.abs() >= 6.0 {
            offer(
                delta.abs() / 6.0,
                Highlight::new(
                    "vs_recent_cadence",
                    if delta > 0.0 { Tone::Good } else { Tone::Note },
                    format!(
                        "Cadence {:.0} spm {} than usual",
                        delta.abs(),
                        if delta > 0.0 { "higher" } else { "lower" }
                    ),
                    format!("Recent sessions of this sport averaged {avg:.0} spm."),
                ),
            );
        }
    }

    if let Some((_, h)) = best {
        out.push(h);
    }
}

/* -------------------------------------------------------------- key --- */

/// What an analysis was computed from.
///
/// Garmin's samples for a finished session never change, so a cached analysis
/// only needs to be thrown away when the cached summary it was built alongside
/// moves — a re-sync that corrects a duration, or a newly written tag.
pub fn fingerprint(activity: &CachedActivity, tags: &[String]) -> String {
    format!(
        "{}|{:?}|{:?}|{:?}|{:?}|{}",
        activity.activity_id,
        activity.duration_s,
        activity.distance_m,
        activity.avg_hr,
        activity.zone_secs,
        tags.join(","),
    )
}

/// Read a cached analysis back, tolerating a shape written by an older build.
pub fn decode(json: &str) -> Result<ActivityAnalysis> {
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity() -> CachedActivity {
        CachedActivity {
            activity_id: 1,
            name: Some("Evening Run".into()),
            type_key: Some("treadmill_running".into()),
            start_time_local: Some("2026-08-01 18:00:00".into()),
            local_date: Some("2026-08-01".into()),
            distance_m: Some(3000.0),
            duration_s: Some(1800.0),
            moving_duration_s: Some(1780.0),
            avg_hr: Some(150.0),
            max_hr: Some(180.0),
            avg_cadence: Some(150.0),
            calories: Some(300.0),
            elevation_gain: None,
            steps: Some(4000),
            aerobic_te: Some(3.0),
            anaerobic_te: Some(1.0),
            zone_secs: [60.0, 240.0, 900.0, 500.0, 100.0],
        }
    }

    /// A details payload in Garmin's column shape, with `n` samples.
    fn details(n: usize, with_gps: bool) -> Value {
        let mut descriptors = vec![
            serde_json::json!({ "key": "sumElapsedDuration", "metricsIndex": 0 }),
            serde_json::json!({ "key": "directHeartRate", "metricsIndex": 1 }),
            serde_json::json!({ "key": "directSpeed", "metricsIndex": 2 }),
        ];
        if with_gps {
            descriptors.push(serde_json::json!({ "key": "directLatitude", "metricsIndex": 3 }));
            descriptors.push(serde_json::json!({ "key": "directLongitude", "metricsIndex": 4 }));
        }
        let rows: Vec<Value> = (0..n)
            .map(|i| {
                let t = i as f64 * 10.0;
                // Second half runs hotter and slower, so the drift and fade
                // passes have something to find.
                let hot = i > n / 2;
                let hr = if hot { 165.0 } else { 140.0 };
                let speed = if hot { 2.2 } else { 2.8 };
                let mut metrics = vec![
                    serde_json::json!(t),
                    serde_json::json!(hr),
                    serde_json::json!(speed),
                ];
                if with_gps {
                    metrics.push(serde_json::json!(43.0 + i as f64 * 0.0001));
                    metrics.push(serde_json::json!(17.0 + i as f64 * 0.0001));
                }
                serde_json::json!({ "metrics": metrics })
            })
            .collect();
        serde_json::json!({
            "metricDescriptors": descriptors,
            "activityDetailMetrics": rows,
        })
    }

    #[test]
    fn a_session_without_samples_still_analyses() {
        let a = activity();
        let out = analyse(&a, None, None, None, &[], vec![], "now");
        assert!(out.indoor, "no coordinates means indoor");
        assert!(out.series.is_empty());
        // The zone breakdown comes off the cached summary, so it survives
        // having no series at all.
        assert!(
            out.highlights.iter().any(|h| h.kind == "zone_balance"),
            "zones are known without a series: {:?}",
            out.highlights.iter().map(|h| &h.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn series_columns_are_read_by_name_not_position() {
        let d = details(60, true);
        let s = extract_series(&d);
        assert_eq!(s.len(), 60);
        assert!(s.has_track());
        assert_eq!(s.hr[0], Some(140.0));
        // 2.8 m/s is a shade under 6 min/km.
        let pace = s.pace_min_km[0].unwrap();
        assert!((pace - 5.952).abs() < 0.01, "{pace}");
        // A column Garmin never sent is all-null rather than absent, so the
        // arrays stay aligned.
        assert_eq!(s.cadence.len(), 60);
        assert!(s.cadence.iter().all(Option::is_none));
    }

    #[test]
    fn a_pause_carries_no_pace() {
        let d = serde_json::json!({
            "metricDescriptors": [
                { "key": "sumDuration", "metricsIndex": 0 },
                { "key": "directSpeed", "metricsIndex": 1 },
            ],
            "activityDetailMetrics": [
                { "metrics": [0.0, 2.5] },
                { "metrics": [10.0, 0.0] },
                { "metrics": [20.0, 2.5] },
            ],
        });
        let s = extract_series(&d);
        assert!(s.pace_min_km[0].is_some());
        assert!(s.pace_min_km[1].is_none(), "standing still is not a pace");
    }

    #[test]
    fn drift_and_fade_are_found_when_they_happened() {
        let a = activity();
        let out = analyse(
            &a,
            Some(&details(120, false)),
            None,
            None,
            &[],
            vec![],
            "now",
        );
        let kinds: Vec<&str> = out.highlights.iter().map(|h| h.kind.as_str()).collect();
        assert!(kinds.contains(&"fade"), "{kinds:?}");
        assert!(kinds.contains(&"cardiac_drift"), "{kinds:?}");
        // Treadmill, so it should also say why VO2 max isn't moving.
        assert!(kinds.contains(&"no_gps"), "{kinds:?}");
    }

    /// The bug this exists to stop: a gym session sitting at Z1 and Z2 between
    /// sets was being told it was a well-executed easy run, given credit for an
    /// eight-minute unbroken Z2 stretch, and asked to hold its first ten
    /// minutes under 136 bpm.
    #[test]
    fn a_gym_session_is_not_read_as_an_easy_run() {
        let mut a = activity();
        a.name = Some("Strength".into());
        a.type_key = Some("strength_training".into());
        // The shape of a real one: mostly at or below Z2, because that is what
        // the ninety seconds between sets looks like.
        a.zone_secs = [800.0, 900.0, 200.0, 0.0, 0.0];

        let out = analyse(
            &a,
            Some(&details(120, false)),
            None,
            None,
            &[],
            vec![],
            "now",
        );
        let kinds: Vec<&str> = out.highlights.iter().map(|h| h.kind.as_str()).collect();

        assert_eq!(out.discipline, Discipline::Interval);
        for endurance_only in [
            "zone_balance",
            "longest_easy",
            "early_hard",
            "cardiac_drift",
            "fade",
        ] {
            assert!(
                !kinds.contains(&endurance_only),
                "{endurance_only} is a statement about continuous aerobic work: {kinds:?}"
            );
        }
        assert!(kinds.contains(&"effort_shape"), "{kinds:?}");
        assert!(
            !out.highlights.iter().any(|h| h.detail.contains("run")),
            "nothing here may talk to a lifter about running: {:?}",
            out.highlights.iter().map(|h| &h.detail).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rounds_are_read_off_the_laps_of_an_interval_session() {
        let mut a = activity();
        a.type_key = Some("jump_rope".into());
        a.zone_secs = [10.0, 20.0, 150.0, 170.0, 10.0];

        // Six rounds, each hotter than the last — the rests not keeping up.
        let laps: Vec<Value> = (0..6)
            .map(|i| {
                serde_json::json!({
                    "lapIndex": i + 1,
                    "duration": 30.0 + i as f64,
                    "averageHR": 130.0 + i as f64 * 6.0,
                })
            })
            .collect();
        let splits = serde_json::json!({ "lapDTOs": laps });

        let out = analyse(&a, None, Some(&splits), None, &[], vec![], "now");
        let kinds: Vec<&str> = out.highlights.iter().map(|h| h.kind.as_str()).collect();
        assert_eq!(out.discipline, Discipline::Interval);
        assert!(kinds.contains(&"rounds"), "{kinds:?}");
        assert!(kinds.contains(&"rounds_climbing"), "{kinds:?}");
    }

    /// A tactical session comes through as Garmin's "other", carrying a GPS
    /// track that wanders at 57 min/km. Every conclusion built on that speed —
    /// the pace fade, the decoupling — is noise with a coach's vocabulary.
    #[test]
    fn an_unclassified_session_borrows_nothing_from_running() {
        let mut a = activity();
        a.type_key = Some("other".into());
        let out = analyse(
            &a,
            Some(&details(120, true)),
            None,
            None,
            &[],
            vec![],
            "now",
        );
        let kinds: Vec<&str> = out.highlights.iter().map(|h| h.kind.as_str()).collect();
        assert_eq!(out.discipline, Discipline::Other);
        assert!(!kinds.contains(&"cardiac_drift"), "{kinds:?}");
        assert!(!kinds.contains(&"fade"), "{kinds:?}");
        assert!(kinds.contains(&"effort_shape"), "{kinds:?}");
    }

    #[test]
    fn garmins_own_zone_floors_win_over_the_fallback() {
        let buckets = serde_json::json!([
            { "zoneNumber": 1, "zoneLowBoundary": 100 },
            { "zoneNumber": 2, "zoneLowBoundary": 120 },
            { "zoneNumber": 3, "zoneLowBoundary": 140 },
            { "zoneNumber": 4, "zoneLowBoundary": 160 },
            { "zoneNumber": 5, "zoneLowBoundary": 180 },
        ]);
        let z = zone_profile(&activity(), Some(&buckets));
        assert!(z.measured);
        assert_eq!(z.floors[2], 140.0);
        assert_eq!(zone_of(145.0, &z.floors), 3);
        assert_eq!(zone_of(60.0, &z.floors), 1, "below Z1 is still Z1");
    }

    #[test]
    fn a_partial_zone_ladder_is_refused_rather_than_mixed() {
        let buckets = serde_json::json!([
            { "zoneNumber": 1, "zoneLowBoundary": 100 },
            { "zoneNumber": 2, "zoneLowBoundary": 120 },
        ]);
        let z = zone_profile(&activity(), Some(&buckets));
        assert!(!z.measured);
        assert_eq!(z.floors, FALLBACK_FLOORS);
    }

    /// The cross-check has to be weighted by time, not by sample count.
    /// Garmin samples irregularly, and a densely-sampled hard minute would
    /// otherwise outvote a sparsely-sampled easy ten and manufacture a
    /// disagreement that isn't there.
    #[test]
    fn the_zone_cross_check_weights_by_time_not_by_sample_count() {
        // Ten minutes at 125 bpm (Z2), sampled once a minute, then one minute
        // at 180 (Z5) sampled every two seconds. By count, Z5 dominates; by
        // time it is a small fraction.
        let mut hr = Vec::new();
        let mut elapsed = Vec::new();
        for i in 0..10 {
            hr.push(Some(125.0));
            elapsed.push(Some(i as f64 * 60.0));
        }
        for i in 0..30 {
            hr.push(Some(180.0));
            elapsed.push(Some(600.0 + i as f64 * 2.0));
        }
        let series = Series {
            hr,
            elapsed_s: elapsed,
            ..Default::default()
        };

        let pct = recompute_zones(&series, &FALLBACK_FLOORS).expect("a split");
        assert!(
            pct[1] > 85.0,
            "ten minutes of Z2 should dominate, got {pct:?}"
        );
        assert!(pct[4] < 15.0, "one minute of Z5 is a fraction, got {pct:?}");
    }

    #[test]
    fn a_trace_with_no_heart_rate_yields_no_second_opinion() {
        let series = Series {
            hr: vec![None; 10],
            elapsed_s: (0..10).map(|i| Some(i as f64)).collect(),
            ..Default::default()
        };
        assert_eq!(recompute_zones(&series, &FALLBACK_FLOORS), None);
    }

    #[test]
    fn laps_without_distance_still_list() {
        let splits = serde_json::json!({
            "lapDTOs": [
                { "lapIndex": 1, "distance": 1000.0, "duration": 400.0, "averageHR": 150.0 },
                { "lapIndex": 2, "duration": 300.0 },
                { "lapIndex": 3, "distance": 1000.0, "duration": 0.0 },
            ]
        });
        let laps = extract_laps(&splits);
        assert_eq!(laps.len(), 2, "a zero-duration lap is dropped");
        assert!(laps[0].pace_min_km.is_some());
        assert!(laps[1].pace_min_km.is_none());
    }

    #[test]
    fn comparison_only_looks_backwards() {
        let mut older = activity();
        older.activity_id = 2;
        older.start_time_local = Some("2026-07-01 18:00:00".into());
        older.avg_cadence = Some(140.0);

        let mut newer = activity();
        newer.activity_id = 3;
        newer.start_time_local = Some("2026-09-01 18:00:00".into());
        newer.avg_cadence = Some(200.0);

        let c = compare(&activity(), &[newer, older]).unwrap();
        assert_eq!(c.sessions, 1, "the later session is not a peer");
        assert_eq!(c.avg_cadence, Some(140.0));
        assert_eq!(c.cadence_delta, Some(10.0));
    }

    #[test]
    fn a_different_sport_is_not_a_peer() {
        let mut ride = activity();
        ride.activity_id = 2;
        ride.type_key = Some("cycling".into());
        ride.start_time_local = Some("2026-07-01 18:00:00".into());
        assert!(compare(&activity(), &[ride]).is_none());
    }

    /// Jump rope belongs to none of the families, and used to compare against
    /// nothing at all — which for a session with no pace and no zone target
    /// removed the only honest judgement left: how it went against the last few.
    #[test]
    fn a_sport_with_no_family_still_compares_against_itself() {
        let mut now = activity();
        now.type_key = Some("jump_rope".into());

        let mut earlier = now.clone();
        earlier.activity_id = 2;
        earlier.start_time_local = Some("2026-07-01 18:00:00".into());
        earlier.duration_s = Some(600.0);

        let c = compare(&now, &[earlier.clone()]).expect("jump rope has peers");
        assert_eq!(c.sessions, 1);
        assert_eq!(c.avg_duration_s, Some(600.0));

        // But two sessions Garmin failed to classify are not peers by virtue of
        // both being unclassified.
        let mut a = now.clone();
        let mut b = earlier;
        a.type_key = Some("other".into());
        b.type_key = Some("other".into());
        assert!(compare(&a, &[b]).is_none());
    }

    #[test]
    fn the_longest_easy_stretch_is_measured_not_counted() {
        let mut a = activity();
        a.zone_secs = [0.0, 600.0, 0.0, 0.0, 0.0];
        // Thirteen minutes easy, then hard, then ten easy. The first stretch is
        // the longer one and has to win on length rather than on order.
        let rows: Vec<Value> = (0..90)
            .map(|i| {
                let hr = if (40..60).contains(&i) { 165.0 } else { 125.0 };
                serde_json::json!({ "metrics": [i as f64 * 20.0, hr] })
            })
            .collect();
        let d = serde_json::json!({
            "metricDescriptors": [
                { "key": "sumElapsedDuration", "metricsIndex": 0 },
                { "key": "directHeartRate", "metricsIndex": 1 },
            ],
            "activityDetailMetrics": rows,
        });
        let s = extract_series(&d);
        let z = zone_profile(&a, None);
        let (from, to) = longest_easy(&s, &z).unwrap();
        assert_eq!(from, 0.0);
        assert!((to - 780.0).abs() < 1.0, "{from}..{to}");

        // Two stretches of equal length resolve to the earlier one, so a report
        // doesn't move between them from one open to the next.
        let even: Vec<Value> = (0..90)
            .map(|i| {
                let hr = if (30..60).contains(&i) { 165.0 } else { 125.0 };
                serde_json::json!({ "metrics": [i as f64 * 20.0, hr] })
            })
            .collect();
        let tied = extract_series(&serde_json::json!({
            "metricDescriptors": [
                { "key": "sumElapsedDuration", "metricsIndex": 0 },
                { "key": "directHeartRate", "metricsIndex": 1 },
            ],
            "activityDetailMetrics": even,
        }));
        assert_eq!(longest_easy(&tied, &z).unwrap().0, 0.0);
    }

    /// The map places a highlight by matching `at_s` against `elapsed_s`, so
    /// every highlight has to be on that clock and not on one that counts from
    /// the start of the session. They coincide when the first sample is at
    /// zero, which is why this session's doesn't.
    #[test]
    fn highlights_are_stamped_on_the_series_clock() {
        let mut a = activity();
        a.zone_secs = [0.0, 1200.0, 300.0, 0.0, 0.0];

        // Starts at 900s, easy until 1500s, then hard.
        let rows: Vec<Value> = (0..90)
            .map(|i| {
                let hr = if i < 60 { 125.0 } else { 150.0 };
                serde_json::json!({ "metrics": [900.0 + i as f64 * 10.0, hr] })
            })
            .collect();
        let d = serde_json::json!({
            "metricDescriptors": [
                { "key": "sumElapsedDuration", "metricsIndex": 0 },
                { "key": "directHeartRate", "metricsIndex": 1 },
            ],
            "activityDetailMetrics": rows,
        });

        let out = analyse(&a, Some(&d), None, None, &[], vec![], "now");
        let first = out.series.elapsed_s[0].unwrap();
        let last = out.series.elapsed_s[out.series.len() - 1].unwrap();

        for h in &out.highlights {
            if let Some(at) = h.at_s {
                assert!(
                    at >= first && at <= last,
                    "{} pinned at {at}, outside the series' {first}..{last}",
                    h.kind
                );
            }
        }
        assert!(
            out.highlights.iter().any(|h| h.kind == "longest_easy"),
            "the easy stretch should have been found"
        );
    }

    #[test]
    fn pace_labels_round_into_the_next_minute() {
        assert_eq!(pace_label(5.0), "5:00");
        assert_eq!(pace_label(5.5), "5:30");
        assert_eq!(pace_label(5.999), "6:00");
        assert_eq!(clock(3661.0), "1:01:01");
        assert_eq!(clock(61.0), "1:01");
    }
}
