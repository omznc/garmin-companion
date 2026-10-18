//! What the user does, read from the cache.
//!
//! The profile counts exercise sessions by sport family over the last 56 days.
//! A screen uses it to decide which sport sections to draw. No family is a
//! default. A user with no sessions gets health sections only.

use crate::derive::days_since;
use chrono::NaiveDate;
use garmin_core::sport::family;
use garmin_core::{CachedActivity, Family};
use serde::Serialize;
use std::collections::BTreeMap;

/// The window that the profile counts, in days.
const WINDOW_DAYS: i64 = 56;
/// One session this recent is enough to make a family active.
const RECENT_DAYS: i64 = 14;
/// Below this many daily rows, the account is new.
pub const NEW_USER_DAYS: usize = 14;
/// A family needs more than this share of all minutes to be the primary one.
const PRIMARY_SHARE: f64 = 0.4;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyStat {
    pub family: Family,
    pub sessions: usize,
    pub minutes: f64,
    /// Date of the newest session in the window, `YYYY-MM-DD`.
    pub last_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityProfile {
    pub window_days: i64,
    /// Exercise families with at least one session in the window, most minutes first.
    pub families: Vec<FamilyStat>,
    /// Families with 2 or more sessions, or 1 session in the last 14 days.
    pub active: Vec<Family>,
    /// The active family with more than 40% of all minutes. `None` otherwise.
    /// A screen never needs it to draw. Only coach copy may use it.
    pub primary: Option<Family>,
    /// True when the window holds 1 or more exercise sessions.
    pub tracks_activity: bool,
    /// Daily rows that the cache holds in the Today window.
    pub data_days: usize,
    /// True when `data_days` is below 14.
    pub new_user: bool,
}

/// Build the profile. `exercise` must not hold non-exercise sessions.
pub fn activity_profile(
    exercise: &[CachedActivity],
    data_days: usize,
    today: NaiveDate,
) -> ActivityProfile {
    let mut by_family: BTreeMap<Family, FamilyStat> = BTreeMap::new();
    let mut recent: BTreeMap<Family, usize> = BTreeMap::new();
    for a in exercise {
        let Some(age) = days_since(a, today).filter(|d| (0..WINDOW_DAYS).contains(d)) else {
            continue;
        };
        let f = family(a.type_key.as_deref());
        let stat = by_family.entry(f).or_insert_with(|| FamilyStat {
            family: f,
            sessions: 0,
            minutes: 0.0,
            last_date: None,
        });
        stat.sessions += 1;
        stat.minutes += a.duration_s.unwrap_or(0.0) / 60.0;
        let date = a.local_date.clone().or_else(|| {
            a.start_time_local
                .as_ref()
                .and_then(|s| s.get(..10).map(String::from))
        });
        if stat.last_date < date {
            stat.last_date = date;
        }
        if age < RECENT_DAYS {
            *recent.entry(f).or_default() += 1;
        }
    }

    let mut families: Vec<FamilyStat> = by_family.into_values().collect();
    families.sort_by(|a, b| b.minutes.total_cmp(&a.minutes));
    let active: Vec<Family> = families
        .iter()
        .filter(|s| s.sessions >= 2 || recent.contains_key(&s.family))
        .map(|s| s.family)
        .collect();
    let total: f64 = families.iter().map(|s| s.minutes).sum();
    let primary = families
        .iter()
        .find(|s| active.contains(&s.family))
        .filter(|top| total > 0.0 && top.minutes / total > PRIMARY_SHARE)
        .map(|top| top.family);

    ActivityProfile {
        window_days: WINDOW_DAYS,
        tracks_activity: !families.is_empty(),
        families,
        active,
        primary,
        data_days,
        new_user: data_days < NEW_USER_DAYS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(date: &str, key: &str, minutes: f64) -> CachedActivity {
        CachedActivity {
            activity_id: 1,
            name: None,
            type_key: Some(key.into()),
            start_time_local: None,
            local_date: Some(date.into()),
            distance_m: None,
            duration_s: Some(minutes * 60.0),
            moving_duration_s: None,
            avg_hr: None,
            max_hr: None,
            avg_cadence: None,
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: [0.0; 5],
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()
    }

    #[test]
    fn counts_families_and_picks_a_primary() {
        let acts = [
            act("2026-03-10", "strength_training", 60.0),
            act("2026-03-03", "strength_training", 60.0),
            act("2026-03-09", "golf", 30.0),
            act("2025-12-01", "running", 90.0),
        ];
        let p = activity_profile(&acts, 40, today());
        assert_eq!(p.families.len(), 2);
        assert_eq!(p.families[0].family, Family::Gym);
        assert_eq!(p.families[0].last_date.as_deref(), Some("2026-03-10"));
        assert_eq!(p.active, [Family::Gym, Family::Golf]);
        assert_eq!(p.primary, Some(Family::Gym));
        assert!(p.tracks_activity && !p.new_user);
    }

    #[test]
    fn an_old_single_session_is_not_active() {
        let p = activity_profile(&[act("2026-01-20", "golf", 120.0)], 40, today());
        assert_eq!(p.families.len(), 1);
        assert!(p.active.is_empty() && p.primary.is_none());
    }

    #[test]
    fn primary_comes_from_the_active_families() {
        let acts = [
            act("2026-01-20", "hiking", 300.0),
            act("2026-03-10", "strength_training", 125.0),
            act("2026-03-05", "strength_training", 125.0),
        ];
        let p = activity_profile(&acts, 40, today());
        assert_eq!(p.primary, Some(Family::Gym));
    }

    #[test]
    fn a_spread_week_has_no_primary() {
        let acts = [
            act("2026-03-10", "golf", 50.0),
            act("2026-03-09", "running", 50.0),
            act("2026-03-08", "yoga", 50.0),
        ];
        assert!(activity_profile(&acts, 40, today()).primary.is_none());
    }

    #[test]
    fn no_sessions_means_no_activity() {
        let p = activity_profile(&[], 3, today());
        assert!(!p.tracks_activity && p.new_user && p.families.is_empty());
    }
}
