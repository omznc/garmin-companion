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

/// Where the day's brief is kept, alongside the fingerprint of the data it was
/// written from and the date it was last opened.
const BRIEF_KEY: &str = "daily_brief";
const BRIEF_FINGERPRINT_KEY: &str = "daily_brief_key";
const BRIEF_READ_KEY: &str = "daily_brief_read";

/// The brief's identity in the `nudges` table.
///
/// One id for all time rather than one per day. Everything that table is asked
/// about the brief — has today's notification gone out, was it put away this
/// morning — is a question about *today*, answered by comparing a stored date
/// against today's. A stable key answers it in one row instead of accumulating
/// one per day forever.
pub const BRIEF_ID: &str = "daily-brief";

/// How many days ahead the plan reaches.
///
/// The app cannot run in the background, so every notification has to be handed
/// to the system in advance and its text frozen at the moment it was queued.
/// That text is now a paragraph written about one particular day, and it ages
/// faster than a rule's did: "you went hard yesterday on six hours of sleep" is
/// wrong by Thursday in a way that "the long run hasn't happened yet" is not.
/// Two covers a day of not opening the app — tonight, and tomorrow night saying
/// out loud how old it is — and then the phone goes quiet, which is the right
/// behaviour for an app that has been abandoned.
const HORIZON_DAYS: u32 = 2;

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

/* ------------------------------------------------------------ daily brief --- */

/// Who wrote the brief.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BriefSource {
    /// A model, handed the day's data and whatever the rules noticed in it.
    Model,
    /// The rules alone — no model is configured, or the call failed.
    Rules,
}

/// What the coach has to say today, written once and read in two places.
///
/// The notification and the block on the Today screen are the same text by
/// construction rather than by discipline: `alert` is what the system shows,
/// `body` is what the screen shows, and both come out of one decision. Tapping
/// the one cannot land on a screen saying something else, because there is
/// nothing else for it to say.
///
/// The rules in [`evaluate`] still run and still decide what is *true*; what
/// they no longer decide is which of it is worth saying, or in what words. They
/// arrive here as `signals`, evidence handed to a writer, and the writer is
/// allowed to conclude that today is not worth interrupting for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyBrief {
    /// The local date this was written about. A brief is about a day, and it
    /// stops being about today at midnight.
    pub date: String,
    /// Whether this is worth interrupting for. False is the common answer and
    /// not a failure: an app that notifies every day is one whose notifications
    /// get switched off in a fortnight. A brief with `notify: false` still
    /// shows on Today — it just doesn't knock.
    pub notify: bool,
    /// One line, written to fit a notification title.
    pub title: String,
    /// One sentence, written to fit a notification body.
    pub alert: String,
    /// The whole thing, which is what tapping the notification opens.
    pub body: String,
    pub tone: Tone,
    /// The numbers behind it, each already formatted for display. Same contract
    /// as [`Nudge::evidence`]: nothing is asserted that this can't show.
    pub evidence: Vec<String>,
    /// The rule ids that fired today, whether or not the brief spoke about
    /// them. What the writer was looking at, kept so the screen can show it —
    /// a brief that quietly ignored a real signal should be visible as that.
    pub signals: Vec<String>,
    pub source: BriefSource,
    /// RFC3339, when it was written.
    pub generated_at: String,
    /// True when it was put away today. Resolved on load from the `nudges`
    /// table rather than stored in the blob, so dismissing doesn't rewrite it.
    #[serde(default)]
    pub dismissed: bool,
    /// True once the block has actually been opened.
    ///
    /// With `notify`, this is what lets a tap that launched the app from cold
    /// still land on the block. That tap arrives before there is any JavaScript
    /// listening for it and the plugin does not replay it, so the screen asks
    /// the question from the other end instead: today's brief judged itself
    /// worth interrupting for and has not been opened since, so open it.
    ///
    /// Deliberately not "a notification actually fired", which is the fact this
    /// would ideally turn on and the one neither platform can supply. Android
    /// hands the whole plan to the system days ahead and is never told which
    /// ones went off; desktop shows it and knows, but desktop is not where a
    /// tap arrives at a process that isn't running. What both platforms do
    /// agree on is whether the brief asked to knock at all — and on a day it
    /// didn't there is nothing to have been tapped.
    #[serde(default)]
    pub read: bool,
}

impl DailyBrief {
    /// The brief the cache is holding for today, with this morning's dismissal
    /// and read state applied.
    ///
    /// `None` when none has been written, when the stored one is about a day
    /// that has ended, or when it no longer parses — all three mean the same
    /// thing to every caller, which is that one needs writing.
    pub fn load(db: &crate::Db, today: NaiveDate) -> anyhow::Result<Option<Self>> {
        let today = today.format("%Y-%m-%d").to_string();
        let Some(raw) = db.sync_state(BRIEF_KEY)?.filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let Ok(mut brief) = serde_json::from_str::<Self>(&raw) else {
            return Ok(None);
        };
        if brief.date != today {
            return Ok(None);
        }

        brief.dismissed = db
            .nudge_state(BRIEF_ID)?
            .and_then(|s| s.dismissed_on)
            .is_some_and(|d| d == today);
        brief.read = db.sync_state(BRIEF_READ_KEY)?.is_some_and(|d| d == today);
        Ok(Some(brief))
    }

    /// Store this brief and the fingerprint of the data behind it.
    pub fn save(&self, db: &crate::Db, fingerprint: &str) -> anyhow::Result<()> {
        db.set_sync_state(BRIEF_KEY, &serde_json::to_string(self)?)?;
        db.set_sync_state(BRIEF_FINGERPRINT_KEY, fingerprint)?;
        // The `nudges` row has to exist before anything can claim or dismiss
        // the brief: both are `UPDATE`s, and an update that matches no row is
        // indistinguishable from one that found nothing left to do. Without
        // this, the desktop's once-a-day claim reads as "already notified" on
        // every single call and the nudge never arrives.
        db.saw_nudge(BRIEF_ID, &self.date)?;
        Ok(())
    }

    /// The fingerprint the stored brief was written from, for the caller
    /// deciding whether to write another.
    pub fn stored_fingerprint(db: &crate::Db) -> anyhow::Result<Option<String>> {
        db.sync_state(BRIEF_FINGERPRINT_KEY)
    }

    /// Record that the block has been opened, so it stops presenting itself as
    /// something not yet read.
    pub fn mark_read(db: &crate::Db, today: NaiveDate) -> anyhow::Result<()> {
        db.set_sync_state(BRIEF_READ_KEY, &today.format("%Y-%m-%d").to_string())
    }
}

/// The brief the rules can write on their own.
///
/// This is what the coach used to be, in full: the highest-priority rule's own
/// copy, straight through. It stays because the two things that can stop a
/// model writing — none configured, and one that couldn't be reached — are both
/// ordinary, and an evening with no coach at all is worse than an evening with
/// the plainer one. The rules still write honest, evidenced sentences. What
/// they can't do is look at a day and decide it isn't worth one.
pub fn rules_brief(report: &CoachReport, generated_at: String) -> DailyBrief {
    let headline = report.headline();
    DailyBrief {
        date: report.date.clone(),
        // Exactly the old behaviour: a notification on the days a rule fired,
        // silence on the days none did. `headline` has already dropped
        // anything dismissed this morning.
        notify: headline.is_some(),
        title: headline.map_or_else(String::new, |n| n.nudge.title.clone()),
        alert: headline.map_or_else(String::new, |n| first_sentence(&n.nudge.body)),
        body: headline.map_or_else(String::new, |n| n.nudge.body.clone()),
        tone: headline.map_or(Tone::Neutral, |n| n.nudge.tone),
        evidence: headline.map_or_else(Vec::new, |n| n.nudge.evidence.clone()),
        signals: report.nudges.iter().map(|n| n.nudge.id.clone()).collect(),
        source: BriefSource::Rules,
        generated_at,
        dismissed: false,
        read: false,
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

/// Up to the first full stop. What a rule's two sentences of body reduce to
/// when they have to fit under a notification title.
fn first_sentence(text: &str) -> String {
    text.split_once(". ")
        .map_or(text, |(first, _)| first)
        .trim()
        .to_string()
}

/// The notification body: the brief's one sentence, and after the first day an
/// admission of how old it is.
///
/// Everything scheduled beyond the first is text frozen at planning time, and
/// the honest framing is that it is exactly as stale as the app itself — no
/// sync has happened either, so opening the app right then would say the same
/// thing. Saying when it was worked out lets that be checked rather than
/// assumed.
fn notification_body(brief: &DailyBrief, day: u32) -> String {
    if day == 0 {
        return brief.alert.clone();
    }
    match NaiveDate::parse_from_str(&brief.date, "%Y-%m-%d") {
        Ok(written_on) => format!(
            "{} (worked out {}, and nothing has synced since.)",
            brief.alert,
            written_on.format("%A")
        ),
        // A brief with an unparseable date is a bug elsewhere, and dropping the
        // parenthetical is a better answer than dropping the notification.
        Err(_) => brief.alert.clone(),
    }
}

/// What to hand the system, given today's brief.
///
/// Pure, so the awkward parts — an hour that has already passed, a brief that
/// chose to stay quiet, one already put away this morning, notifications
/// switched off — are decided here and testable, leaving the platform layer
/// with nothing but the scheduling calls.
///
/// Note what is *not* decided here any more: whether today is worth a
/// notification at all. That is `brief.notify`, and it was settled when the
/// brief was written, by whoever wrote it.
pub fn plan_notifications(
    brief: &DailyBrief,
    settings: &NotifySettings,
    now: NaiveDateTime,
) -> Vec<PlannedNudge> {
    if !settings.enabled || !brief.notify || brief.dismissed {
        return Vec::new();
    }
    // A brief that wants to notify but has nothing to put in the notification
    // is a malformed one. Silence beats an empty banner.
    if brief.title.trim().is_empty() || brief.alert.trim().is_empty() {
        return Vec::new();
    }
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
            nudge_id: BRIEF_ID.to_string(),
            title: brief.title.clone(),
            body: notification_body(brief, day),
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

    /// A brief that wants to speak, of the shape a model returns.
    fn brief() -> DailyBrief {
        DailyBrief {
            date: "2026-08-08".into(),
            notify: true,
            title: "The long easy run hasn't happened yet".into(),
            alert: "Longest run this week is 11 minutes against a 30 minute goal".into(),
            body: "Longest run this week is 11 minutes against a 30 minute goal. \
                   There are two days left to put one in, and yesterday's readiness \
                   of 81 says you have the room for it."
                .into(),
            tone: Tone::Watch,
            evidence: vec!["Longest run: 11 min".into()],
            signals: vec!["long-run-missing".into()],
            source: BriefSource::Model,
            generated_at: "2026-08-08T07:00:00Z".into(),
            dismissed: false,
            read: false,
        }
    }

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
            &brief(),
            &NotifySettings::default(),
            at("2026-08-08 09:00:00"),
        );
        assert_eq!(plan.len(), HORIZON_DAYS as usize);
        assert_eq!(plan[0].at, at("2026-08-08 18:00:00"));
        assert_eq!(
            plan[HORIZON_DAYS as usize - 1].at,
            at("2026-08-09 18:00:00")
        );
    }

    #[test]
    fn the_plan_starts_tomorrow_once_the_hour_has_gone() {
        let plan = plan_notifications(
            &brief(),
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
            &brief(),
            &NotifySettings::default(),
            at("2026-08-08 18:00:00"),
        );
        assert_eq!(plan[0].at, at("2026-08-09 18:00:00"));
    }

    /// Only the first is current. The rest have to say so, because by then no
    /// sync has happened and the brief in them is about a day that has ended.
    #[test]
    fn only_the_first_notification_speaks_as_of_today() {
        let plan = plan_notifications(
            &brief(),
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

    /// The whole point of the rewrite: the writer decides, and it is allowed to
    /// decide that today is not worth a notification even though it still has a
    /// block's worth to say on the screen.
    #[test]
    fn a_brief_that_chose_to_stay_quiet_schedules_nothing() {
        let plan = plan_notifications(
            &DailyBrief {
                notify: false,
                ..brief()
            },
            &NotifySettings::default(),
            at("2026-08-08 09:00:00"),
        );
        assert!(plan.is_empty());
    }

    #[test]
    fn a_dismissed_brief_is_not_scheduled() {
        let plan = plan_notifications(
            &DailyBrief {
                dismissed: true,
                ..brief()
            },
            &NotifySettings::default(),
            at("2026-08-08 09:00:00"),
        );
        assert!(plan.is_empty());
    }

    /// A model that returns `notify: true` and then nothing to put in the
    /// banner shouldn't produce an empty one.
    #[test]
    fn a_brief_with_no_words_in_it_schedules_nothing() {
        for malformed in [
            DailyBrief {
                alert: "  ".into(),
                ..brief()
            },
            DailyBrief {
                title: String::new(),
                ..brief()
            },
        ] {
            let plan = plan_notifications(
                &malformed,
                &NotifySettings::default(),
                at("2026-08-08 09:00:00"),
            );
            assert!(plan.is_empty());
        }
    }

    #[test]
    fn switched_off_schedules_nothing() {
        let plan = plan_notifications(
            &brief(),
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
            &brief(),
            &NotifySettings {
                enabled: true,
                hour: 99,
            },
            at("2026-08-08 09:00:00"),
        );
        assert_eq!(plan[0].at, at("2026-08-08 23:00:00"));
    }

    /* ----------------------------------------------------- the fallback --- */

    /// With no model, the coach has to behave exactly as it did before: a
    /// notification on the days a rule fired, carrying that rule's own words.
    #[test]
    fn the_rules_fallback_reproduces_the_old_behaviour() {
        let fallback = rules_brief(
            &report(vec![("long-run-missing", false)]),
            "2026-08-08T07:00:00Z".into(),
        );
        assert!(fallback.notify);
        assert_eq!(fallback.source, BriefSource::Rules);
        assert_eq!(fallback.title, "The long easy run hasn't happened yet");
        assert_eq!(
            fallback.alert,
            "Longest run this week is 11 minutes against a 30 minute goal"
        );
        assert_eq!(fallback.evidence, vec!["Longest run: 11 min".to_string()]);

        let plan = plan_notifications(
            &fallback,
            &NotifySettings::default(),
            at("2026-08-08 09:00:00"),
        );
        assert_eq!(plan.len(), HORIZON_DAYS as usize);
        assert_eq!(plan[0].nudge_id, BRIEF_ID);
    }

    #[test]
    fn the_rules_fallback_stays_quiet_when_no_rule_fired() {
        let fallback = rules_brief(&report(vec![]), "2026-08-08T07:00:00Z".into());
        assert!(!fallback.notify);
        assert!(plan_notifications(
            &fallback,
            &NotifySettings::default(),
            at("2026-08-08 09:00:00")
        )
        .is_empty());
    }

    /// `headline` already drops what was put away this morning, so the fallback
    /// inherits dismissal without having to know about it.
    #[test]
    fn the_rules_fallback_stays_quiet_when_the_only_rule_was_dismissed() {
        let fallback = rules_brief(
            &report(vec![("long-run-missing", true)]),
            "2026-08-08T07:00:00Z".into(),
        );
        assert!(!fallback.notify);
        // The rule still fired, and the brief still says so.
        assert_eq!(fallback.signals, vec!["long-run-missing".to_string()]);
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
