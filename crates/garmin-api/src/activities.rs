//! The Activities list and the Activity detail screen as view models.
//!
//! The list groups sessions by day and gives each one a main figure that suits
//! its sport family. The detail view holds the stats, the zone bar (only for a
//! sport where a zone verdict is true), the laps, the strength sets and a thin
//! HR and elevation series. The core builds the text. A UI only draws it.
//!
//! Laps and the series come from the cached analysis row. A session that was
//! never analysed has neither, and the view leaves those blocks out.
//!
//! Left out on purpose: the map and the per-activity chat.

use crate::derive::{above_z2_pct, has_zone_data};
use crate::text::num;
use crate::ApiError;
use chrono::NaiveDate;
use garmin_core::analysis::{self, is_paced, Lap};
use garmin_core::sport::{family, is_wheeled, shows_zone_verdict, sport_label};
use garmin_core::strength::{self, ExerciseSet};
use garmin_core::{CachedActivity, Db, Family};
use serde::{Deserialize, Serialize};

/// The most points in the chart series. A phone draws no more than this.
const CHART_POINTS: usize = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitiesView {
    /// True when the cache holds no session.
    pub empty: bool,
    /// Newest day first.
    pub groups: Vec<DayGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayGroup {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// "Today", "Yesterday", or a date such as "Wed 11 Mar".
    pub label: String,
    pub sessions: Vec<ActivityCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCard {
    pub activity_id: i64,
    pub name: String,
    pub sport_label: String,
    pub family: Family,
    /// Start time, `HH:MM`.
    pub time: Option<String>,
    /// The main figure for the sport family, such as "6.4 km" or "14 sets".
    pub headline: String,
    /// The other figures, such as "35 min - 148 bpm avg".
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneRow {
    /// 1 to 5.
    pub zone: u8,
    pub percent: f64,
    /// Time in the zone, such as "12 min".
    pub time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneBlock {
    pub rows: Vec<ZoneRow>,
    pub sentence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LapRow {
    pub label: String,
    pub distance: Option<String>,
    pub time: Option<String>,
    pub pace: Option<String>,
    pub avg_hr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRow {
    /// "Set 1".
    pub label: String,
    pub reps: Option<String>,
    pub work: Option<String>,
    /// The rest after the set. The last set has none.
    pub rest: Option<String>,
    /// The watch guess, such as "Bench press (guess)". Often empty.
    pub exercise: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrengthBlock {
    pub sentence: String,
    pub sets: Vec<SetRow>,
}

/// Evenly spaced samples. A `None` is a gap in the line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub hr: Vec<Option<f64>>,
    /// Empty when the session has no elevation.
    pub elevation: Vec<Option<f64>>,
    /// The length of the session, for the axis, such as "42 min".
    pub span: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDetailView {
    pub activity_id: i64,
    pub name: String,
    pub sport_label: String,
    pub family: Family,
    /// For example "Wed 11 Mar, 07:15".
    pub when: String,
    pub stats: Vec<Stat>,
    pub zones: Option<ZoneBlock>,
    pub laps: Vec<LapRow>,
    pub strength: Option<StrengthBlock>,
    pub chart: Option<Chart>,
}

/* ------------------------------------------------------------------- text --- */

/// `35 min`, or `1h 05m` from one hour.
fn duration_text(secs: f64) -> String {
    let total = (secs / 60.0).round() as i64;
    if total < 60 {
        format!("{total} min")
    } else {
        format!("{}h {:02}m", total / 60, total % 60)
    }
}

/// `5:32` from minutes per kilometre.
fn pace_text(min_per_km: f64) -> String {
    let secs = (min_per_km * 60.0).round() as i64;
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn km_text(metres: f64) -> String {
    format!("{:.1} km", metres / 1000.0)
}

fn day_of(a: &CachedActivity) -> Option<NaiveDate> {
    let text = a.local_date.as_deref().or(a.start_time_local.as_deref())?;
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

fn time_of(a: &CachedActivity) -> Option<String> {
    let text = a.start_time_local.as_deref()?;
    // `YYYY-MM-DD HH:MM:SS` or `YYYY-MM-DDTHH:MM:SS`.
    text.get(11..16).map(str::to_owned)
}

fn day_label(day: NaiveDate, today: NaiveDate) -> String {
    match (today - day).num_days() {
        0 => "Today".to_owned(),
        1 => "Yesterday".to_owned(),
        _ => day.format("%a %d %b").to_string(),
    }
}

fn session_name(a: &CachedActivity) -> String {
    a.name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| sport_label(a.type_key.as_deref()))
}

fn work_sets(sets: &[ExerciseSet]) -> (usize, i64) {
    let work = sets.iter().filter(|s| s.active);
    (
        work.clone().count(),
        work.filter_map(|s| s.reps).sum::<i64>(),
    )
}

/// "Bench press" from `BENCH_PRESS`.
fn exercise_name(raw: &str) -> String {
    let text = raw.replace('_', " ").to_lowercase();
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/* ------------------------------------------------------------------- list --- */

/// The main figure and the rest, as `(headline, detail)`.
fn card_text(a: &CachedActivity, sets: &[ExerciseSet]) -> (String, String) {
    let duration = a.duration_s.filter(|d| *d > 0.0).map(duration_text);
    let hr = a.avg_hr.map(|h| format!("{} bpm avg", num(h)));
    let distance = a.distance_m.filter(|d| *d >= 100.0).map(km_text);
    let (n_sets, reps) = work_sets(sets);
    let (headline, extras): (Option<String>, Vec<Option<String>>) =
        match family(a.type_key.as_deref()) {
            Family::Gym if n_sets > 0 => (
                Some(format!("{n_sets} sets")),
                vec![Some(format!("{reps} reps")), duration, hr],
            ),
            Family::Endurance | Family::NonExercise | Family::Water if distance.is_some() => {
                (distance, vec![duration, hr])
            }
            _ => (duration, vec![hr]),
        };
    let detail = extras.into_iter().flatten().collect::<Vec<_>>().join(" - ");
    (headline.unwrap_or_else(|| "Logged".to_owned()), detail)
}

fn card(a: &CachedActivity, sets: &[ExerciseSet]) -> ActivityCard {
    let (headline, detail) = card_text(a, sets);
    ActivityCard {
        activity_id: a.activity_id,
        name: session_name(a),
        sport_label: sport_label(a.type_key.as_deref()),
        family: family(a.type_key.as_deref()),
        time: time_of(a),
        headline,
        detail,
    }
}

/// Group sessions by day. The input is newest first.
pub fn group_by_day(
    cards: Vec<(Option<NaiveDate>, ActivityCard)>,
    today: NaiveDate,
) -> Vec<DayGroup> {
    let mut groups: Vec<DayGroup> = Vec::new();
    for (day, card) in cards {
        let key = day.map_or_else(String::new, |d| d.format("%Y-%m-%d").to_string());
        match groups.last_mut() {
            Some(g) if g.date == key => g.sessions.push(card),
            _ => groups.push(DayGroup {
                label: day.map_or_else(|| "Earlier".to_owned(), |d| day_label(d, today)),
                date: key,
                sessions: vec![card],
            }),
        }
    }
    groups
}

/// The list of recent sessions, grouped by day, newest first.
pub fn activities_view(db: &Db, today: NaiveDate, limit: u32) -> Result<ActivitiesView, ApiError> {
    let rows = db.recent_activities(limit, None)?;
    let mut cards = Vec::with_capacity(rows.len());
    for a in &rows {
        // Only a gym card needs the sets.
        let sets = if family(a.type_key.as_deref()) == Family::Gym {
            db.exercise_sets(a.activity_id)?
        } else {
            Vec::new()
        };
        cards.push((day_of(a), card(a, &sets)));
    }
    Ok(ActivitiesView {
        empty: rows.is_empty(),
        groups: group_by_day(cards, today),
    })
}

/* ----------------------------------------------------------------- detail --- */

fn stat(label: &str, value: Option<String>) -> Option<Stat> {
    value.map(|value| Stat {
        label: label.to_owned(),
        value,
    })
}

/// How a session shows its rate: pace and spm on foot, km/h and rpm on a bike.
/// Other sports show neither, because no unit is certain.
fn rate_stat(a: &CachedActivity) -> Option<Stat> {
    let key = a.type_key.as_deref();
    let label = if is_wheeled(key) { "Speed" } else { "Pace" };
    stat(label, rate_text(key, a.pace_min_per_km()?))
}

/// `5:30 /km` on foot, `24.5 km/h` on wheels, nothing for other sports.
fn rate_text(type_key: Option<&str>, min_per_km: f64) -> Option<String> {
    if is_paced(type_key) {
        Some(format!("{} /km", pace_text(min_per_km)))
    } else if is_wheeled(type_key) && min_per_km > 0.0 {
        Some(format!("{:.1} km/h", 60.0 / min_per_km))
    } else {
        None
    }
}

fn cadence_unit(type_key: Option<&str>) -> Option<&'static str> {
    if is_paced(type_key) {
        Some("spm")
    } else if is_wheeled(type_key) {
        Some("rpm")
    } else {
        None
    }
}

fn stats(a: &CachedActivity) -> Vec<Stat> {
    let secs = a.moving_duration_s.or(a.duration_s).filter(|s| *s > 0.0);
    let cadence = cadence_unit(a.type_key.as_deref())
        .and_then(|unit| a.avg_cadence.map(|c| format!("{} {unit}", num(c))));
    [
        stat(
            "Distance",
            a.distance_m.filter(|d| *d >= 100.0).map(km_text),
        ),
        stat("Time", secs.map(duration_text)),
        rate_stat(a),
        stat("Avg HR", a.avg_hr.map(|h| format!("{} bpm", num(h)))),
        stat("Max HR", a.max_hr.map(|h| format!("{} bpm", num(h)))),
        stat(
            "Elevation",
            a.elevation_gain
                .filter(|e| *e > 0.0)
                .map(|e| format!("{} m", num(e))),
        ),
        stat("Cadence", cadence),
        stat("Steps", a.steps.map(|s| num(s as f64))),
        stat("Calories", a.calories.map(|c| format!("{} kcal", num(c)))),
        stat("Aerobic TE", a.aerobic_te.map(|t| format!("{t:.1}"))),
        stat("Anaerobic TE", a.anaerobic_te.map(|t| format!("{t:.1}"))),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn zone_block(a: &CachedActivity) -> Option<ZoneBlock> {
    if !shows_zone_verdict(a.type_key.as_deref()) || !has_zone_data(a) {
        return None;
    }
    let pct = a.zone_percentages();
    let rows = (0..5)
        .map(|i| ZoneRow {
            zone: i as u8 + 1,
            percent: pct[i],
            time: duration_text(a.zone_secs[i]),
        })
        .collect();
    let above = above_z2_pct(a);
    let sentence = if above < 1.0 {
        "The whole session stayed in Z1 and Z2.".to_owned()
    } else {
        format!("{}% of the session was above Z2.", num(above))
    };
    Some(ZoneBlock { rows, sentence })
}

fn lap_row(l: &Lap, type_key: Option<&str>) -> LapRow {
    LapRow {
        label: format!("Lap {}", l.index),
        distance: l.distance_m.filter(|d| *d >= 10.0).map(km_text),
        time: l.duration_s.map(duration_text_exact),
        pace: l.pace_min_km.and_then(|p| rate_text(type_key, p)),
        avg_hr: l.avg_hr.map(num),
    }
}

/// `5:32` for a lap. A lap is short, so seconds matter.
fn duration_text_exact(secs: f64) -> String {
    let total = secs.round() as i64;
    if total >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            total % 3600 / 60,
            total % 60
        )
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

fn thin(values: &[Option<f64>]) -> Vec<Option<f64>> {
    if values.iter().all(Option::is_none) {
        return Vec::new();
    }
    if values.len() <= CHART_POINTS {
        return values.to_vec();
    }
    (0..CHART_POINTS)
        .map(|i| values[i * (values.len() - 1) / (CHART_POINTS - 1)])
        .collect()
}

fn chart(analysis: &analysis::ActivityAnalysis) -> Option<Chart> {
    let hr = thin(&analysis.series.hr);
    if hr.is_empty() {
        return None;
    }
    let span = analysis
        .series
        .elapsed_s
        .iter()
        .rev()
        .find_map(|v| *v)
        .map(duration_text)
        .unwrap_or_default();
    Some(Chart {
        hr,
        elevation: thin(&analysis.series.elevation_m),
        span,
    })
}

fn strength_block(sets: &[ExerciseSet]) -> Option<StrengthBlock> {
    let summary = strength::summarise(sets);
    if summary.work_sets == 0 {
        return None;
    }
    let mut sentence = format!(
        "{} sets and {} reps, {} of work.",
        summary.work_sets,
        summary.total_reps,
        duration_text(summary.work_s)
    );
    if let Some(rest) = summary.median_rest_s {
        sentence.push_str(&format!(" Median rest was {}.", duration_text_exact(rest)));
    }
    let mut rows: Vec<SetRow> = Vec::new();
    for s in sets {
        if s.active {
            rows.push(SetRow {
                label: format!("Set {}", rows.len() + 1),
                reps: s.reps.map(|r| r.to_string()),
                work: s.duration_s.map(duration_text_exact),
                rest: None,
                exercise: s
                    .exercise
                    .as_deref()
                    .map(|e| format!("{} (guess)", exercise_name(e))),
            });
        } else if let Some(last) = rows.last_mut() {
            last.rest = s.duration_s.map(duration_text_exact);
        }
    }
    Some(StrengthBlock {
        sentence,
        sets: rows,
    })
}

/// The cached analysis of a session, or `None` when it was never computed or
/// is out of date.
fn cached_analysis(
    db: &Db,
    a: &CachedActivity,
) -> Result<Option<analysis::ActivityAnalysis>, ApiError> {
    let tags = db.activity_tags(a.activity_id)?;
    let key = analysis::fingerprint(a, &tags);
    // A row from an older build that no longer decodes counts as missing.
    Ok(db
        .activity_analysis(a.activity_id, &key)?
        .and_then(|json| analysis::decode(&json).ok()))
}

/// The detail view of one session. `None` when the id is not in the cache.
pub fn activity_detail_view(
    db: &Db,
    activity_id: i64,
) -> Result<Option<ActivityDetailView>, ApiError> {
    let Some(a) = db.activity(activity_id)? else {
        return Ok(None);
    };
    let fam = family(a.type_key.as_deref());
    let endurance = fam == Family::Endurance;
    let analysis = cached_analysis(db, &a)?;
    let sets = if fam == Family::Gym {
        db.exercise_sets(activity_id)?
    } else {
        Vec::new()
    };
    let when = match (day_of(&a), time_of(&a)) {
        (Some(d), Some(t)) => format!("{}, {t}", d.format("%a %d %b")),
        (Some(d), None) => d.format("%a %d %b").to_string(),
        _ => String::new(),
    };
    Ok(Some(ActivityDetailView {
        activity_id,
        name: session_name(&a),
        sport_label: sport_label(a.type_key.as_deref()),
        family: fam,
        when,
        stats: stats(&a),
        zones: zone_block(&a),
        laps: analysis
            .as_ref()
            .filter(|_| endurance)
            .map(|x| {
                let key = a.type_key.as_deref();
                x.laps.iter().map(|l| lap_row(l, key)).collect()
            })
            .unwrap_or_default(),
        strength: strength_block(&sets),
        chart: analysis.as_ref().and_then(chart),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn durations_and_paces_read_plainly() {
        assert_eq!(duration_text(2100.0), "35 min");
        assert_eq!(duration_text(3900.0), "1h 05m");
        assert_eq!(pace_text(5.5), "5:30");
        assert_eq!(duration_text_exact(332.0), "5:32");
        assert_eq!(exercise_name("BENCH_PRESS"), "Bench press");
    }

    fn session(key: &str) -> CachedActivity {
        CachedActivity {
            activity_id: 1,
            name: None,
            type_key: Some(key.into()),
            start_time_local: None,
            local_date: Some("2026-03-10".into()),
            distance_m: Some(30_000.0),
            duration_s: Some(3_600.0),
            moving_duration_s: None,
            avg_hr: None,
            max_hr: None,
            avg_cadence: Some(85.0),
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: [0.0; 5],
        }
    }

    fn stat_of(a: &CachedActivity, label: &str) -> Option<String> {
        stats(a)
            .into_iter()
            .find(|s| s.label == label)
            .map(|s| s.value)
    }

    #[test]
    fn a_ride_shows_speed_and_rpm_and_a_run_shows_pace_and_spm() {
        let ride = session("cycling");
        assert_eq!(stat_of(&ride, "Speed").as_deref(), Some("30.0 km/h"));
        assert!(stat_of(&ride, "Pace").is_none());
        let run = session("running");
        assert_eq!(stat_of(&run, "Pace").as_deref(), Some("2:00 /km"));
        assert_eq!(stat_of(&run, "Cadence").as_deref(), Some("85 spm"));
        assert_eq!(stat_of(&ride, "Cadence").as_deref(), Some("85 rpm"));
        assert!(stat_of(&session("motorcycling_v2"), "Pace").is_none());
        assert!(stat_of(&session("indoor_rowing"), "Cadence").is_none());
        assert!(stat_of(&run, "Speed").is_none());
        assert_eq!(cadence_unit(Some("mountain_biking")), Some("rpm"));
        assert_eq!(cadence_unit(Some("hiking")), Some("spm"));
        assert_eq!(cadence_unit(Some("indoor_rowing")), None);
        assert_eq!(cadence_unit(Some("motorcycling_v2")), None);
        assert_eq!(rate_text(Some("indoor_rowing"), 2.0), None);
    }

    #[test]
    fn days_are_labelled_against_today() {
        let today = d("2026-03-11");
        assert_eq!(day_label(today, today), "Today");
        assert_eq!(day_label(d("2026-03-10"), today), "Yesterday");
        assert_eq!(day_label(d("2026-03-06"), today), "Fri 06 Mar");
    }

    #[test]
    fn thin_keeps_the_ends_and_drops_an_empty_column() {
        let v: Vec<Option<f64>> = (0..500).map(|i| Some(f64::from(i))).collect();
        let t = thin(&v);
        assert_eq!(t.len(), CHART_POINTS);
        assert_eq!(t[0], Some(0.0));
        assert_eq!(t[CHART_POINTS - 1], Some(499.0));
        assert!(thin(&[None, None]).is_empty());
    }

    #[test]
    fn grouping_keeps_one_group_per_day() {
        let a = |id| ActivityCard {
            activity_id: id,
            name: String::new(),
            sport_label: String::new(),
            family: Family::Mixed,
            time: None,
            headline: String::new(),
            detail: String::new(),
        };
        let today = d("2026-03-11");
        let groups = group_by_day(
            vec![
                (Some(today), a(1)),
                (Some(today), a(2)),
                (Some(d("2026-03-09")), a(3)),
            ],
            today,
        );
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].sessions.len(), 2);
        assert_eq!(groups[1].label, "Mon 09 Mar");
    }
}
