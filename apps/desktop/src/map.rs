//! From the typed `garmin-api` Today view to the formatted view model that the
//! screen draws.
//!
//! The API returns plain numbers. The wording of a figure is a UI job, so the
//! formatting from `app/src/lib/format.ts` lives here.

use chrono::{Datelike, Duration, NaiveDate};
use garmin_api::TodayView;
use garmin_core::analysis::Discipline;
use garmin_core::CachedActivity;

use crate::data::{
    Attention, Drift, DriftRun, Fuel, Link, Metric, Part, Session, Stat, Today, Week,
};

const DASH: &str = "—";

fn text(value: impl Into<String>) -> Part {
    Part {
        text: value.into(),
        unit: false,
    }
}

fn unit(value: impl Into<String>) -> Part {
    Part {
        text: value.into(),
        unit: true,
    }
}

/// `1,980`, with a comma between thousands.
fn num(value: f64) -> String {
    let digits = format!("{:.0}", value.abs());
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if value < -0.5 {
        format!("-{out}")
    } else {
        out
    }
}

fn sleep_value(secs: f64) -> Vec<Part> {
    let total_minutes = (secs / 60.0).round() as i64;
    vec![
        text((total_minutes / 60).to_string()),
        unit("h"),
        text(format!(" {:02}", total_minutes % 60)),
        unit("m"),
    ]
}

fn plain(value: Option<f64>) -> Vec<Part> {
    vec![text(value.map_or(DASH.to_string(), num))]
}

/// The health tiles that have a figure, in the order of the old screen.
fn metrics(view: &TodayView) -> Vec<Metric> {
    let latest = |l: &Option<garmin_api::Latest>| plain(l.as_ref().map(|l| l.value));
    let resting_label = view.resting_hr.as_ref().map(|r| {
        format!(
            "Resting HR · {}{:.0} vs 30d",
            if r.delta > 0.0 { "+" } else { "" },
            r.delta
        )
    });
    let tile = |label: &str, value: Vec<Part>, accent: bool| Metric {
        label: label.to_string(),
        value,
        accent,
    };
    let plain_tile = |label: &str, l: &Option<garmin_api::Latest>| {
        l.as_ref().map(|_| tile(label, latest(l), false))
    };
    // A tile with no figure is left out, so a user without the sensor sees no dash.
    [
        view.sleep
            .as_ref()
            .map(|s| tile("Sleep", sleep_value(s.value), false)),
        plain_tile("Body battery", &view.body_battery),
        plain_tile("Readiness", &view.readiness),
        view.hrv
            .as_ref()
            .map(|h| tile("HRV", vec![text(num(h.value)), unit(" ms")], false)),
        plain_tile("Steps", &view.steps),
        plain_tile("Stress", &view.stress),
        view.resting_hr
            .as_ref()
            .zip(resting_label)
            .map(|(r, label)| tile(&label, plain(Some(r.value)), r.accent)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The fuel balance as a tile, and the sentence about it. `None` when nothing
/// was logged, so the section hides.
fn fuel(view: &TodayView) -> Option<Fuel> {
    if view.fuel.logged == 0 && view.hydration.is_none() {
        return None;
    }
    let label = match view.fuel.age {
        Some(1) => "Fuel balance · yesterday".to_string(),
        Some(n) if n > 1 => format!("Fuel balance · {n}d ago"),
        _ => "Fuel balance".to_string(),
    };
    let tile = view.fuel.balance.map(|b| {
        let sign = if b.round() > 0.0 { "+" } else { "" };
        Metric {
            label,
            value: vec![text(format!("{sign}{}", num(b))), unit(" kcal")],
            accent: view.fuel.accent,
        }
    });
    let note = match &view.hydration_sentence {
        Some(h) => format!("{} {h}", view.fuel_sentence),
        None => view.fuel_sentence.clone(),
    };
    Some(Fuel { tile, note })
}

fn attention(view: &TodayView) -> Vec<Attention> {
    view.attention
        .iter()
        .map(|o| Attention {
            text: o.text.clone(),
            accent: o.accent,
            spark: o
                .spark
                .iter()
                .flatten()
                .map(|day| day.map(|v| v as f32))
                .collect(),
            link: o.link.as_ref().map(|l| Link {
                label: l.label.clone(),
            }),
        })
        .collect()
}

fn parse_day(raw: &str) -> Option<NaiveDate> {
    raw.get(..10)
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
}

fn short_date(raw: &str) -> String {
    parse_day(raw).map_or(DASH.to_string(), |d| d.format("%-d %b").to_string())
}

fn clock(secs: f64) -> String {
    let s = secs.round() as i64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// The figures of a session. A figure with no value is left out.
fn stats(a: &CachedActivity, discipline: Discipline) -> Vec<Stat> {
    let secs = a.moving_duration_s.or(a.duration_s).filter(|s| *s > 0.0);
    // Under 50 m there is no pace worth showing.
    let dist = a.distance_m.filter(|d| *d >= 50.0);
    let rate = match (dist, secs, discipline) {
        // Over 60 min/km it is a walk with the watch left on.
        (Some(d), Some(s), Discipline::Paced) if s / (d / 1000.0) <= 3600.0 => {
            Some(("Pace", format!("{} /km", clock(s / (d / 1000.0)))))
        }
        (Some(d), Some(s), Discipline::Endurance) => {
            Some(("Speed", format!("{:.1} km/h", d / 1000.0 / (s / 3600.0))))
        }
        _ => None,
    };
    let positive = |v: Option<f64>| v.filter(|v| *v > 0.0);
    [
        dist.map(|d| ("Distance", format!("{:.1} km", d / 1000.0))),
        secs.map(|s| ("Time", clock(s))),
        rate,
        positive(a.avg_hr).map(|v| ("Avg HR", format!("{v:.0}"))),
        positive(a.max_hr).map(|v| ("Max HR", format!("{v:.0}"))),
        positive(a.avg_cadence).map(|c| ("Cadence", format!("{c:.0} spm"))),
        positive(a.aerobic_te).map(|t| ("Aerobic TE", format!("{t:.1}"))),
    ]
    .into_iter()
    .flatten()
    .map(|(label, value)| Stat {
        label: label.into(),
        value,
    })
    .collect()
}

fn session(l: &garmin_api::today::LastSession) -> Session {
    let a = &l.activity;
    let zones = a.zone_percentages().map(|p| p as f32);
    let has_zones = l.has_zone_data && zones.iter().sum::<f32>() > 0.0;
    // A verdict about zones holds for endurance sport only, so the zone bar
    // and the sentence follow `show_zone_verdict`.
    let mut detail = l.sport_label.clone();
    if let Some(d) = a.start_time_local.as_deref().or(a.local_date.as_deref()) {
        detail.push_str(&format!(" · {}", short_date(d)));
    }
    if l.show_zone_verdict && has_zones {
        detail.push_str(&format!(" · {:.0}% above Z2", l.above_z2_pct));
    }
    Session {
        name: a
            .name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| l.sport_label.clone()),
        detail,
        zones: (l.show_zone_verdict && has_zones).then_some(zones),
        zone_note: (l.show_zone_verdict && !has_zones)
            .then(|| "No heart-rate data recorded".to_string()),
        stats: stats(a, l.discipline),
    }
}

fn drift(view: &TodayView) -> Option<Drift> {
    let d = &view.zone_drift;
    if d.runs.len() < 2 {
        return None;
    }
    let summary = d.recent_avg_pct.map_or(String::new(), |avg| {
        format!(
            "{avg:.0}% above Z2 lately · {:.0}% is the target",
            d.target_pct
        )
    });
    Some(Drift {
        noun: d.noun.clone(),
        target: d.target_pct as f32,
        summary,
        runs: d
            .runs
            .iter()
            .map(|r| DriftRun {
                date: r.date.as_deref().map_or(DASH.to_string(), short_date),
                above_z2: r.above_z2_pct as f32,
            })
            .collect(),
    })
}

/// The weekday of each of the seven days, ending on `today`.
fn week_axis(today: NaiveDate) -> Vec<String> {
    (0..7)
        .map(|i| (today - Duration::days(6 - i)).format("%a").to_string())
        .collect()
}

pub fn today(view: &TodayView, who: Option<String>) -> Today {
    let day = parse_day(&view.date);
    let weekday = day.map_or(String::new(), |d| d.format("%A").to_string());
    let eyebrow = day.map_or(String::new(), |d| {
        format!("{}, {} {}", weekday, d.day(), d.format("%B"))
    });
    Today {
        who,
        synced_ago: None,
        eyebrow,
        title: format!("{weekday}."),
        lede: view.lede.clone(),
        empty: view.empty,
        sections: view.sections.clone(),
        metrics: metrics(view),
        fuel: fuel(view),
        weight_note: view.weight.as_ref().map(|w| w.sentence.clone()),
        attention: attention(view),
        last_session: view.last_session.as_ref().map(session),
        zone_drift: drift(view),
        week: week(view, day),
    }
}

/// The week in minutes, which every sport has. Distance is a caption, and only
/// a user with distance gets it.
fn week(view: &TodayView, day: Option<NaiveDate>) -> Option<Week> {
    let w = &view.week;
    // The API says whether the week is worth a section. A user with no sessions has zeros.
    if !view.sections.iter().any(|k| k == "week") {
        return None;
    }
    Some(Week {
        minutes: w.daily_minutes.iter().map(|m| *m as f32).collect(),
        axis: week_axis(day.unwrap_or_else(|| chrono::Local::now().date_naive())),
        summary: w.sentence.clone(),
        distance: (w.total_km > 0.0).then(|| format!("{:.1} km", w.total_km)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_get_thousands_separators_and_a_sign() {
        assert_eq!(num(1980.0), "1,980");
        assert_eq!(num(-612.4), "-612");
        assert_eq!(num(1234567.0), "1,234,567");
        assert_eq!(num(0.0), "0");
    }

    #[test]
    fn sleep_splits_into_hours_and_minutes_with_units() {
        let parts = sleep_value(25_320.0);
        let joined: String = parts.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(joined, "7h 02m");
        assert!(parts[1].unit && parts[3].unit);
    }

    #[test]
    fn clock_uses_hours_only_when_needed() {
        assert_eq!(clock(3130.0), "52:10");
        assert_eq!(clock(3725.0), "1:02:05");
    }

    #[test]
    fn the_week_axis_ends_on_today() {
        let sat = NaiveDate::from_ymd_opt(2026, 8, 8).unwrap();
        let axis = week_axis(sat);
        assert_eq!(axis.first().map(String::as_str), Some("Sun"));
        assert_eq!(axis.last().map(String::as_str), Some("Sat"));
    }

    #[test]
    fn an_empty_cache_maps_without_failing() {
        let dir = std::env::temp_dir().join(format!("gc-desktop-map-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = garmin_api::open_db(&dir.join("cache.sqlite3")).unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 8, 8).unwrap();
        let view = garmin_api::today_view(&db, day).unwrap();
        let mapped = today(&view, None);
        assert_eq!(mapped.title, "Saturday.");
        assert!(mapped.empty && mapped.metrics.is_empty() && mapped.week.is_none());
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }
}
