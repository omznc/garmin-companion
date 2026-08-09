//! The one part of this app that speaks first.
//!
//! Everything else answers a question that was asked. This looks at the week so
//! far and decides whether anything is worth saying — which is usually nothing,
//! and saying nothing is the design goal rather than a failure mode. An app that
//! nudges every day is one whose notifications get turned off in a fortnight.
//!
//! Three rules keep it honest:
//!
//! - **Every nudge carries its evidence.** No line of copy asserts something the
//!   `evidence` field can't show the numbers for.
//! - **Nothing is invented.** A rule that needs data the cache doesn't have
//!   doesn't fire — it does not guess, and it does not fire a weaker version.
//! - **It notices what went right.** A coach that only ever reports problems is
//!   one you learn to dread, and the recovery signal here is genuinely good.
//!
//! [`evaluate`] is pure: everything it reads is in [`CoachContext`], so the
//! rules can be tested against a hand-built week without a database.

use chrono::{Datelike, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::db::{CachedActivity, DailyMetrics, FitnessDay};
use crate::goals::{week_start, Goals, WeekProgress};

/// Stable identity for a nudge, so the same one can be recognised across days
/// and counted rather than repeated as if it were new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NudgeKind {
    /// The week's long easy run hasn't happened and the week is running out.
    LongRunMissing,
    /// Garmin's own monthly load balance has anaerobic work over its target.
    AnaerobicOverTarget,
    /// Hard-effort share climbing across consecutive runs.
    HardDrift,
    /// Step rate well under the target.
    CadenceLow,
    /// Acute load has run away from chronic — Garmin's own ratio.
    LoadSpike,
    /// Recovery is good and nothing hard has happened for a while.
    GreenLight,
    /// No outdoor GPS run, so VO2 max still can't be computed.
    NoOutdoorRun,
    /// The week's goals were all met.
    WeekComplete,
}

impl NudgeKind {
    /// The stable string form, used as the dedupe key and in the deep link.
    pub fn id(self) -> &'static str {
        match self {
            Self::LongRunMissing => "long-run-missing",
            Self::AnaerobicOverTarget => "anaerobic-over-target",
            Self::HardDrift => "hard-drift",
            Self::CadenceLow => "cadence-low",
            Self::LoadSpike => "load-spike",
            Self::GreenLight => "green-light",
            Self::NoOutdoorRun => "no-outdoor-run",
            Self::WeekComplete => "week-complete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    /// Something went right.
    Good,
    /// Worth knowing, no action implied.
    Neutral,
    /// Worth acting on.
    Watch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Nudge {
    pub kind: NudgeKind,
    pub id: String,
    /// One line, written to fit a notification title.
    pub title: String,
    /// A sentence or two. Never asserts anything `evidence` can't support.
    pub body: String,
    pub tone: Tone,
    /// Higher fires first when only one can be shown.
    pub priority: u8,
    /// The numbers behind it, each already formatted for display.
    pub evidence: Vec<String>,
}

/// Everything the rules are allowed to read.
pub struct CoachContext<'a> {
    pub today: NaiveDate,
    pub goals: &'a Goals,
    pub week: &'a WeekProgress,
    /// Newest first, covering at least the last month.
    pub activities: &'a [CachedActivity],
    /// Newest first.
    pub daily: &'a [DailyMetrics],
    /// Garmin's latest verdict, when it has been synced.
    pub fitness: Option<&'a FitnessDay>,
}

/* --------------------------------------------------------------- thresholds --- */

/// The day of the week the long-run nudge starts appearing. Thursday: early
/// enough that there is still a weekend to do it in, late enough that it isn't
/// nagging about a week that has barely started.
const LONG_RUN_NUDGE_FROM: u32 = 3;

/// How many recent runs the drift rule looks across, and how much the hard
/// share has to climb over them to count as drifting.
const DRIFT_RUNS: usize = 3;
const DRIFT_RISE_PCT: f64 = 15.0;

/// How far under the cadence goal counts as low. Wide, because a beginner's
/// step rate moves slowly and a nudge every week about the same 8 spm is noise.
const CADENCE_SLACK_SPM: f64 = 15.0;

/// Above this, Garmin considers acute load to have run away from chronic.
const ACWR_HIGH: f64 = 1.5;

/// What "recovered" means for the green-light rule: HRV in its normal band or
/// better, and readiness at least this.
const READY_SCORE: f64 = 70.0;

/// How many days without a hard session before the green light is worth saying.
const GREEN_LIGHT_QUIET_DAYS: i64 = 3;

/// How often the outdoor-run reminder is allowed to come round. Long, because
/// nothing changes between one week and the next.
const OUTDOOR_REMINDER_DAYS: i64 = 21;

/// How old a recovery or fitness reading may be and still describe now.
///
/// Readiness, HRV and the acute:chronic ratio are all statements about today,
/// and a watch left in a drawer doesn't produce a wrong one — it produces no
/// new one, leaving the last good reading sitting at the top of the table. Two
/// days of slack covers a watch charging overnight; past that the rules that
/// speak in the present tense have to go quiet rather than quote history as
/// though it were this morning.
const FRESH_DAYS: i64 = 2;

/// The newest row, if it is recent enough to be talking about now.
fn fresh<'a, T>(row: Option<&'a T>, date: &dyn Fn(&T) -> &str, today: NaiveDate) -> Option<&'a T> {
    let row = row?;
    (crate::days_between(date(row), today)? <= FRESH_DAYS).then_some(row)
}

/* --------------------------------------------------------------------- rules --- */

fn hard_share(a: &CachedActivity) -> Option<f64> {
    let total: f64 = a.zone_secs.iter().sum();
    (total > 0.0).then(|| (a.zone_secs[2] + a.zone_secs[3] + a.zone_secs[4]) / total * 100.0)
}

fn date_of(a: &CachedActivity) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(a.local_date.as_deref()?, "%Y-%m-%d").ok()
}

fn mins(a: &CachedActivity) -> f64 {
    a.duration_s.unwrap_or(0.0) / 60.0
}

/// Every nudge the week currently earns, highest priority first.
///
/// Usually empty. The caller decides how many to show and which one, if any,
/// becomes a notification.
pub fn evaluate(ctx: &CoachContext<'_>) -> Vec<Nudge> {
    let mut out = Vec::new();
    let runs: Vec<&CachedActivity> = ctx
        .activities
        .iter()
        .filter(|a| crate::query::is_run(a))
        .collect();

    out.extend(long_run_missing(ctx));
    out.extend(anaerobic_over_target(ctx));
    out.extend(hard_drift(&runs));
    out.extend(cadence_low(ctx, &runs));
    out.extend(load_spike(ctx));
    out.extend(green_light(ctx, &runs));
    out.extend(no_outdoor_run(ctx, &runs));
    out.extend(week_complete(ctx));

    out.sort_by_key(|n| std::cmp::Reverse(n.priority));
    out
}

fn long_run_missing(ctx: &CoachContext<'_>) -> Option<Nudge> {
    let target = ctx.goals.long_run_minutes?;
    if ctx.today.weekday().num_days_from_monday() < LONG_RUN_NUDGE_FROM {
        return None;
    }
    let longest = ctx.week.longest_run_minutes;
    if longest >= target {
        return None;
    }

    let left = 6 - ctx.today.weekday().num_days_from_monday();
    let days = match left {
        0 => "Today is the last day of the week".to_string(),
        1 => "One day left this week".to_string(),
        n => format!("{n} days left this week"),
    };

    Some(Nudge {
        kind: NudgeKind::LongRunMissing,
        id: NudgeKind::LongRunMissing.id().into(),
        title: "The long easy run hasn't happened yet".to_string(),
        body: format!(
            "{days}. The longest run so far is {:.0} min against a {:.0} min target. \
             Duration is the goal, not pace — walk breaks are fine and keeping the \
             HR down is the point.",
            longest, target
        ),
        tone: Tone::Watch,
        priority: 90,
        evidence: vec![
            format!("Longest run this week: {longest:.0} min"),
            format!("Target: {target:.0} min"),
        ],
    })
}

fn anaerobic_over_target(ctx: &CoachContext<'_>) -> Option<Nudge> {
    // A rolling month's load balance, so it too describes now.
    let s = &fresh(ctx.fitness, &|f: &crate::db::FitnessDay| &f.date, ctx.today)?.status;
    if !s.anaerobic_over_target() {
        return None;
    }
    let (anaerobic, max) = (s.anaerobic?, s.anaerobic_target_max?);
    let over = (anaerobic / max - 1.0) * 100.0;

    Some(Nudge {
        kind: NudgeKind::AnaerobicOverTarget,
        id: NudgeKind::AnaerobicOverTarget.id().into(),
        title: "Garmin has your anaerobic load over target".into(),
        body: format!(
            "The month's anaerobic load is {anaerobic:.0} against a ceiling of {max:.0} — \
             {over:.0}% over. This is Garmin's own accounting, not this app's, and it \
             agrees with the zone numbers: the hard sessions are fine, there just \
             isn't enough easy work underneath them."
        ),
        tone: Tone::Watch,
        priority: 85,
        evidence: {
            let mut e = vec![format!(
                "Anaerobic load: {anaerobic:.0} (target {:.0}–{max:.0})",
                s.anaerobic_target_min.unwrap_or(0.0)
            )];
            if let Some(p) = &s.balance_phrase {
                e.push(format!("Garmin's balance verdict: {p}"));
            }
            if let (Some(low), Some(lmin)) = (s.aerobic_low, s.aerobic_low_target_min) {
                e.push(format!(
                    "Low aerobic load: {low:.0} (target from {lmin:.0})"
                ));
            }
            e
        },
    })
}

fn hard_drift(runs: &[&CachedActivity]) -> Option<Nudge> {
    let recent: Vec<(&CachedActivity, f64)> = runs
        .iter()
        .filter_map(|a| Some((*a, hard_share(a)?)))
        .take(DRIFT_RUNS)
        .collect();
    if recent.len() < DRIFT_RUNS {
        return None;
    }

    // `runs` is newest first, so the oldest of the three is last.
    let newest = recent.first()?.1;
    let oldest = recent.last()?.1;
    let rise = newest - oldest;
    if rise < DRIFT_RISE_PCT {
        return None;
    }

    Some(Nudge {
        kind: NudgeKind::HardDrift,
        id: NudgeKind::HardDrift.id().into(),
        title: "Hard-effort share is climbing again".into(),
        body: format!(
            "Across the last {DRIFT_RUNS} runs the Z3–Z5 share went from {oldest:.0}% \
             to {newest:.0}%. Not a problem by itself — it's the pattern that \
             preceded the last drift back into short, hard-only weeks."
        ),
        tone: Tone::Watch,
        priority: 70,
        evidence: recent
            .iter()
            .rev()
            .map(|(a, pct)| {
                format!(
                    "{}: {pct:.0}% hard, {:.0} min",
                    a.local_date.as_deref().unwrap_or("?"),
                    mins(a)
                )
            })
            .collect(),
    })
}

fn cadence_low(ctx: &CoachContext<'_>, runs: &[&CachedActivity]) -> Option<Nudge> {
    let target = ctx.goals.cadence_spm?;
    let recent: Vec<f64> = runs.iter().filter_map(|a| a.avg_cadence).take(3).collect();
    // One run is an anecdote; the rule needs a habit.
    if recent.len() < 2 {
        return None;
    }
    let avg = recent.iter().sum::<f64>() / recent.len() as f64;
    if avg >= target - CADENCE_SLACK_SPM {
        return None;
    }

    Some(Nudge {
        kind: NudgeKind::CadenceLow,
        id: NudgeKind::CadenceLow.id().into(),
        title: format!("Cadence is sitting around {avg:.0} spm"),
        body: format!(
            "Target is about {target:.0}. Quicker, lighter steps at the same speed \
             cut the load each one puts through the knee — which matters more at \
             your bodyweight than it would at 70 kg. Shorten the stride rather \
             than pushing the pace."
        ),
        tone: Tone::Neutral,
        priority: 40,
        evidence: recent
            .iter()
            .map(|c| format!("Recent run: {c:.0} spm"))
            .collect(),
    })
}

fn load_spike(ctx: &CoachContext<'_>) -> Option<Nudge> {
    // Same reasoning as `green_light`: "this week's load" has to be this week's.
    let s = &fresh(ctx.fitness, &|f: &crate::db::FitnessDay| &f.date, ctx.today)?.status;
    let acwr = s.acwr?;
    if acwr < ACWR_HIGH {
        return None;
    }

    Some(Nudge {
        kind: NudgeKind::LoadSpike,
        id: NudgeKind::LoadSpike.id().into(),
        title: "This week's load has run ahead of the base".into(),
        // Reports the ratio as Garmin's opinion, which is all it is. The old
        // copy said 1.5 was "where the injury numbers start to climb", and that
        // claim doesn't survive the literature: the acute:chronic ratio is
        // mathematically coupled — the latest week sits in both halves — and
        // when the underlying data is treated as continuous rather than binned,
        // the relationship with injury largely disappears. It remains a
        // reasonable prompt to look at a week that jumped. It is not a risk
        // figure, and this app shouldn't dress it as one.
        body: format!(
            "Garmin puts the acute:chronic ratio at {acwr:.2}, which is its way of \
             saying this week ran well ahead of what the last month built. Treat \
             that as a prompt to look rather than as a risk score — the threshold \
             it's measured against is a rule of thumb the evidence has not been \
             kind to. If the week did feel like a jump, an easy week answers it \
             better than a rest day."
        ),
        tone: Tone::Watch,
        priority: 95,
        evidence: vec![
            format!("Acute load: {:.0}", s.acute_load.unwrap_or_default()),
            format!("Chronic load: {:.0}", s.chronic_load.unwrap_or_default()),
            format!(
                "Ratio: {acwr:.2} ({})",
                s.acwr_status.as_deref().unwrap_or("?")
            ),
        ],
    })
}

fn green_light(ctx: &CoachContext<'_>, runs: &[&CachedActivity]) -> Option<Nudge> {
    // "You're recovered, go hard" is a claim about this morning. Said off a
    // reading from six weeks ago it is the most expensive thing this file can
    // get wrong, so the freshness check comes before anything else.
    let latest = fresh(
        ctx.daily.first(),
        &|d: &crate::DailyMetrics| &d.date,
        ctx.today,
    )?;
    let readiness = latest.training_readiness?;
    if readiness < READY_SCORE {
        return None;
    }
    // A good HRV status is the stronger signal of the two, and the one this
    // athlete's data has been consistent on.
    let hrv_ok = latest
        .hrv_status
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("balanced") || s.eq_ignore_ascii_case("good"));
    if !hrv_ok {
        return None;
    }

    // "Balanced" is a comparison against a personal range, and the range is
    // only as good as the nights that built it — Garmin wants about four a
    // week. Below that the status still reads Balanced, but it is measuring
    // against a baseline drawn from too little, which is not the clearance it
    // looks like. Nothing else in this file can catch that: the reading is
    // present, fresh, and says the right word.
    //
    // The denominator is the calendar window, deliberately — not `daily.len()`.
    // A row only exists for a day that recorded something, so counting rows
    // would divide six nights of wear by six rows, call it seven a week, and
    // wave through exactly the sparse baseline this check exists to catch.
    let nights = ctx
        .daily
        .iter()
        .filter(|d| d.hrv_last_night.is_some())
        .count();
    if !crate::signal::hrv_coverage(nights, WINDOW_DAYS as usize).sufficient {
        return None;
    }

    // Quiet means nothing hard, not nothing at all — an easy jog doesn't spend
    // the readiness this rule is reporting.
    let last_hard = ctx
        .activities
        .iter()
        .filter(|a| hard_share(a).is_some_and(|p| p > 30.0))
        .filter_map(date_of)
        .max()?;
    let quiet_days = (ctx.today - last_hard).num_days();
    if quiet_days < GREEN_LIGHT_QUIET_DAYS {
        return None;
    }
    // Don't hand out a green light in the same breath as a load warning.
    if ctx
        .fitness
        .and_then(|f| f.status.acwr)
        .is_some_and(|r| r >= ACWR_HIGH)
    {
        return None;
    }
    let _ = runs;

    Some(Nudge {
        kind: NudgeKind::GreenLight,
        id: NudgeKind::GreenLight.id().into(),
        title: "Recovered — this is a good day to go hard".into(),
        body: format!(
            "Readiness {readiness:.0}, HRV flagged {}, and {quiet_days} days since \
             the last hard session. If you want one of the short sharp ones, today \
             is the day the data supports it.",
            latest.hrv_status.as_deref().unwrap_or("normal")
        ),
        tone: Tone::Good,
        priority: 60,
        evidence: vec![
            format!("Training readiness: {readiness:.0}"),
            format!(
                "HRV last night: {} ms (weekly avg {})",
                latest
                    .hrv_last_night
                    .map(|v| format!("{v:.0}"))
                    .unwrap_or_else(|| "—".into()),
                latest
                    .hrv_weekly_avg
                    .map(|v| format!("{v:.0}"))
                    .unwrap_or_else(|| "—".into())
            ),
            format!("Last hard session: {last_hard}"),
        ],
    })
}

fn no_outdoor_run(ctx: &CoachContext<'_>, runs: &[&CachedActivity]) -> Option<Nudge> {
    // Only worth saying if VO2 max is actually missing, which is the thing an
    // outdoor run would fix.
    if ctx.fitness.is_some_and(|f| f.status.vo2max.is_some()) {
        return None;
    }
    if runs.is_empty() {
        return None;
    }
    // A treadmill run reports no elevation gain and carries no GPS trace; the
    // cache's honest signal for "was this outdoors" is the trace.
    let outdoors = runs
        .iter()
        .any(|a| a.elevation_gain.is_some_and(|m| m > 0.0));
    if outdoors {
        return None;
    }
    // Anchored to the oldest run in the window rather than to "now", so this
    // comes round on a schedule instead of every single day.
    let oldest = runs.iter().filter_map(|a| date_of(a)).min()?;
    if (ctx.today - oldest).num_days() % OUTDOOR_REMINDER_DAYS != 0 {
        return None;
    }

    Some(Nudge {
        kind: NudgeKind::NoOutdoorRun,
        id: NudgeKind::NoOutdoorRun.id().into(),
        title: "Still no VO2 max — it needs one run outdoors".into(),
        body: format!(
            "All {} runs in the window were indoors. Garmin only computes VO2 max \
             from outdoor GPS runs, so the number stays blank until one happens. \
             One easy 20 minutes outside is enough to start it.",
            runs.len()
        ),
        tone: Tone::Neutral,
        priority: 30,
        evidence: vec![format!("Runs in the window: {}", runs.len())],
    })
}

fn week_complete(ctx: &CoachContext<'_>) -> Option<Nudge> {
    if ctx.week.rings.is_empty() || !ctx.week.rings.iter().all(|r| r.met) {
        return None;
    }
    // Only worth saying once the week has enough in it to be an achievement.
    if ctx.week.sessions < 2 {
        return None;
    }

    Some(Nudge {
        kind: NudgeKind::WeekComplete,
        id: NudgeKind::WeekComplete.id().into(),
        title: "Every goal met this week".into(),
        body: format!(
            "{} sessions, {:.0} minutes, and the long run went to {:.0}. That's the \
             week as designed.",
            ctx.week.sessions, ctx.week.minutes, ctx.week.longest_run_minutes
        ),
        tone: Tone::Good,
        priority: 50,
        evidence: ctx
            .week
            .rings
            .iter()
            .map(|r| format!("{}: {} / {} {}", r.label, r.actual, r.target, r.unit))
            .collect(),
    })
}

/// The week `date` belongs to, as a stable string — the dedupe window for
/// nudges that should be said at most once a week.
pub fn week_key(date: NaiveDate) -> String {
    week_start(date).format("%Y-%m-%d").to_string()
}

/* ------------------------------------------------------------------- report --- */

/// How much history the rules get to see. Four weeks: enough for the drift and
/// green-light rules, short enough that a run from March isn't evidence about
/// this week.
const WINDOW_DAYS: i64 = 28;

/// A nudge together with what the cache remembers about it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandingNudge {
    #[serde(flatten)]
    pub nudge: Nudge,
    /// How many days running this has been saying the same thing. 1 means it's
    /// new today.
    pub days_running: i64,
    pub first_seen: String,
    /// True when it was dismissed today and shouldn't be shown again until
    /// tomorrow.
    pub dismissed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachReport {
    pub date: String,
    pub week: WeekProgress,
    /// Everything that fired, highest priority first, dismissed ones included
    /// and flagged. Frequently empty, which is the intended resting state.
    pub nudges: Vec<StandingNudge>,
}

impl CoachReport {
    /// The one worth putting in a notification: the highest-priority nudge that
    /// hasn't been dismissed today. `None` most days.
    pub fn headline(&self) -> Option<&StandingNudge> {
        self.nudges.iter().find(|n| !n.dismissed)
    }
}

/* ----------------------------------------------------------- notifications --- */

const NOTIFY_KEY: &str = "coach_notifications";

/// How many days ahead the plan reaches.
///
/// The app cannot run in the background, so every notification has to be handed
/// to the system in advance, from a plan that is only refreshed when the app is
/// next opened or synced. One day ahead would mean the chain dies the first time
/// a nudge is ignored; a repeating alarm would mean text frozen forever. Four
/// covers a weekend of not opening the app and then stops, which is the right
/// behaviour for an app that has been abandoned.
const HORIZON_DAYS: u32 = 4;

/// When the coach is allowed to interrupt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotifySettings {
    pub enabled: bool,
    /// Local hour, 0–23.
    pub hour: u32,
}

impl Default for NotifySettings {
    fn default() -> Self {
        // Six in the evening: late enough that the day's training has either
        // happened or hasn't, early enough that "go tomorrow" is still a plan
        // rather than a regret.
        Self {
            enabled: true,
            hour: 18,
        }
    }
}

impl NotifySettings {
    pub fn load(db: &crate::Db) -> anyhow::Result<Self> {
        Ok(db
            .sync_state(NOTIFY_KEY)?
            .filter(|s| !s.is_empty())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default())
    }

    pub fn save(&self, db: &crate::Db) -> anyhow::Result<()> {
        db.set_sync_state(NOTIFY_KEY, &serde_json::to_string(self)?)
    }

    /// Clamped to a real hour. A stored 25 is a bug somewhere, not a reason to
    /// schedule nothing.
    pub fn hour(&self) -> u32 {
        self.hour.min(23)
    }
}

/// One notification the system has been asked to deliver, and when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedNudge {
    pub nudge_id: String,
    pub title: String,
    pub body: String,
    /// Local time it fires.
    pub at: NaiveDateTime,
    /// 0 for the next one due, counting up from there.
    pub day: u32,
}

/// The notification body: one sentence, and after the first day an admission of
/// how old it is.
///
/// Everything scheduled beyond the first is text frozen at planning time, and
/// the honest framing is that this is exactly as stale as the app itself — no
/// sync has happened either, so opening the app right then would say the same
/// thing. Saying when it was worked out lets that be checked rather than
/// assumed.
fn notification_body(nudge: &Nudge, planned_on: NaiveDate, day: u32) -> String {
    let first = nudge
        .body
        .split_once(". ")
        .map(|(a, _)| a)
        .unwrap_or(&nudge.body)
        .to_string();
    if day == 0 {
        first
    } else {
        format!(
            "{first} (worked out {}, and nothing has synced since.)",
            planned_on.format("%A")
        )
    }
}

/// What to hand the system, given today's report.
///
/// Pure, so the awkward parts — an hour that has already passed, a report with
/// nothing to say, notifications switched off — are decided here and testable,
/// leaving the platform layer with nothing but the scheduling calls.
pub fn plan_notifications(
    report: &CoachReport,
    settings: &NotifySettings,
    now: NaiveDateTime,
) -> Vec<PlannedNudge> {
    if !settings.enabled {
        return Vec::new();
    }
    let Some(headline) = report.headline() else {
        return Vec::new();
    };
    let Some(at_hour) = now.date().and_hms_opt(settings.hour(), 0, 0) else {
        return Vec::new();
    };

    // Today's slot if it is still ahead, otherwise start tomorrow. Strictly
    // ahead: scheduling into the past would fire the whole run at once.
    let first = if at_hour > now {
        at_hour
    } else {
        at_hour + chrono::Duration::days(1)
    };

    (0..HORIZON_DAYS)
        .map(|day| PlannedNudge {
            nudge_id: headline.nudge.id.clone(),
            title: headline.nudge.title.clone(),
            body: notification_body(&headline.nudge, now.date(), day),
            at: first + chrono::Duration::days(day as i64),
            day,
        })
        .collect()
}

/// Evaluate the coach against the cache, recording what fired.
///
/// Writing on a read is deliberate: `days_running` is the count of days a nudge
/// has been standing, and the only moment it can be observed is the moment the
/// rules fire. The write is idempotent within a day.
pub fn for_today(db: &crate::Db, today: NaiveDate) -> anyhow::Result<CoachReport> {
    let today_str = today.format("%Y-%m-%d").to_string();
    let from = (today - chrono::Duration::days(WINDOW_DAYS))
        .format("%Y-%m-%d")
        .to_string();

    let goals = Goals::load(db)?;
    let activities = db.activities_since(&from)?;
    let daily = db.daily_since(&from)?;
    let fitness = db.fitness_since(&from)?.into_iter().next();
    let week = crate::goals::week_progress(&goals, &activities, today);

    let nudges = evaluate(&CoachContext {
        today,
        goals: &goals,
        week: &week,
        activities: &activities,
        daily: &daily,
        fitness: fitness.as_ref(),
    });

    let mut standing = Vec::new();
    for nudge in nudges {
        let state = db.saw_nudge(&nudge.id, &today_str)?;
        standing.push(StandingNudge {
            days_running: state.times_seen,
            first_seen: state.first_seen,
            dismissed: state.dismissed_on.as_deref() == Some(today_str.as_str()),
            nudge,
        });
    }

    Ok(CoachReport {
        date: today_str,
        week,
        nudges: standing,
    })
}

#[cfg(test)]
mod notification_tests {
    use super::*;

    fn report(nudges: Vec<(&str, bool)>) -> CoachReport {
        CoachReport {
            date: "2026-08-08".into(),
            week: crate::goals::week_progress(&Goals::default(), &[], date("2026-08-08")),
            nudges: nudges
                .into_iter()
                .map(|(id, dismissed)| StandingNudge {
                    nudge: Nudge {
                        kind: NudgeKind::LongRunMissing,
                        id: id.into(),
                        title: "The long easy run hasn't happened yet".into(),
                        body: "Longest run this week is 11 minutes against a 30 minute goal. \
                               There are two days left to put one in."
                            .into(),
                        tone: Tone::Watch,
                        priority: 90,
                        evidence: vec!["Longest run: 11 min".into()],
                    },
                    days_running: 1,
                    first_seen: "2026-08-08".into(),
                    dismissed,
                })
                .collect(),
        }
    }

    fn date(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn the_plan_starts_today_when_the_hour_is_still_ahead() {
        let plan = plan_notifications(
            &report(vec![("long-run-missing", false)]),
            &NotifySettings::default(),
            at("2026-08-08 09:00:00"),
        );
        assert_eq!(plan.len(), HORIZON_DAYS as usize);
        assert_eq!(plan[0].at, at("2026-08-08 18:00:00"));
        assert_eq!(plan[3].at, at("2026-08-11 18:00:00"));
    }

    #[test]
    fn the_plan_starts_tomorrow_once_the_hour_has_gone() {
        let plan = plan_notifications(
            &report(vec![("long-run-missing", false)]),
            &NotifySettings::default(),
            at("2026-08-08 20:30:00"),
        );
        assert_eq!(plan[0].at, at("2026-08-09 18:00:00"));
    }

    /// The boundary is the one that would misfire: scheduling *at* now means
    /// handing the system an alarm it considers overdue, which fires at once.
    #[test]
    fn the_hour_exactly_now_is_treated_as_gone() {
        let plan = plan_notifications(
            &report(vec![("long-run-missing", false)]),
            &NotifySettings::default(),
            at("2026-08-08 18:00:00"),
        );
        assert_eq!(plan[0].at, at("2026-08-09 18:00:00"));
    }

    /// Only the first is current. The rest have to say so, because by then no
    /// sync has happened and the numbers in them are days old.
    #[test]
    fn only_the_first_notification_speaks_as_of_today() {
        let plan = plan_notifications(
            &report(vec![("long-run-missing", false)]),
            &NotifySettings::default(),
            at("2026-08-08 09:00:00"),
        );
        assert_eq!(
            plan[0].body,
            "Longest run this week is 11 minutes against a 30 minute goal"
        );
        assert!(plan[1].body.contains("worked out Saturday"));
        assert!(plan[1].body.contains("nothing has synced since"));
    }

    #[test]
    fn a_dismissed_nudge_is_not_scheduled() {
        let plan = plan_notifications(
            &report(vec![("long-run-missing", true)]),
            &NotifySettings::default(),
            at("2026-08-08 09:00:00"),
        );
        assert!(plan.is_empty());
    }

    #[test]
    fn nothing_to_say_schedules_nothing() {
        let plan = plan_notifications(
            &report(vec![]),
            &NotifySettings::default(),
            at("2026-08-08 09:00:00"),
        );
        assert!(plan.is_empty());
    }

    #[test]
    fn switched_off_schedules_nothing() {
        let plan = plan_notifications(
            &report(vec![("long-run-missing", false)]),
            &NotifySettings {
                enabled: false,
                hour: 18,
            },
            at("2026-08-08 09:00:00"),
        );
        assert!(plan.is_empty());
    }

    /// A stored hour out of range should cost the day's nudge, not schedule at
    /// a time that doesn't exist.
    #[test]
    fn an_impossible_hour_is_clamped_rather_than_dropped() {
        let plan = plan_notifications(
            &report(vec![("long-run-missing", false)]),
            &NotifySettings {
                enabled: true,
                hour: 99,
            },
            at("2026-08-08 09:00:00"),
        );
        assert_eq!(plan[0].at, at("2026-08-08 23:00:00"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goals::week_progress;

    fn run(date: &str, mins: f64, zones: [f64; 5], cadence: Option<f64>) -> CachedActivity {
        CachedActivity {
            activity_id: date.replace('-', "").parse::<i64>().unwrap_or(1) + mins as i64,
            name: Some("Run".into()),
            type_key: Some("treadmill_running".into()),
            start_time_local: Some(format!("{date} 10:00:00")),
            local_date: Some(date.into()),
            distance_m: Some(2000.0),
            duration_s: Some(mins * 60.0),
            moving_duration_s: None,
            avg_hr: Some(150.0),
            max_hr: Some(180.0),
            avg_cadence: cadence,
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: zones,
        }
    }

    /// Saturday.
    fn saturday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 8).unwrap()
    }

    fn ctx<'a>(
        today: NaiveDate,
        goals: &'a Goals,
        week: &'a WeekProgress,
        acts: &'a [CachedActivity],
        daily: &'a [DailyMetrics],
    ) -> CoachContext<'a> {
        CoachContext {
            today,
            goals,
            week,
            activities: acts,
            daily,
            fitness: None,
        }
    }

    fn fired(nudges: &[Nudge], kind: NudgeKind) -> bool {
        nudges.iter().any(|n| n.kind == kind)
    }

    /// A run of nights ending on `newest`, each carrying an HRV reading.
    ///
    /// The green light needs a baseline built on about four nights a week
    /// before it will speak, so a fixture with one good night no longer
    /// exercises the rest of the rule. `nights` consecutive days clears it.
    fn worn_nights(newest: &str, nights: usize, readiness: f64) -> Vec<DailyMetrics> {
        let last = NaiveDate::parse_from_str(newest, "%Y-%m-%d").unwrap();
        (0..nights)
            .map(|i| DailyMetrics {
                date: (last - chrono::Duration::days(i as i64))
                    .format("%Y-%m-%d")
                    .to_string(),
                training_readiness: Some(readiness),
                hrv_status: Some("BALANCED".into()),
                hrv_last_night: Some(80.0),
                hrv_weekly_avg: Some(79.0),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn a_quiet_week_with_nothing_wrong_says_nothing() {
        let goals = Goals::default();
        let acts = vec![run(
            "2026-08-05",
            35.0,
            [0.0, 2100.0, 0.0, 0.0, 0.0],
            Some(172.0),
        )];
        let week = week_progress(&goals, &acts, saturday());
        let out = evaluate(&ctx(saturday(), &goals, &week, &acts, &[]));
        assert!(out.is_empty(), "expected silence, got {out:#?}");
    }

    #[test]
    fn the_long_run_nudge_waits_until_the_week_is_running_out() {
        let goals = Goals::default();
        let acts = vec![run(
            "2026-08-04",
            10.0,
            [0.0, 600.0, 0.0, 0.0, 0.0],
            Some(170.0),
        )];
        let week = week_progress(&goals, &acts, saturday());

        // Tuesday: too early to nag about a week that has barely started.
        let tuesday = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        assert!(!fired(
            &evaluate(&ctx(tuesday, &goals, &week, &acts, &[])),
            NudgeKind::LongRunMissing
        ));
        // Saturday: still nothing long, and the week is nearly gone.
        assert!(fired(
            &evaluate(&ctx(saturday(), &goals, &week, &acts, &[])),
            NudgeKind::LongRunMissing
        ));
    }

    #[test]
    fn a_long_run_that_happened_silences_the_rule() {
        let goals = Goals::default();
        let acts = vec![run(
            "2026-08-05",
            32.0,
            [0.0, 1920.0, 0.0, 0.0, 0.0],
            Some(170.0),
        )];
        let week = week_progress(&goals, &acts, saturday());
        assert!(!fired(
            &evaluate(&ctx(saturday(), &goals, &week, &acts, &[])),
            NudgeKind::LongRunMissing
        ));
    }

    #[test]
    fn drift_needs_a_rise_across_three_runs_not_one_hard_day() {
        let goals = Goals::default();
        // Newest first: 20% → 25% → 30% is a real climb.
        let climbing = vec![
            run("2026-08-07", 10.0, [0.0, 420.0, 180.0, 0.0, 0.0], None),
            run("2026-08-05", 10.0, [0.0, 480.0, 120.0, 0.0, 0.0], None),
            run("2026-08-03", 10.0, [0.0, 570.0, 30.0, 0.0, 0.0], None),
        ];
        let week = week_progress(&goals, &climbing, saturday());
        assert!(fired(
            &evaluate(&ctx(saturday(), &goals, &week, &climbing, &[])),
            NudgeKind::HardDrift
        ));

        // One hard session among steady ones is not a drift.
        let spike = vec![
            run("2026-08-07", 10.0, [0.0, 540.0, 60.0, 0.0, 0.0], None),
            run("2026-08-05", 10.0, [0.0, 300.0, 300.0, 0.0, 0.0], None),
            run("2026-08-03", 10.0, [0.0, 540.0, 60.0, 0.0, 0.0], None),
        ];
        let week = week_progress(&goals, &spike, saturday());
        assert!(!fired(
            &evaluate(&ctx(saturday(), &goals, &week, &spike, &[])),
            NudgeKind::HardDrift
        ));
    }

    #[test]
    fn runs_without_hr_cannot_produce_a_drift_verdict() {
        let goals = Goals::default();
        let acts = vec![
            run("2026-08-07", 10.0, [0.0; 5], None),
            run("2026-08-05", 10.0, [0.0; 5], None),
            run("2026-08-03", 10.0, [0.0; 5], None),
        ];
        let week = week_progress(&goals, &acts, saturday());
        assert!(!fired(
            &evaluate(&ctx(saturday(), &goals, &week, &acts, &[])),
            NudgeKind::HardDrift
        ));
    }

    #[test]
    fn the_green_light_needs_recovery_and_a_quiet_stretch() {
        let goals = Goals::default();
        let acts = vec![run(
            "2026-08-01",
            10.0,
            [0.0, 120.0, 480.0, 0.0, 0.0],
            Some(170.0),
        )];
        let week = week_progress(&goals, &acts, saturday());
        let recovered = worn_nights("2026-08-08", 20, 82.0);
        assert!(fired(
            &evaluate(&ctx(saturday(), &goals, &week, &acts, &recovered)),
            NudgeKind::GreenLight
        ));

        // Same week, poor readiness: no green light.
        let tired = worn_nights("2026-08-08", 20, 31.0);
        assert!(!fired(
            &evaluate(&ctx(saturday(), &goals, &week, &acts, &tired)),
            NudgeKind::GreenLight
        ));
    }

    /// "Balanced" is a verdict against a personal range, and the range is built
    /// from nights of wear. Garmin wants about four a week; below that the
    /// status still reads Balanced against a baseline drawn from too little.
    /// The reading is present, fresh, and says the right word — which is why
    /// nothing else in this file catches it.
    #[test]
    fn a_balanced_status_on_too_few_nights_is_not_a_green_light() {
        let goals = Goals::default();
        let acts = vec![run(
            "2026-08-01",
            10.0,
            [0.0, 120.0, 480.0, 0.0, 0.0],
            Some(170.0),
        )];
        let week = week_progress(&goals, &acts, saturday());

        // Three nights across the four-week window: roughly one a week.
        let sparse = [
            worn_nights("2026-08-08", 1, 82.0),
            worn_nights("2026-07-30", 1, 82.0),
            worn_nights("2026-07-21", 1, 82.0),
        ]
        .concat();
        assert!(
            !fired(
                &evaluate(&ctx(saturday(), &goals, &week, &acts, &sparse)),
                NudgeKind::GreenLight
            ),
            "a baseline built on one night a week is not clearance to go hard"
        );
    }

    /// The bug this guard exists for: a watch that stopped being worn leaves
    /// its last good reading at the top of the table, where every rule that
    /// speaks in the present tense reads it as this morning's. The reading is
    /// real and correctly dated — nothing is fabricated — which is exactly why
    /// it went unnoticed, and why "go hard today" came out of a recovery score
    /// from two months ago.
    #[test]
    fn a_recovery_reading_from_two_months_ago_cannot_give_a_green_light() {
        let goals = Goals::default();
        let acts = vec![run(
            "2026-06-01",
            10.0,
            [0.0, 120.0, 480.0, 0.0, 0.0],
            Some(170.0),
        )];
        let week = week_progress(&goals, &acts, saturday());
        // Excellent numbers, and every one of them from June.
        let june = vec![DailyMetrics {
            date: "2026-06-08".into(),
            training_readiness: Some(88.0),
            hrv_status: Some("BALANCED".into()),
            hrv_last_night: Some(84.0),
            hrv_weekly_avg: Some(79.0),
            ..Default::default()
        }];
        assert!(
            !fired(
                &evaluate(&ctx(saturday(), &goals, &week, &acts, &june)),
                NudgeKind::GreenLight
            ),
            "a two-month-old readiness score must not be read as this morning's"
        );
    }

    /// The other half: the guard has to be slack enough for a watch that spent
    /// last night on a charger, or it would silence the rule for everyone.
    #[test]
    fn yesterdays_reading_still_counts_as_current() {
        let goals = Goals::default();
        let acts = vec![run(
            "2026-08-01",
            10.0,
            [0.0, 120.0, 480.0, 0.0, 0.0],
            Some(170.0),
        )];
        let week = week_progress(&goals, &acts, saturday());
        let yesterday = worn_nights("2026-08-07", 20, 82.0);
        assert!(fired(
            &evaluate(&ctx(saturday(), &goals, &week, &acts, &yesterday)),
            NudgeKind::GreenLight
        ));
    }

    #[test]
    fn a_hard_session_yesterday_withdraws_the_green_light() {
        let goals = Goals::default();
        let acts = vec![run(
            "2026-08-07",
            10.0,
            [0.0, 120.0, 480.0, 0.0, 0.0],
            Some(170.0),
        )];
        let week = week_progress(&goals, &acts, saturday());
        let recovered = vec![DailyMetrics {
            date: "2026-08-08".into(),
            training_readiness: Some(82.0),
            hrv_status: Some("BALANCED".into()),
            ..Default::default()
        }];
        assert!(!fired(
            &evaluate(&ctx(saturday(), &goals, &week, &acts, &recovered)),
            NudgeKind::GreenLight
        ));
    }

    #[test]
    fn garmins_anaerobic_verdict_fires_with_its_own_numbers() {
        let goals = Goals::default();
        let acts = vec![run(
            "2026-08-05",
            32.0,
            [0.0, 1920.0, 0.0, 0.0, 0.0],
            Some(170.0),
        )];
        let week = week_progress(&goals, &acts, saturday());
        let fitness = FitnessDay {
            date: "2026-08-08".into(),
            status: crate::TrainingStatus {
                anaerobic: Some(473.0),
                anaerobic_target_min: Some(133.0),
                anaerobic_target_max: Some(400.0),
                balance_phrase: Some("ANAEROBIC_FOCUS".into()),
                ..Default::default()
            },
            predictions: Default::default(),
        };
        let mut c = ctx(saturday(), &goals, &week, &acts, &[]);
        c.fitness = Some(&fitness);

        let out = evaluate(&c);
        let n = out
            .iter()
            .find(|n| n.kind == NudgeKind::AnaerobicOverTarget)
            .expect("should fire");
        assert!(n.evidence.iter().any(|e| e.contains("473")));
        assert!(n.evidence.iter().any(|e| e.contains("ANAEROBIC_FOCUS")));
    }

    #[test]
    fn a_load_spike_outranks_everything_else() {
        let goals = Goals::default();
        let acts = vec![run(
            "2026-08-04",
            10.0,
            [0.0, 600.0, 0.0, 0.0, 0.0],
            Some(140.0),
        )];
        let week = week_progress(&goals, &acts, saturday());
        let fitness = FitnessDay {
            date: "2026-08-08".into(),
            status: crate::TrainingStatus {
                acwr: Some(1.8),
                acwr_status: Some("HIGH".into()),
                acute_load: Some(600.0),
                chronic_load: Some(330.0),
                ..Default::default()
            },
            predictions: Default::default(),
        };
        let mut c = ctx(saturday(), &goals, &week, &acts, &[]);
        c.fitness = Some(&fitness);

        let out = evaluate(&c);
        assert_eq!(out.first().map(|n| n.kind), Some(NudgeKind::LoadSpike));
    }

    #[test]
    fn every_nudge_carries_evidence() {
        let goals = Goals::default();
        let acts = vec![
            run(
                "2026-08-07",
                10.0,
                [0.0, 420.0, 180.0, 0.0, 0.0],
                Some(140.0),
            ),
            run(
                "2026-08-05",
                10.0,
                [0.0, 480.0, 120.0, 0.0, 0.0],
                Some(138.0),
            ),
            run(
                "2026-08-03",
                10.0,
                [0.0, 570.0, 30.0, 0.0, 0.0],
                Some(142.0),
            ),
        ];
        let week = week_progress(&goals, &acts, saturday());
        let out = evaluate(&ctx(saturday(), &goals, &week, &acts, &[]));
        assert!(!out.is_empty());
        for n in &out {
            assert!(!n.evidence.is_empty(), "{} had no evidence", n.id);
            assert!(!n.title.is_empty());
            assert!(!n.body.is_empty());
        }
    }
}
