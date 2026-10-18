//! What the model is told before the conversation starts.

use anyhow::Result;
use garmin_core::db::Db;
use serde_json::{json, Value};

use crate::turn::HistoryMessage;

/// How much of a conversation goes back to the model. The screen keeps the
/// whole transcript. The model gets the opening question and the newest tail,
/// because the whole array is sent again on every round.
const HISTORY_MESSAGES: usize = 12;
const HISTORY_CHARS: usize = 20_000;

/// The same bytes on every request. Anything that changes goes in
/// [`grounding_message`].
///
/// The audience is every Garmin user: golfers, gym-only, cyclists, walkers,
/// people who only track sleep. The prompt names no sport. Sport-specific
/// advice comes from what the data shows.
pub(crate) const SYSTEM_PROMPT: &str = "\
You are a health and training coach with read access to this person's Garmin \
history, held in a local cache on their device.

Answer from the tools, not from general knowledge. Call a tool before you make \
any claim about a number. If a tool returns nothing, say so. Never estimate a \
figure to fill a gap.

You have no clock. The next system message gives today's date and how current \
the cache is. Those are the only source for both. A date on a row tells you \
when that row happened, not how long ago it was. Before you call anything \
recent, check that the data reaches today. When it does not, say where it \
stops and answer about the period that you have.

Start from health: sleep, resting heart rate, HRV, stress, body battery, \
readiness and daily activity. Talk about a sport only when the data shows that \
sport. Do not assume that the person runs, and do not suggest that they should. \
This person may play golf, lift weights, cycle, walk, swim, or only wear the \
watch for sleep. If the cache has very little, say so plainly and say what \
would fill it. A new account is not a problem.

Not every recorded number is a measurement. Read these fields before you quote \
a figure that they apply to:

- `hr_confidence.level` is `good`, `caution` or `poor` for the heart-rate zone \
split of a session. `hr_confidence.notes` gives the reason. Do not drop a \
caveated session from a total. Say the caveat the first time that you use it.
- `pace_estimated: true` means distance and pace came from the arm \
accelerometer, not from GPS. They can be well wrong. Two estimated paces are \
two estimates.
- `resting_hr_source` is `overnight`, `daytimeEstimate` or `unverified`. Only \
`overnight` is Garmin's real resting heart rate. Do not put the other kinds on \
one trend line without saying so.
- An activity with `has_hr_data: false` recorded no heart rate. It is not an \
easy session. Leave it out of heart-rate totals.

Heart-rate zones suit steady aerobic work. They do not suit strength training, \
golf, circuits or other stop-and-go sessions. There the heart rate rises and \
falls with the sets, the rests or the walk between shots. Do not give a \
heart-rate target for those sessions.

A day with `logged: false` has no food log. It is not a day of eating nothing. \
Never average such a day in as zero.

Weigh-ins are irregular. Quote `trendKg`, not a single reading. Treat a point \
marked `outlier` as a mis-entry. Do not describe a direction when \
`rateKgPerWeek` is null.

Be direct and quantitative. Flag overreaching when the data shows it, without \
alarm, and say plainly what is going well. Prefer short paragraphs to bullet \
lists. You can ask the person a short question in your answer when the answer \
changes your advice and no tool can supply it.";

/// Today's date and what the cache can support. A separate system message, so
/// the fixed prompt above keeps a stable prefix.
pub(crate) fn grounding_message(db: &Db) -> Result<Value> {
    let now = chrono::Local::now();
    let status = crate::tools::cache_status(db)?;
    let goals = garmin_core::Goals::load(db)?;

    let mut s = format!(
        "Current date and time, which you cannot know any other way: {}, local \
         time {} (UTC{}). A question about \"today\", \"this week\" or \
         \"recently\" is anchored here, not to the newest row in the cache.\n\n",
        now.format("%A %-d %B %Y"),
        now.format("%H:%M"),
        now.format("%:z"),
    );
    s.push_str("How current the cached data is:\n");
    s.push_str(&age_line(
        "Wellness (resting HR, HRV, sleep, readiness)",
        status.newest_daily_date.as_deref(),
        status.days_since_daily,
    ));
    s.push_str(&age_line(
        "Activities",
        status.newest_activity_date.as_deref(),
        status.days_since_activity,
    ));
    if status.stale {
        s.push_str(
            "\nThe wellness data has stopped. The watch was probably not worn. \
             It is not a sync fault. Say that the data stops on that date, give \
             each figure with its date, and do not call the newest reading \
             \"this morning\". Their readiness now is unknown.\n",
        );
    }
    s.push_str(
        "\nEvery tool that takes a day count windows by date. An empty result \
         means nothing was recorded in that window. It does not mean the person \
         did nothing.\n",
    );

    let mut targets = Vec::new();
    if let Some(v) = goals.weekly_minutes {
        targets.push(format!("{v:.0} training minutes a week"));
    }
    if let Some(v) = goals.weekly_sessions {
        targets.push(format!("{v} sessions a week"));
    }
    if !targets.is_empty() {
        s.push_str(&format!(
            "\nTheir own standing goals, which the app set and not Garmin: {}. \
             Call `coach` for how the current week compares.\n",
            targets.join(", "),
        ));
    }
    Ok(json!({ "role": "system", "content": s }))
}

fn age_line(label: &str, date: Option<&str>, days: Option<i64>) -> String {
    match (date, days) {
        (Some(d), Some(0)) => format!("- {label}: through {d}, which is today.\n"),
        (Some(d), Some(1)) => format!("- {label}: through {d}, which is yesterday.\n"),
        (Some(d), Some(n)) => format!("- {label}: through {d}, {n} days ago.\n"),
        _ => format!("- {label}: nothing cached.\n"),
    }
}

/// The opening question and as much of the tail as fits. The newest message is
/// never dropped, however long it is.
pub(crate) fn trim_history(history: Vec<HistoryMessage>) -> Vec<HistoryMessage> {
    let total: usize = history.iter().map(|m| m.content.len()).sum();
    if history.len() <= HISTORY_MESSAGES && total <= HISTORY_CHARS {
        return history;
    }
    let mut start = history.len();
    let mut chars = 0usize;
    for (i, m) in history.iter().enumerate().rev() {
        chars += m.content.len();
        if history.len() - i > HISTORY_MESSAGES || chars > HISTORY_CHARS {
            break;
        }
        start = i;
    }
    start = start.min(history.len() - 1);
    // A tail that opens on an answer reads as a reply to nothing.
    while start < history.len() - 1 && history[start].role != "user" {
        start += 1;
    }
    let mut out = Vec::with_capacity(history.len() - start + 1);
    if start > 0 {
        out.push(history[0].clone());
    }
    out.extend(history[start..].iter().cloned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, n: usize) -> HistoryMessage {
        HistoryMessage {
            role: role.into(),
            content: "x".repeat(n),
        }
    }

    #[test]
    fn a_short_history_is_kept_whole() {
        let h = vec![msg("user", 5), msg("assistant", 5), msg("user", 5)];
        assert_eq!(trim_history(h).len(), 3);
    }

    #[test]
    fn a_long_history_keeps_the_first_and_the_newest_user_turn() {
        let mut h = Vec::new();
        for i in 0..20 {
            h.push(msg(if i % 2 == 0 { "user" } else { "assistant" }, 10));
        }
        h.push(msg("user", 10));
        let out = trim_history(h);
        assert!(out.len() <= HISTORY_MESSAGES + 1);
        assert_eq!(out[0].role, "user");
        assert_eq!(out.last().unwrap().role, "user");
    }

    #[test]
    fn the_newest_message_survives_any_length() {
        let h = vec![msg("user", 5), msg("assistant", 5), msg("user", 50_000)];
        let out = trim_history(h);
        assert_eq!(out.last().unwrap().content.len(), 50_000);
    }

    #[test]
    fn the_prompt_names_no_sport_as_the_default() {
        let lower = SYSTEM_PROMPT.to_lowercase();
        for word in [
            "running coach",
            "runner",
            "cadence",
            "pace_min",
            "per kilometre",
        ] {
            assert!(!lower.contains(word), "{word}");
        }
    }
}
