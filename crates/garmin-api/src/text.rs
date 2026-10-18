//! The plain-language sentences on the Today screen.
//!
//! Each sentence is built from clauses that appear only when the number behind
//! them exists. A partial day reads as a shorter sentence, not one full of
//! dashes. The sentences are the local fallback for the written opening.

use crate::derive::{days_since, easy_hard_split, is_run, latest, mean, Fuel, Hydration};
use chrono::NaiveDate;
use garmin_core::{CachedActivity, DailyMetrics};

/// A whole number with thousands separators, like `en-GB` formatting.
pub fn num(v: f64) -> String {
    let digits = (v.round() as i64).unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if v.round() < 0.0 {
        out.insert(0, '-');
    }
    out
}

/// `7h 05m`.
pub fn hours_minutes(secs: f64) -> String {
    let total = (secs / 60.0).round() as i64;
    format!("{}h {:02}m", total / 60, total % 60)
}

/// `06 Aug`, or `That day` if the date does not parse.
pub fn short_date(date: &str) -> Option<String> {
    let d = NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d").ok()?;
    Some(d.format("%d %b").to_string())
}

fn litres(ml: f64) -> String {
    format!("{:.2} L", ml / 1000.0)
}

/// Only call this with hydration that is tracked. `hydration()` returns `None`
/// otherwise, which keeps this clause off accounts where the column is zeros.
pub fn hydration_sentence(h: &Hydration) -> String {
    let today = h.latest.as_ref().filter(|l| l.age == 0);
    let head = match today {
        Some(l) => format!("You've logged {} of water today", litres(l.ml)),
        None => format!(
            "Water averaged {} over the {} of {} days you logged it",
            litres(h.avg_ml),
            h.logged,
            h.window
        ),
    };
    let Some(goal) = h.goal_ml else {
        return format!("{head}.");
    };
    let against = today.map_or(h.avg_ml, |l| l.ml);
    format!(
        "{head} — {:.0}% of your {} goal.",
        against / goal * 100.0,
        litres(goal)
    )
}

/// The fuel line. It always says something, because burn comes off the watch
/// whether or not anyone logged food.
pub fn fuel_sentence(f: &Fuel) -> String {
    let burn = f.avg_burn.map(|b| format!("{} kcal a day", num(b)));
    let (Some(day), Some(age)) = (&f.day, f.age) else {
        return match burn {
            Some(b) => format!("No food logged in the last {} days. You've burned an average of {b} over the same stretch — the intake side is what's missing, not the day.", f.window),
            None => format!("No food logged in the last {} days.", f.window),
        };
    };

    let when = match age {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        _ => short_date(&day.date).unwrap_or_else(|| "That day".into()),
    };
    let eaten = day.consumed_kcal.map_or(String::new(), num);
    let head = match day.calorie_balance() {
        None => format!("{when}: {eaten} kcal logged."),
        Some(b) => format!(
            "{when}: {eaten} kcal in against {} burned — a {} kcal {}.",
            day.total_burn_kcal.map_or(String::new(), num),
            num(b.abs().round()),
            if b < 0.0 { "deficit" } else { "surplus" }
        ),
    };

    // Averaging over unlogged days would invent a deficit, so the sentence
    // says what the average is over.
    match f.avg_balance {
        Some(avg) if f.logged >= 3 => {
            let sign = if avg.round() > 0.0 { "+" } else { "" };
            format!(
                "{head} Across the {} of {} days you logged, the average was {sign}{} kcal.",
                f.logged,
                f.window,
                num(avg)
            )
        }
        _ if f.logged == 1 && f.window > 1 => format!(
            "{head} It's the only day logged this week, so there's no average worth quoting."
        ),
        _ => head,
    }
}

/// This week's distance against the previous week.
pub fn load_sentence(now: f64, prior: f64) -> String {
    let change = (now - prior) / prior * 100.0;
    let verdict = if change.abs() < 10.0 {
        "steady"
    } else if change > 30.0 {
        "a big jump for one week"
    } else {
        "inside a sensible range"
    };
    format!(
        "Distance is {} {:.0}% on the previous week — {verdict}.",
        if change >= 0.0 { "up" } else { "down" },
        change.abs()
    )
}

/// The summary of the last seven days, next to the chart.
pub fn week_sentence(total_km: f64, prior_km: f64, sessions: usize) -> String {
    let noun = if sessions == 1 { "session" } else { "sessions" };
    let first = if total_km > 0.0 {
        format!("{total_km:.1} km across {sessions} {noun}.")
    } else {
        format!("{sessions} {noun}, none with recorded distance.")
    };
    let second = if prior_km > 0.0 && total_km > 0.0 {
        load_sentence(total_km, prior_km)
    } else {
        "No previous week to compare against yet.".to_string()
    };
    format!("{first} {second}")
}

fn sleep_clause(rows: &[DailyMetrics]) -> Option<String> {
    let sleep = latest(rows, |d| d.sleep_secs)?;
    let avg = mean(rows.iter().map(|d| d.sleep_secs));
    let vs = match avg {
        None => "",
        Some(a) if sleep.value > a * 1.05 => ", above your recent average",
        Some(a) if sleep.value < a * 0.95 => ", short of your recent average",
        Some(_) => ", about your usual",
    };
    Some(format!("You slept {}{vs}.", hours_minutes(sleep.value)))
}

fn readiness_clause(rows: &[DailyMetrics]) -> Option<String> {
    let readiness = latest(rows, |d| d.training_readiness)?;
    Some(match latest(rows, |d| d.body_battery_high) {
        Some(b) => format!(
            "Body battery peaked at {} and Garmin puts your readiness at {}.",
            num(b.value),
            num(readiness.value)
        ),
        None => format!("Garmin puts your readiness at {}.", num(readiness.value)),
    })
}

fn last_session_clause(last: &CachedActivity, today: NaiveDate) -> String {
    let ago = match days_since(last, today) {
        Some(0) => "Today's".to_string(),
        Some(1) => "Yesterday's".to_string(),
        Some(n) => format!("{n} days ago, your"),
        _ => "Your last".to_string(),
    };
    let dist = last
        .distance_m
        .filter(|m| *m > 0.0)
        .map_or(String::new(), |m| format!(" {:.1} km", m / 1000.0));
    let kind = last
        .type_key
        .as_deref()
        .unwrap_or("activity")
        .replace('_', " ");
    format!("{ago} session was{dist} {kind}.")
}

fn zones_clause(activities: &[CachedActivity]) -> Option<String> {
    let runs: Vec<_> = activities
        .iter()
        .filter(|a| is_run(&a.type_key))
        .take(5)
        .collect();
    let split = easy_hard_split(&runs)?;
    Some(if split.hard_pct > 40.0 {
        format!("Across your last {} runs with heart-rate data, {:.0}% of the time was above Z2 — still the thing most worth fixing.", split.counted, split.hard_pct)
    } else {
        format!("Across your last {} runs with heart-rate data, {:.0}% of the time was in Z1–Z2, which is where you want it.", split.counted, split.easy_pct)
    })
}

/// The opening paragraph, from clauses that exist only when their data does.
pub fn narrative(rows: &[DailyMetrics], activities: &[CachedActivity], today: NaiveDate) -> String {
    let parts: Vec<String> = [
        sleep_clause(rows),
        readiness_clause(rows),
        activities.first().map(|a| last_session_clause(a, today)),
        zones_clause(activities),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        "No metrics recorded for the last few days.".into()
    } else {
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::{fuel, HydrationLatest};

    #[test]
    fn num_groups_thousands() {
        assert_eq!(num(1234567.4), "1,234,567");
        assert_eq!(num(-2500.0), "-2,500");
        assert_eq!(num(999.0), "999");
    }

    #[test]
    fn load_sentence_picks_a_verdict() {
        assert!(load_sentence(51.0, 50.0).ends_with("steady."));
        assert!(load_sentence(80.0, 50.0).contains("up 60%"));
        assert!(load_sentence(80.0, 50.0).contains("big jump"));
        assert!(load_sentence(30.0, 50.0).contains("down 40%"));
        assert!(load_sentence(30.0, 50.0).contains("sensible"));
    }

    #[test]
    fn week_sentence_handles_missing_distance() {
        assert!(week_sentence(0.0, 10.0, 1).starts_with("1 session, none with"));
        assert!(week_sentence(12.0, 0.0, 3).ends_with("compare against yet."));
    }

    #[test]
    fn fuel_sentence_for_empty_log_still_reports_burn() {
        let d = DailyMetrics {
            date: "2026-03-01".into(),
            total_burn_kcal: Some(2650.0),
            ..DailyMetrics::default()
        };
        let s = fuel_sentence(&fuel(&[d], 7));
        assert!(s.contains("2,650 kcal a day"));
        assert!(s.starts_with("No food logged in the last 1 days."));
    }

    #[test]
    fn fuel_sentence_names_the_day_and_the_balance() {
        let d = DailyMetrics {
            date: "2026-03-01".into(),
            consumed_kcal: Some(1800.0),
            total_burn_kcal: Some(2600.0),
            ..DailyMetrics::default()
        };
        let rows = [d, DailyMetrics::default()];
        let s = fuel_sentence(&fuel(&rows, 7));
        assert!(
            s.starts_with("Yesterday: 1,800 kcal in against 2,600 burned — a 800 kcal deficit.")
        );
        assert!(s.ends_with("so there's no average worth quoting."));
    }

    #[test]
    fn hydration_sentence_uses_today_against_the_goal() {
        let h = Hydration {
            logged: 3,
            window: 7,
            avg_ml: 1000.0,
            latest: Some(HydrationLatest { ml: 1250.0, age: 0 }),
            goal_ml: Some(2500.0),
        };
        assert_eq!(
            hydration_sentence(&h),
            "You've logged 1.25 L of water today — 50% of your 2.50 L goal."
        );
        let h = Hydration {
            latest: None,
            goal_ml: None,
            ..h
        };
        assert!(hydration_sentence(&h).ends_with("you logged it."));
    }

    #[test]
    fn narrative_falls_back_when_empty() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 11).unwrap();
        assert_eq!(
            narrative(&[], &[], today),
            "No metrics recorded for the last few days."
        );
    }
}
