//! From the typed `garmin-api` Today view to the formatted view model that the
//! screen draws.
//!
//! The API returns plain numbers. The wording of a figure is a UI job, so the
//! formatting from `app/src/lib/format.ts` lives here.

use chrono::{Datelike, Duration, NaiveDate};
use garmin_api::TodayView;
use garmin_core::CachedActivity;

use crate::data::{Attention, Drift, DriftRun, Link, Metric, Part, Session, Stat, Today, Week};

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

fn metrics(view: &TodayView) -> Vec<Metric> {
    let latest = |l: &Option<garmin_api::Latest>| plain(l.as_ref().map(|l| l.value));
    let resting_label = view
        .resting_hr
        .as_ref()
        .map_or("Resting HR".to_string(), |r| {
            format!(
                "Resting HR · {}{:.0} vs 30d",
                if r.delta > 0.0 { "+" } else { "" },
                r.delta
            )
        });
    let fuel_label = match view.fuel.age {
        Some(1) => "Fuel balance · yesterday".to_string(),
        Some(n) if n > 1 => format!("Fuel balance · {n}d ago"),
        _ => "Fuel balance".to_string(),
    };
    let fuel_value = match view.fuel.balance {
        Some(b) => {
            let sign = if b.round() > 0.0 { "+" } else { "" };
            vec![text(format!("{sign}{}", num(b))), unit(" kcal")]
        }
        None => vec![text(DASH)],
    };
    vec![
        Metric {
            label: "Sleep".into(),
            value: view
                .sleep
                .as_ref()
                .map_or(vec![text(DASH)], |s| sleep_value(s.value)),
            accent: false,
        },
        Metric {
            label: "Body battery".into(),
            value: latest(&view.body_battery),
            accent: false,
        },
        Metric {
            label: "Readiness".into(),
            value: latest(&view.readiness),
            accent: false,
        },
        Metric {
            label: "HRV".into(),
            value: view
                .hrv
                .as_ref()
                .map_or(vec![text(DASH)], |h| vec![text(num(h.value)), unit(" ms")]),
            accent: false,
        },
        Metric {
            label: resting_label,
            value: plain(view.resting_hr.as_ref().map(|r| r.value)),
            accent: view.resting_hr.as_ref().is_some_and(|r| r.accent),
        },
        Metric {
            label: fuel_label,
            value: fuel_value,
            accent: view.fuel.accent,
        },
    ]
}

fn weight_note(view: &TodayView) -> Option<String> {
    let w = view.weight.as_ref().filter(|w| w.count > 0)?;
    let trend = w
        .trend_kg
        .map_or(DASH.to_string(), |t| format!("{t:.1} kg"));
    Some(match w.rate_kg_per_week {
        Some(rate) => format!(
            "Weight trend {trend}, {} {:.2} kg a week.",
            if rate < 0.0 { "down" } else { "up" },
            rate.abs()
        ),
        None => format!("Weight trend {trend}."),
    })
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
                .flatten()
                .map(|v| *v as f32)
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

fn sport(a: &CachedActivity) -> String {
    let raw = a
        .type_key
        .as_deref()
        .unwrap_or("activity")
        .replace('_', " ");
    let mut chars = raw.chars();
    chars
        .next()
        .map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

fn session(a: &CachedActivity, has_zones: bool, above_z2: f64) -> Session {
    let secs = a.moving_duration_s.or(a.duration_s);
    let dist = a.distance_m.filter(|d| *d > 0.0);
    let is_run = a.type_key.as_deref().is_some_and(|k| k.contains("running"));
    let pace_or_speed = match (dist, secs) {
        (Some(d), Some(s)) if is_run => ("Pace", format!("{} /km", clock(s / (d / 1000.0)))),
        (Some(d), Some(s)) => ("Speed", format!("{:.1} km/h", d / 1000.0 / (s / 3600.0))),
        _ => (if is_run { "Pace" } else { "Speed" }, DASH.to_string()),
    };
    let hr = |v: Option<f64>| v.map_or(DASH.to_string(), |v| format!("{v:.0}"));
    let stat = |label: &str, value: String| Stat {
        label: label.into(),
        value,
    };
    let mut detail = sport(a);
    if let Some(d) = a.start_time_local.as_deref().or(a.local_date.as_deref()) {
        detail.push_str(&format!(" · {}", short_date(d)));
    }
    if has_zones {
        detail.push_str(&format!(" · {above_z2:.0}% above Z2"));
    }
    Session {
        name: a
            .name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| sport(a)),
        detail,
        zones: a.zone_percentages().map(|p| p as f32),
        stats: vec![
            stat(
                "Distance",
                dist.map_or(DASH.to_string(), |d| format!("{:.1} km", d / 1000.0)),
            ),
            stat("Time", secs.map_or(DASH.to_string(), clock)),
            stat(pace_or_speed.0, pace_or_speed.1),
            stat("Avg HR", hr(a.avg_hr)),
            stat("Max HR", hr(a.max_hr)),
            stat(
                "Cadence",
                a.avg_cadence
                    .map_or(DASH.to_string(), |c| format!("{c:.0} spm")),
            ),
            stat(
                "Aerobic TE",
                a.aerobic_te.map_or(DASH.to_string(), |t| format!("{t:.1}")),
            ),
        ],
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
        metrics: metrics(view),
        fuel_note: match &view.hydration_sentence {
            Some(h) => format!("{} {h}", view.fuel_sentence),
            None => view.fuel_sentence.clone(),
        },
        weight_note: weight_note(view),
        attention: attention(view),
        last_session: view
            .last_session
            .as_ref()
            .map(|l| session(&l.activity, l.has_zone_data, l.above_z2_pct)),
        zone_drift: drift(view),
        week: Week {
            km: view.week.daily_km.iter().map(|k| *k as f32).collect(),
            axis: week_axis(day.unwrap_or_else(|| chrono::Local::now().date_naive())),
            summary: view.week.sentence.clone(),
        },
    }
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
        assert_eq!(mapped.metrics.len(), 6);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }
}
