//! One night's sleep, in the detail Garmin actually sends.
//!
//! The cache has always kept two numbers about sleep — how long, and the score
//! — because that is all a row on the Health chart needs. The payload behind
//! those two numbers is far richer: the stage-by-stage hypnogram, the times the
//! athlete fell asleep and woke, overnight HRV, respiration, SpO2, restlessness,
//! and Garmin's own verdict on each component of the score against the range it
//! wants that component in. None of it cost an extra request; it was being
//! parsed for two fields and thrown away.
//!
//! So this module is the whole night, parsed once at sync time into
//! [`SleepNight`], and the reading of a run of nights in [`report`]. The
//! analysis is here rather than in the screen for the reason CLAUDE.md gives:
//! the app's Sleep screen and the coach in Ask both have to be able to say the
//! same thing about the same night, and they can only do that if there is one
//! implementation of what the night means.
//!
//! Everything below [`SleepNight::from_payload`] is a pure function of rows
//! that were already fetched — nothing here opens a database or talks to
//! Garmin, so a hand-built fortnight can be run through it in a test.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::{CachedActivity, DailyMetrics, Db};

/* ------------------------------------------------------------------ shapes --- */

/// Which stage a slice of the night was spent in.
///
/// Garmin numbers these 0–3 in `sleepLevels` and never says so anywhere; the
/// mapping was confirmed against a real night by summing each level's slices
/// and checking the four totals against `deepSleepSeconds` and friends, which
/// matched to the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Deep,
    Light,
    Rem,
    Awake,
    /// A slice the watch recorded but couldn't classify. Rare, and never
    /// counted into a stage total.
    Unmeasurable,
}

impl Stage {
    fn from_level(level: f64) -> Stage {
        match level as i64 {
            0 => Stage::Deep,
            1 => Stage::Light,
            2 => Stage::Rem,
            3 => Stage::Awake,
            _ => Stage::Unmeasurable,
        }
    }
}

/// One unbroken run in a single stage — a bar of the hypnogram.
///
/// Times are local to where the athlete slept, not GMT: a chart of a night has
/// to be able to print "01:31" and mean the clock on the wall.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageSlice {
    pub stage: Stage,
    /// `YYYY-MM-DDTHH:MM:SS`, local.
    pub start_local: String,
    pub end_local: String,
    /// Minutes from the moment sleep began, so a chart can lay the night out on
    /// one axis without re-parsing every timestamp.
    pub from_start_mins: f64,
    pub secs: f64,
}

/// A heart-rate reading during the night, positioned the same way the stages
/// are: minutes from the start of sleep.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HrSample {
    pub from_start_mins: f64,
    pub bpm: f64,
}

/// One component of Garmin's sleep score, with the range Garmin wants it in.
///
/// Kept as a list rather than as columns because the interesting fact about
/// "deep sleep was 13%" is entirely the "against a target of 16–33%" beside it,
/// and that band moves with the length of the night.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScorePart {
    /// Garmin's key: `deepPercentage`, `remPercentage`, `lightPercentage`,
    /// `awakeCount`, `restlessness`, `stress`, `totalDuration`.
    pub key: String,
    /// The measured value, in whatever unit the key implies. Absent on the
    /// components Garmin qualifies without publishing a number.
    pub value: Option<f64>,
    /// `EXCELLENT`, `GOOD`, `FAIR`, `POOR`.
    pub qualifier: Option<String>,
    pub optimal_start: Option<f64>,
    pub optimal_end: Option<f64>,
}

/// Everything the cache keeps about one night.
///
/// Keyed by the calendar date the athlete *woke up on*, which is how Garmin
/// keys it and which lines this table up with `daily_metrics` for free: the
/// resting HR and HRV on a given day's row are measurements from the night this
/// row describes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepNight {
    pub date: String,

    pub score: Option<f64>,
    /// The word Garmin puts on the score.
    pub score_qualifier: Option<String>,
    /// `POSITIVE_HIGHLY_RECOVERING` and the like — Garmin's shouted verdict.
    pub feedback: Option<String>,
    pub insight: Option<String>,

    pub total_secs: Option<f64>,
    pub deep_secs: Option<f64>,
    pub light_secs: Option<f64>,
    pub rem_secs: Option<f64>,
    pub awake_secs: Option<f64>,
    pub nap_secs: Option<f64>,

    /// When sleep began and ended, local. `YYYY-MM-DDTHH:MM:SS`.
    pub start_local: Option<String>,
    pub end_local: Option<String>,

    /// What Garmin reckoned was needed, and the unadjusted baseline it starts
    /// from — both in seconds, though Garmin sends minutes.
    pub need_secs: Option<f64>,
    pub need_baseline_secs: Option<f64>,

    pub awake_count: Option<f64>,
    pub restless_count: Option<f64>,

    pub avg_overnight_hrv: Option<f64>,
    pub resting_hr: Option<f64>,
    pub avg_hr: Option<f64>,
    pub avg_stress: Option<f64>,
    pub body_battery_change: Option<f64>,
    pub avg_respiration: Option<f64>,
    pub low_respiration: Option<f64>,
    pub high_respiration: Option<f64>,
    pub avg_spo2: Option<f64>,
    pub lowest_spo2: Option<f64>,

    pub score_parts: Vec<ScorePart>,
    pub stages: Vec<StageSlice>,
    /// Overnight heart rate, thinned to roughly one point every five minutes.
    /// Garmin samples every two; nothing on a chart this wide can show the
    /// difference, and the untouched series is three times the bytes.
    pub hr: Vec<HrSample>,
}

impl SleepNight {
    /// True once anything worth keeping came back. A night the watch spent on
    /// the bedside table parses fine and holds nothing.
    pub fn has_data(&self) -> bool {
        self.total_secs.is_some() || self.score.is_some() || !self.stages.is_empty()
    }

    /// Time in bed — asleep plus the minutes awake inside the sleep window.
    pub fn in_bed_secs(&self) -> Option<f64> {
        Some(self.total_secs? + self.awake_secs.unwrap_or(0.0))
    }

    /// Share of the night asleep rather than awake, as a percentage. The
    /// textbook name is sleep efficiency and the textbook target is 85%+.
    pub fn efficiency(&self) -> Option<f64> {
        let in_bed = self.in_bed_secs()?;
        (in_bed > 0.0).then_some(self.total_secs? / in_bed * 100.0)
    }

    fn stage_pct(&self, secs: Option<f64>) -> Option<f64> {
        let total = self.total_secs?;
        (total > 0.0).then_some(secs? / total * 100.0)
    }

    pub fn deep_pct(&self) -> Option<f64> {
        self.stage_pct(self.deep_secs)
    }
    pub fn rem_pct(&self) -> Option<f64> {
        self.stage_pct(self.rem_secs)
    }
    pub fn light_pct(&self) -> Option<f64> {
        self.stage_pct(self.light_secs)
    }

    /// Bedtime as minutes past 18:00, so an 01:30 bedtime sorts after a 22:45
    /// one instead of being seven hours earlier. Nothing in this app compares
    /// bedtimes any other way, because the wrap at midnight is exactly where
    /// the interesting variation lives.
    pub fn bedtime_mins(&self) -> Option<f64> {
        mins_past_six_pm(self.start_local.as_deref()?)
    }

    /// Wake time, on the same 18:00-based scale — so 07:41 reads as 821.
    pub fn wake_mins(&self) -> Option<f64> {
        mins_past_six_pm(self.end_local.as_deref()?)
    }
}

/// `2026-08-09T22:58:42` → 298. Wraps so that anything before 18:00 is read as
/// belonging to the following morning.
fn mins_past_six_pm(local: &str) -> Option<f64> {
    let time = local.get(11..16)?;
    let (h, m) = time.split_once(':')?;
    let mins = h.parse::<f64>().ok()? * 60.0 + m.parse::<f64>().ok()?;
    Some(if mins >= 18.0 * 60.0 {
        mins - 18.0 * 60.0
    } else {
        mins + 6.0 * 60.0
    })
}

/* ----------------------------------------------------------------- parsing --- */

fn num(v: &Value, path: &[&str]) -> Option<f64> {
    let mut cur = v;
    for key in path {
        cur = cur.get(key)?;
    }
    cur.as_f64()
}

fn text(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for key in path {
        cur = cur.get(key)?;
    }
    cur.as_str().map(str::to_string)
}

/// The score components Garmin publishes, in the order they're worth reading:
/// how long, then what it was made of, then what interrupted it.
const SCORE_KEYS: [&str; 7] = [
    "totalDuration",
    "deepPercentage",
    "remPercentage",
    "lightPercentage",
    "awakeCount",
    "restlessness",
    "stress",
];

/// Roughly how far apart the kept heart-rate samples are.
const HR_BUCKET_MINS: f64 = 5.0;

impl SleepNight {
    /// Read one night out of the `dailySleepData` response.
    ///
    /// Never fails: every field is optional, because every field is missing on
    /// some real night. A watch charging overnight, a nap-only day, a device
    /// too old for REM — each of those comes back as a payload with holes in
    /// it, and a parse that insisted on any one field would drop the rest.
    pub fn from_payload(date: &str, v: &Value) -> SleepNight {
        let dto = v.get("dailySleepDTO").unwrap_or(&Value::Null);

        // Garmin sends both a GMT and a local epoch for the same instant, and
        // the gap between them is the athlete's UTC offset that night. That's
        // the only offset in the payload, and it's what turns the stage
        // timeline — sent in GMT, and in a different format again — into
        // wall-clock times.
        let start_gmt_ms = num(dto, &["sleepStartTimestampGMT"]);
        let start_local_ms = num(dto, &["sleepStartTimestampLocal"]);
        let offset_ms = match (start_gmt_ms, start_local_ms) {
            (Some(g), Some(l)) => l - g,
            _ => 0.0,
        };

        let mut night = SleepNight {
            date: date.to_string(),
            score: num(dto, &["sleepScores", "overall", "value"]),
            score_qualifier: text(dto, &["sleepScores", "overall", "qualifierKey"]),
            feedback: text(dto, &["sleepScoreFeedback"]),
            insight: text(dto, &["sleepScoreInsight"]),

            total_secs: num(dto, &["sleepTimeSeconds"]),
            deep_secs: num(dto, &["deepSleepSeconds"]),
            light_secs: num(dto, &["lightSleepSeconds"]),
            rem_secs: num(dto, &["remSleepSeconds"]),
            awake_secs: num(dto, &["awakeSleepSeconds"]),
            nap_secs: num(dto, &["napTimeSeconds"]),

            start_local: start_local_ms.map(local_iso),
            end_local: num(dto, &["sleepEndTimestampLocal"]).map(local_iso),

            // Sent in minutes, kept in seconds like every other duration here.
            need_secs: num(dto, &["sleepNeed", "actual"]).map(|m| m * 60.0),
            need_baseline_secs: num(dto, &["sleepNeed", "baseline"]).map(|m| m * 60.0),

            awake_count: num(dto, &["awakeCount"]),
            restless_count: num(v, &["restlessMomentsCount"]),

            avg_overnight_hrv: num(v, &["avgOvernightHrv"]),
            resting_hr: num(v, &["restingHeartRate"]),
            avg_hr: num(dto, &["avgHeartRate"]),
            avg_stress: num(dto, &["avgSleepStress"]),
            body_battery_change: num(v, &["bodyBatteryChange"]),
            avg_respiration: num(dto, &["averageRespirationValue"]),
            low_respiration: num(dto, &["lowestRespirationValue"]),
            high_respiration: num(dto, &["highestRespirationValue"]),
            avg_spo2: num(dto, &["averageSpO2Value"]),
            lowest_spo2: num(dto, &["lowestSpO2Value"]),

            score_parts: Vec::new(),
            stages: Vec::new(),
            hr: Vec::new(),
        };

        for key in SCORE_KEYS {
            let Some(part) = dto.pointer(&format!("/sleepScores/{key}")) else {
                continue;
            };
            night.score_parts.push(ScorePart {
                key: key.to_string(),
                value: num(part, &["value"]),
                qualifier: text(part, &["qualifierKey"]),
                optimal_start: num(part, &["optimalStart"]),
                optimal_end: num(part, &["optimalEnd"]),
            });
        }

        if let Some(levels) = v.get("sleepLevels").and_then(Value::as_array) {
            for slice in levels {
                let (Some(start), Some(end)) = (
                    text(slice, &["startGMT"]).and_then(|s| gmt_naive_ms(&s)),
                    text(slice, &["endGMT"]).and_then(|s| gmt_naive_ms(&s)),
                ) else {
                    continue;
                };
                let Some(level) = num(slice, &["activityLevel"]) else {
                    continue;
                };
                night.stages.push(StageSlice {
                    stage: Stage::from_level(level),
                    start_local: local_iso(start + offset_ms),
                    end_local: local_iso(end + offset_ms),
                    from_start_mins: start_gmt_ms.map(|s| (start - s) / 60_000.0).unwrap_or(0.0),
                    secs: (end - start) / 1000.0,
                });
            }
            // Garmin sends these in order, but a chart that assumes so and is
            // wrong draws a night that goes backwards.
            night
                .stages
                .sort_by(|a, b| a.from_start_mins.total_cmp(&b.from_start_mins));
        }

        if let (Some(samples), Some(start)) = (
            v.get("sleepHeartRate").and_then(Value::as_array),
            start_gmt_ms,
        ) {
            night.hr = thin_hr(samples, start);
        }

        night
    }
}

/// Epoch milliseconds already shifted into local time → `YYYY-MM-DDTHH:MM:SS`.
///
/// Deliberately naive: the offset has been folded in above, so attaching a
/// timezone here would apply it twice.
fn local_iso(ms: f64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|t| t.naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// `2026-08-09T22:58:42.0` (GMT, no zone marker) → epoch milliseconds.
fn gmt_naive_ms(s: &str) -> Option<f64> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|t| t.and_utc().timestamp_millis() as f64)
}

/// Average the overnight heart rate into five-minute buckets.
fn thin_hr(samples: &[Value], start_ms: f64) -> Vec<HrSample> {
    let mut out: Vec<HrSample> = Vec::new();
    let mut bucket = f64::NAN;
    let mut sum = 0.0;
    let mut n = 0.0;

    let flush = |bucket: f64, sum: f64, n: f64, out: &mut Vec<HrSample>| {
        if n > 0.0 {
            out.push(HrSample {
                from_start_mins: bucket * HR_BUCKET_MINS,
                bpm: (sum / n * 10.0).round() / 10.0,
            });
        }
    };

    for s in samples {
        let (Some(at), Some(bpm)) = (num(s, &["startGMT"]), num(s, &["value"])) else {
            continue;
        };
        let slot = ((at - start_ms) / 60_000.0 / HR_BUCKET_MINS).floor();
        // Garmin's overnight series opens a minute or so before sleep does.
        // Those readings are of someone lying awake, and on a chart laid out
        // from the start of sleep they'd sit off the left edge.
        if slot < 0.0 {
            continue;
        }
        if slot != bucket {
            flush(bucket, sum, n, &mut out);
            bucket = slot;
            sum = 0.0;
            n = 0.0;
        }
        sum += bpm;
        n += 1.0;
    }
    flush(bucket, sum, n, &mut out);
    out
}

/* ------------------------------------------------------------------ report --- */

/// What a window of nights averages out to.
///
/// Every average is over the nights that carry the figure, not over the window
/// — a fortnight with three nights the watch wasn't worn is a fortnight of
/// eleven nights, and calling it fourteen would drag every mean toward nothing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepAverages {
    pub nights: usize,
    pub total_secs: Option<f64>,
    pub score: Option<f64>,
    pub deep_pct: Option<f64>,
    pub rem_pct: Option<f64>,
    pub light_pct: Option<f64>,
    pub awake_secs: Option<f64>,
    pub efficiency: Option<f64>,
    pub overnight_hrv: Option<f64>,
    pub resting_hr: Option<f64>,
    pub restless_count: Option<f64>,
    /// Bedtime and wake time as minutes past 18:00, with the spread that
    /// matters more than either average: the standard deviation.
    pub bedtime_mins: Option<f64>,
    pub bedtime_sd_mins: Option<f64>,
    pub wake_mins: Option<f64>,
    pub wake_sd_mins: Option<f64>,
    /// Nights in the window that came in under seven hours.
    pub short_nights: usize,
}

/// How a note should read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Good,
    Note,
    Watch,
}

/// Something this athlete's own nights say.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepInsight {
    pub id: String,
    pub tone: Tone,
    /// The claim, in one sentence.
    pub claim: String,
    /// The numbers behind it, and what to do about them.
    pub detail: String,
    /// How many nights it was computed from, so a reader can weigh it.
    pub nights: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepReport {
    /// The most recent night in the window, which is the one the screen leads
    /// with. `None` when the window holds nothing.
    pub last_night: Option<SleepNight>,
    /// The window, newest first.
    pub nights: Vec<SleepNight>,
    pub averages: SleepAverages,
    pub insights: Vec<SleepInsight>,
    /// True when the cache has wellness rows in this window but no sleep
    /// detail for them — the state every existing install is in until it
    /// re-syncs, and the one thing an empty screen must not be silent about.
    pub needs_backfill: bool,
}

impl SleepReport {
    /// The same report with the per-night series dropped and the window
    /// trimmed.
    ///
    /// For the two surfaces that hand this to a language model. A night's
    /// hypnogram is seventeen slices and its heart-rate curve about a hundred
    /// points; ninety of those is a megabyte of JSON that says nothing a model
    /// can use, and it would crowd out the summary that does. The charts need
    /// the series and nobody else does.
    pub fn brief(mut self, max_nights: usize) -> SleepReport {
        let strip = |mut n: SleepNight| {
            n.stages = Vec::new();
            n.hr = Vec::new();
            n
        };
        self.last_night = self.last_night.map(strip);
        self.nights = self
            .nights
            .into_iter()
            .take(max_nights)
            .map(strip)
            .collect();
        self
    }
}

/// Read the last `days` of nights out of the cache and make what can be made
/// of them.
pub fn report(db: &Db, days: u32) -> Result<SleepReport> {
    let from = crate::days_ago(days);
    let nights = db.sleep_nights_since(&from)?;
    let daily = db.daily_since(&from)?;
    // Insights that reach for training only need the sessions in the window,
    // and a run is only interesting here for when it started and how hard it
    // was.
    let activities = db.activities_since(&from)?;

    let averages = averages(&nights);
    let insights = insights(&nights, &daily, &activities);

    Ok(SleepReport {
        needs_backfill: nights.is_empty() && daily.iter().any(|d| d.sleep_secs.is_some()),
        last_night: nights.first().cloned(),
        nights,
        averages,
        insights,
    })
}

fn mean(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64)
}

fn sd(xs: &[f64]) -> Option<f64> {
    // One point has no spread, and reporting 0 would read as perfect
    // consistency rather than as one night.
    if xs.len() < 2 {
        return None;
    }
    let m = mean(xs)?;
    let var = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64;
    Some(var.sqrt())
}

fn collect(nights: &[SleepNight], f: impl Fn(&SleepNight) -> Option<f64>) -> Vec<f64> {
    nights.iter().filter_map(f).collect()
}

/// Seven hours. Not a target — the shortest night that isn't a short night.
const SHORT_NIGHT_SECS: f64 = 7.0 * 3600.0;

pub fn averages(nights: &[SleepNight]) -> SleepAverages {
    let bedtimes = collect(nights, |n| n.bedtime_mins());
    let wakes = collect(nights, |n| n.wake_mins());

    SleepAverages {
        nights: nights.len(),
        total_secs: mean(&collect(nights, |n| n.total_secs)),
        score: mean(&collect(nights, |n| n.score)),
        deep_pct: mean(&collect(nights, |n| n.deep_pct())),
        rem_pct: mean(&collect(nights, |n| n.rem_pct())),
        light_pct: mean(&collect(nights, |n| n.light_pct())),
        awake_secs: mean(&collect(nights, |n| n.awake_secs)),
        efficiency: mean(&collect(nights, |n| n.efficiency())),
        overnight_hrv: mean(&collect(nights, |n| n.avg_overnight_hrv)),
        resting_hr: mean(&collect(nights, |n| n.resting_hr)),
        restless_count: mean(&collect(nights, |n| n.restless_count)),
        bedtime_mins: mean(&bedtimes),
        bedtime_sd_mins: sd(&bedtimes),
        wake_mins: mean(&wakes),
        wake_sd_mins: sd(&wakes),
        short_nights: nights
            .iter()
            .filter(|n| n.total_secs.is_some_and(|s| s < SHORT_NIGHT_SECS))
            .count(),
    }
}

/// The fewest nights any claim here is allowed to rest on.
///
/// Five is low for statistics and right for this: these are descriptions of a
/// window the reader is looking at, not inferences about a population, and
/// every one of them prints the count it used.
const MIN_NIGHTS: usize = 5;

/// Bedtime spread past which consistency is the thing to fix first. An hour of
/// standard deviation means a typical week swings two hours end to end.
const BEDTIME_SD_WATCH: f64 = 60.0;

/// Seconds above threshold in a day that make it a hard day. Same bar
/// `findings` uses, deliberately — two modules disagreeing about what "hard"
/// means would make their conclusions incomparable.
const HARD_DAY_SECS: f64 = 300.0;

/// What this window says, in the order it's worth hearing.
pub fn insights(
    nights: &[SleepNight],
    daily: &[DailyMetrics],
    activities: &[CachedActivity],
) -> Vec<SleepInsight> {
    let mut out = Vec::new();
    if nights.len() < MIN_NIGHTS {
        return out;
    }
    let avg = averages(nights);

    if let Some(claim) = duration_insight(&avg) {
        out.push(claim);
    }
    if let Some(claim) = consistency_insight(&avg) {
        out.push(claim);
    }
    if let Some(claim) = stage_insight(nights, &avg) {
        out.push(claim);
    }
    if let Some(claim) = late_session_insight(nights, activities) {
        out.push(claim);
    }
    if let Some(claim) = hard_day_insight(nights, activities) {
        out.push(claim);
    }
    if let Some(claim) = hrv_insight(nights, daily) {
        out.push(claim);
    }
    out
}

fn hm(secs: f64) -> String {
    let mins = (secs / 60.0).round() as i64;
    format!("{}h {:02}m", mins / 60, mins % 60)
}

/// Minutes past 18:00 back into a clock face.
fn clock(mins: f64) -> String {
    let total = (mins.round() as i64 + 18 * 60).rem_euclid(24 * 60);
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn duration_insight(avg: &SleepAverages) -> Option<SleepInsight> {
    let total = avg.total_secs?;
    let short = avg.short_nights;
    let n = avg.nights;

    // Eight hours is Garmin's baseline need for this account and a reasonable
    // one generally; the bands below are read against it rather than against a
    // number invented here.
    let (tone, claim, detail) = if total >= 7.5 * 3600.0 && short * 4 <= n {
        (
            Tone::Good,
            format!("You're averaging {} a night.", hm(total)),
            format!(
                "{short} of {n} nights came in under seven hours, which is a normal amount of \
                 short nights rather than a pattern. Duration isn't the thing to work on."
            ),
        )
    } else if total >= 7.0 * 3600.0 {
        (
            Tone::Note,
            format!("You're averaging {} a night.", hm(total)),
            format!(
                "That clears seven hours, but {short} of {n} nights didn't. The gap between a \
                 good week and a bad one here is bedtime, not wake time — the alarm doesn't move."
            ),
        )
    } else {
        (
            Tone::Watch,
            format!("You're averaging {} a night, under seven hours.", hm(total)),
            format!(
                "{short} of {n} nights were short. At 88 kg and three months into running, sleep \
                 is where the adaptation actually happens — a short week shows up as a higher \
                 resting HR and a flatter HRV before it shows up as a bad session."
            ),
        )
    };

    Some(SleepInsight {
        id: "duration".into(),
        tone,
        claim,
        detail,
        nights: n,
    })
}

fn consistency_insight(avg: &SleepAverages) -> Option<SleepInsight> {
    let sd = avg.bedtime_sd_mins?;
    let bed = avg.bedtime_mins?;
    let wake_sd = avg.wake_sd_mins;
    let n = avg.nights;

    let wake_note = match wake_sd {
        Some(w) if w < sd * 0.7 => format!(
            " Waking is steadier than going to bed (±{:.0} min against ±{:.0}), so the variable \
             you actually control is the one that's moving.",
            w, sd
        ),
        Some(w) => format!(" Wake time swings about as much, at ±{w:.0} min."),
        None => String::new(),
    };

    let (tone, claim, detail) = if sd >= BEDTIME_SD_WATCH {
        (
            Tone::Watch,
            format!(
                "Your bedtime swings ±{sd:.0} minutes around {}.",
                clock(bed)
            ),
            format!(
                "That's a typical week spanning roughly {:.0} hours end to end. Regularity is the \
                 single most reliable lever on sleep quality — steadier than duration, and it \
                 costs nothing.{wake_note}",
                sd * 4.0 / 60.0
            ),
        )
    } else if sd >= 35.0 {
        (
            Tone::Note,
            format!(
                "Your bedtime holds to ±{sd:.0} minutes around {}.",
                clock(bed)
            ),
            format!("Reasonably steady. Tightening it toward half an hour is the cheapest quality gain left.{wake_note}"),
        )
    } else {
        (
            Tone::Good,
            format!(
                "Your bedtime is steady: ±{sd:.0} minutes around {}.",
                clock(bed)
            ),
            format!("That consistency is doing more for you than any single long night would.{wake_note}"),
        )
    };

    Some(SleepInsight {
        id: "consistency".into(),
        tone,
        claim,
        detail,
        nights: n,
    })
}

/// Deep and REM against the bands Garmin published for these very nights.
///
/// Reading them against Garmin's own target rather than a textbook percentage
/// matters, because the band moves with the length of the night: 13% deep is
/// short of the range on an eight-hour night and fine on a five-hour one.
fn stage_insight(nights: &[SleepNight], avg: &SleepAverages) -> Option<SleepInsight> {
    let deep = avg.deep_pct?;
    let rem = avg.rem_pct?;

    let below = |key: &str| {
        nights
            .iter()
            .filter(|n| {
                n.score_parts.iter().any(|p| {
                    p.key == key && matches!(p.qualifier.as_deref(), Some("FAIR" | "POOR"))
                })
            })
            .count()
    };
    let deep_short = below("deepPercentage");
    let rem_short = below("remPercentage");
    let n = nights.len();

    let (tone, claim, detail) = if deep_short * 2 >= n {
        (
            Tone::Note,
            format!("Deep sleep is averaging {deep:.0}% of the night."),
            format!(
                "Garmin marked deep sleep short on {deep_short} of {n} nights. Deep is the \
                 front-loaded stage — it's mostly decided in the first three hours, which makes \
                 it the stage a late bedtime and a late meal cost you first. REM is averaging \
                 {rem:.0}%."
            ),
        )
    } else if rem_short * 2 >= n {
        (
            Tone::Note,
            format!("REM is averaging {rem:.0}% of the night."),
            format!(
                "Garmin marked REM short on {rem_short} of {n} nights. REM is back-loaded — it \
                 comes disproportionately from the last two hours, so it's the stage a short \
                 night or an early alarm takes. Deep is averaging {deep:.0}%, which it isn't \
                 flagging."
            ),
        )
    } else {
        (
            Tone::Good,
            format!("Stage mix is fine: {deep:.0}% deep, {rem:.0}% REM."),
            format!(
                "Garmin flagged deep on {deep_short} of {n} nights and REM on {rem_short}. \
                 There's nothing to chase here — stage percentages are mostly a consequence of \
                 total time asleep, not something to optimise directly."
            ),
        )
    };

    Some(SleepInsight {
        id: "stages".into(),
        tone,
        claim,
        detail,
        nights: n,
    })
}

/// Local hour a session started, or `None` if the stamp won't parse.
fn start_hour(a: &CachedActivity) -> Option<f64> {
    let s = a.start_time_local.as_deref()?;
    let (h, m) = s.get(11..16)?.split_once(':')?;
    Some(h.parse::<f64>().ok()? + m.parse::<f64>().ok()? / 60.0)
}

/// A session after this hour is a late one.
const LATE_HOUR: f64 = 19.5;

/// Nights following a late session against nights following an early one or
/// none.
///
/// The comparison is deliberately crude — two group means, no interval — and
/// says so in the text. It exists to be looked at, not to settle anything.
fn late_session_insight(
    nights: &[SleepNight],
    activities: &[CachedActivity],
) -> Option<SleepInsight> {
    let late_dates: std::collections::HashSet<&str> = activities
        .iter()
        .filter(|a| start_hour(a).is_some_and(|h| h >= LATE_HOUR))
        .filter_map(|a| a.local_date.as_deref())
        .collect();
    if late_dates.is_empty() {
        return None;
    }

    // A night is keyed by the morning it ended on, so the session that could
    // have affected it happened the *previous* day.
    let day_before = |date: &str| {
        chrono::NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d")
            .ok()
            .map(|d| {
                (d - chrono::Duration::days(1))
                    .format("%Y-%m-%d")
                    .to_string()
            })
    };

    let (mut after_late, mut after_rest) = (Vec::new(), Vec::new());
    for n in nights {
        let Some(score) = n.score else { continue };
        let Some(prev) = day_before(&n.date) else {
            continue;
        };
        if late_dates.contains(prev.as_str()) {
            after_late.push(score);
        } else {
            after_rest.push(score);
        }
    }
    // Three nights either side is the floor for saying anything at all, and
    // even that is thin.
    if after_late.len() < 3 || after_rest.len() < 3 {
        return None;
    }

    let late = mean(&after_late)?;
    let rest = mean(&after_rest)?;
    let gap = rest - late;
    let n = after_late.len();

    // Under three points of sleep score is inside the noise of the score
    // itself, and saying anything about it would be reading tea leaves.
    if gap.abs() < 3.0 {
        return Some(SleepInsight {
            id: "late-session".into(),
            tone: Tone::Good,
            claim: "Training late doesn’t seem to cost you sleep.".into(),
            detail: format!(
                "The {n} nights after a session starting past {:.0}:{:02.0} scored {late:.0} on \
                 average, against {rest:.0} for the rest — a difference small enough to be \
                 nothing. Worth knowing, since evening is when you actually train.",
                LATE_HOUR.floor(),
                LATE_HOUR.fract() * 60.0
            ),
            nights: n,
        });
    }

    Some(SleepInsight {
        id: "late-session".into(),
        tone: if gap > 0.0 { Tone::Watch } else { Tone::Good },
        claim: if gap > 0.0 {
            format!("Sleep scores {gap:.0} points lower after an evening session.")
        } else {
            format!(
                "Sleep scores {:.0} points higher after an evening session.",
                -gap
            )
        },
        detail: format!(
            "{n} nights followed a session starting past {:.0}:{:02.0} and averaged {late:.0}; \
             the others averaged {rest:.0}. Two group means with no interval behind them — read \
             it as something to watch across the next month, not as settled. If it holds, the fix \
             is finishing hard work three hours before bed rather than training earlier in \
             general.",
            LATE_HOUR.floor(),
            LATE_HOUR.fract() * 60.0
        ),
        nights: n,
    })
}

/// Whether hard days cost the night that follows them.
fn hard_day_insight(nights: &[SleepNight], activities: &[CachedActivity]) -> Option<SleepInsight> {
    let mut hard: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for a in activities {
        let Some(date) = a.local_date.as_deref() else {
            continue;
        };
        *hard.entry(date).or_insert(0.0) += a.zone_secs[3] + a.zone_secs[4];
    }

    let day_before = |date: &str| {
        chrono::NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d")
            .ok()
            .map(|d| {
                (d - chrono::Duration::days(1))
                    .format("%Y-%m-%d")
                    .to_string()
            })
    };

    let (mut after_hard, mut after_easy) = (Vec::new(), Vec::new());
    for n in nights {
        let Some(hr) = n.resting_hr else { continue };
        let Some(prev) = day_before(&n.date) else {
            continue;
        };
        if hard.get(prev.as_str()).is_some_and(|s| *s >= HARD_DAY_SECS) {
            after_hard.push(hr);
        } else {
            after_easy.push(hr);
        }
    }
    if after_hard.len() < 3 || after_easy.len() < 3 {
        return None;
    }

    let h = mean(&after_hard)?;
    let e = mean(&after_easy)?;
    let gap = h - e;
    let n = after_hard.len();

    Some(SleepInsight {
        id: "hard-day".into(),
        tone: if gap >= 3.0 { Tone::Note } else { Tone::Good },
        claim: if gap >= 3.0 {
            format!("Overnight resting HR runs {gap:.0} bpm higher after a hard day.")
        } else {
            "Hard days aren't lifting your overnight resting HR much.".into()
        },
        // A decimal place, because the interesting case is the one where the
        // two are close — and "51 bpm against 51" printed as whole numbers
        // reads as a rounding bug rather than as the finding it is.
        detail: format!(
            "The {n} nights after a day with five minutes or more above threshold averaged \
             {h:.1} bpm, against {e:.1} on the others. A few beats is the normal cost of a hard \
             session being paid overnight; a persistent gap of five or more is the signal to \
             space hard days further apart."
        ),
        nights: n,
    })
}

/// Whether the long nights are the high-HRV nights.
///
/// Correlation, not cause, and worded that way — HRV and sleep move together
/// for a dozen reasons, most of which are upstream of both.
fn hrv_insight(nights: &[SleepNight], daily: &[DailyMetrics]) -> Option<SleepInsight> {
    let hrv_by_date: std::collections::HashMap<&str, f64> = daily
        .iter()
        .filter_map(|d| Some((d.date.as_str(), d.hrv_last_night?)))
        .collect();

    let pairs: Vec<(f64, f64)> = nights
        .iter()
        .filter_map(|n| Some((n.total_secs? / 3600.0, *hrv_by_date.get(n.date.as_str())?)))
        .collect();
    if pairs.len() < 8 {
        return None;
    }

    let xs: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = pairs.iter().map(|p| p.1).collect();
    let (mx, my) = (mean(&xs)?, mean(&ys)?);
    let cov: f64 = pairs.iter().map(|(x, y)| (x - mx) * (y - my)).sum();
    let vx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum::<f64>().sqrt();
    let vy: f64 = ys.iter().map(|y| (y - my).powi(2)).sum::<f64>().sqrt();
    if vx == 0.0 || vy == 0.0 {
        return None;
    }
    let r = cov / (vx * vy);
    let n = pairs.len();

    // Below a moderate correlation there is nothing here worth a paragraph.
    if r.abs() < 0.3 {
        return None;
    }

    Some(SleepInsight {
        id: "hrv-link".into(),
        tone: if r > 0.0 { Tone::Note } else { Tone::Watch },
        claim: if r > 0.0 {
            "Your longer nights are also your higher-HRV nights.".into()
        } else {
            "Your longer nights are running with *lower* HRV.".into()
        },
        detail: format!(
            "Across {n} nights, hours asleep and next-morning HRV correlate at r = {r:.2}. \
             Correlation over one window, not cause — HRV is the strongest recovery signal you \
             have, so it's worth knowing which way it moves with sleep, but a long night and a \
             good HRV usually share a cause rather than making each other."
        ),
        nights: n,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of a real payload, cut down to the fields the parser reads.
    fn payload() -> Value {
        serde_json::json!({
            "avgOvernightHrv": 89.0,
            "restingHeartRate": 48,
            "restlessMomentsCount": 62,
            "bodyBatteryChange": 83,
            "dailySleepDTO": {
                "sleepTimeSeconds": 31200,
                "deepSleepSeconds": 3960,
                "lightSleepSeconds": 19800,
                "remSleepSeconds": 7440,
                "awakeSleepSeconds": 180,
                "napTimeSeconds": 0,
                "awakeCount": 0,
                "avgSleepStress": 10.0,
                "avgHeartRate": 54.0,
                "averageRespirationValue": 13.0,
                "lowestRespirationValue": 11.0,
                "highestRespirationValue": 17.0,
                "averageSpO2Value": 97.0,
                "lowestSpO2Value": 90,
                "sleepStartTimestampGMT": 1786316322000f64,
                "sleepStartTimestampLocal": 1786323522000f64,
                "sleepEndTimestampGMT": 1786347702000f64,
                "sleepEndTimestampLocal": 1786354902000f64,
                "sleepNeed": { "actual": 480, "baseline": 480 },
                "sleepScoreFeedback": "POSITIVE_HIGHLY_RECOVERING",
                "sleepScores": {
                    "overall": { "value": 93, "qualifierKey": "EXCELLENT" },
                    "deepPercentage": {
                        "value": 13, "qualifierKey": "FAIR",
                        "optimalStart": 16.0, "optimalEnd": 33.0
                    },
                    "remPercentage": {
                        "value": 24, "qualifierKey": "EXCELLENT",
                        "optimalStart": 21.0, "optimalEnd": 31.0
                    }
                }
            },
            "sleepLevels": [
                { "activityLevel": 1.0, "startGMT": "2026-08-09T22:58:42.0", "endGMT": "2026-08-09T23:12:42.0" },
                { "activityLevel": 0.0, "startGMT": "2026-08-09T23:12:42.0", "endGMT": "2026-08-09T23:30:42.0" },
                { "activityLevel": 3.0, "startGMT": "2026-08-09T23:30:42.0", "endGMT": "2026-08-09T23:32:42.0" }
            ],
            "sleepHeartRate": [
                // The first of these is from before sleep began, as Garmin's
                // real series always is.
                { "startGMT": 1786316280000f64, "value": 71 },
                { "startGMT": 1786316340000f64, "value": 60 },
                { "startGMT": 1786316400000f64, "value": 58 },
                { "startGMT": 1786316700000f64, "value": 54 }
            ]
        })
    }

    #[test]
    fn a_night_parses_whole() {
        let n = SleepNight::from_payload("2026-08-10", &payload());
        assert_eq!(n.score, Some(93.0));
        assert_eq!(n.total_secs, Some(31200.0));
        assert_eq!(n.need_secs, Some(28800.0));
        assert_eq!(n.avg_overnight_hrv, Some(89.0));
        // The overall score is lifted out into `score` and `score_qualifier`;
        // what's left in the parts are the components it was built from.
        assert_eq!(n.score_parts.len(), 2);
        // Garmin sends the local epoch two hours ahead of the GMT one here, and
        // the parsed wall-clock time has to be the local one.
        assert_eq!(n.start_local.as_deref(), Some("2026-08-10T00:58:42"));
    }

    #[test]
    fn stages_carry_local_times_and_the_right_levels() {
        let n = SleepNight::from_payload("2026-08-10", &payload());
        assert_eq!(n.stages.len(), 3);
        assert_eq!(n.stages[0].stage, Stage::Light);
        assert_eq!(n.stages[1].stage, Stage::Deep);
        assert_eq!(n.stages[2].stage, Stage::Awake);
        // Two hours on from the GMT stamp, same as the sleep start.
        assert_eq!(n.stages[0].start_local, "2026-08-10T00:58:42");
        assert_eq!(n.stages[0].from_start_mins, 0.0);
        assert_eq!(n.stages[1].secs, 1080.0);
    }

    #[test]
    fn heart_rate_thins_into_five_minute_buckets() {
        let n = SleepNight::from_payload("2026-08-10", &payload());
        // The pre-sleep reading is dropped, the two inside the first five
        // minutes average, and the last opens a bucket of its own.
        assert_eq!(n.hr.len(), 2);
        assert_eq!(n.hr[0].bpm, 59.0);
        assert_eq!(n.hr[1].from_start_mins, 5.0);
        assert_eq!(n.hr[1].bpm, 54.0);
    }

    #[test]
    fn an_empty_payload_is_a_night_with_nothing_in_it() {
        let n = SleepNight::from_payload("2026-08-10", &serde_json::json!({}));
        assert!(!n.has_data());
        assert!(n.stages.is_empty());
    }

    #[test]
    fn bedtimes_either_side_of_midnight_stay_in_order() {
        assert!(mins_past_six_pm("2026-08-09T22:58:00") < mins_past_six_pm("2026-08-10T01:30:00"));
        assert_eq!(mins_past_six_pm("2026-08-09T18:00:00"), Some(0.0));
        assert_eq!(mins_past_six_pm("2026-08-10T07:41:00"), Some(821.0));
    }

    fn night(date: &str, start: &str, secs: f64, score: f64) -> SleepNight {
        SleepNight {
            date: date.into(),
            score: Some(score),
            total_secs: Some(secs),
            deep_secs: Some(secs * 0.15),
            light_secs: Some(secs * 0.6),
            rem_secs: Some(secs * 0.25),
            awake_secs: Some(600.0),
            start_local: Some(start.into()),
            ..Default::default()
        }
    }

    #[test]
    fn averages_skip_the_nights_that_have_nothing() {
        let nights = vec![
            night("2026-08-10", "2026-08-09T23:00:00", 28800.0, 90.0),
            night("2026-08-09", "2026-08-08T23:00:00", 25200.0, 80.0),
            SleepNight {
                date: "2026-08-08".into(),
                ..Default::default()
            },
        ];
        let a = averages(&nights);
        assert_eq!(a.nights, 3);
        // 8h and 7h, and the empty night is not a third night of zero.
        assert_eq!(a.total_secs, Some(27000.0));
        assert_eq!(a.short_nights, 0);
    }

    #[test]
    fn a_swinging_bedtime_is_called_out() {
        // Five nights alternating between 21:00 and 02:00.
        let nights: Vec<SleepNight> = ["23:00", "02:00", "21:00", "01:30", "22:00"]
            .iter()
            .enumerate()
            .map(|(i, t)| {
                night(
                    &format!("2026-08-{:02}", 10 - i),
                    &format!("2026-08-{:02}T{t}:00", 9 - i),
                    27000.0,
                    85.0,
                )
            })
            .collect();

        let out = insights(&nights, &[], &[]);
        let c = out.iter().find(|i| i.id == "consistency").unwrap();
        assert_eq!(c.tone, Tone::Watch);
    }

    #[test]
    fn too_few_nights_says_nothing_at_all() {
        let nights = vec![night("2026-08-10", "2026-08-09T23:00:00", 28800.0, 90.0)];
        assert!(insights(&nights, &[], &[]).is_empty());
    }
}
