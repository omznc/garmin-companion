//! The blocks of the Today screen. Each one hides when its data is missing.

use crate::scale::px;
use gpui::{div, prelude::*, ColorExt as _, Div};

use super::{Block, BlockDef, Ctx, Go, Width};
use crate::charts::{self, AreaChart, Bar, MeterRow, Tone};
use crate::data::{Drift, Fuel, Session, Vital, Week};
use crate::icons::Icon;
use crate::kit::{self, Cell, ListRow};
use crate::screen::Screen;
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "health",
        title: "Health this morning",
        group: Screen::Today,
        default_width: Width::Full,
        render: health,
    },
    BlockDef {
        id: "last",
        title: "Last session",
        group: Screen::Today,
        default_width: Width::TwoThirds,
        render: last_session,
    },
    BlockDef {
        id: "attention",
        title: "Attention",
        group: Screen::Today,
        default_width: Width::Third,
        render: attention,
    },
    BlockDef {
        id: "week",
        title: "Last seven days",
        group: Screen::Today,
        default_width: Width::TwoThirds,
        render: week,
    },
    BlockDef {
        id: "drift",
        title: "Zone drift",
        group: Screen::Today,
        default_width: Width::Third,
        render: drift,
    },
    BlockDef {
        id: "night",
        title: "Last night",
        group: Screen::Today,
        default_width: Width::TwoThirds,
        render: night,
    },
    BlockDef {
        id: "fuel",
        title: "Fuel",
        group: Screen::Today,
        default_width: Width::Third,
        render: fuel,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("health", Width::Full),
    ("last", Width::TwoThirds),
    ("attention", Width::Third),
    ("week", Width::TwoThirds),
    ("drift", Width::Third),
    ("night", Width::TwoThirds),
    ("fuel", Width::Third),
];

/// The verdict of the headline, or the weekday.
pub fn title(ctx: &Ctx) -> Option<String> {
    Some(ctx.today.title.clone())
}

pub fn lede(ctx: &Ctx) -> Option<String> {
    Some(ctx.today.lede.clone())
}

/// Today keeps the date text of the screen.
pub fn crumb(_ctx: &Ctx) -> Option<String> {
    None
}

fn block(
    meta: Option<String>,
    link: Option<(&'static str, Go)>,
    body: impl IntoElement,
) -> Option<Block> {
    Some(Block {
        meta: meta.map(Into::into),
        link: link.map(|(label, go)| (label.into(), go)),
        body: body.into_any_element(),
    })
}

/// "1:14" for a long time, "52m" for a short one.
fn minutes_label(minutes: f32) -> String {
    let m = minutes.round() as i64;
    match m {
        ..=0 => String::new(),
        1..=59 => format!("{m}m"),
        _ => format!("{}:{:02}", m / 60, m % 60),
    }
}

/// "5:18" for under an hour, "1:02:10" for more.
fn clock(secs: f32) -> String {
    let s = secs.round() as i64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn padded(body: impl IntoElement) -> Div {
    div().px(px(12.0)).py(px(12.0)).child(body)
}

/// A figure, its unit and the words beside it, as at the head of a block.
fn head(value: &str, unit: &str, words: &str, t: &Theme) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap(px(6.0))
        .child(
            div()
                .font_family(MONO)
                .text_size(px(21.0))
                .text_color(t.fg)
                .child(value.to_string()),
        )
        .child(
            div()
                .font_family(MONO)
                .text_size(px(11.5))
                .text_color(t.mutd)
                .child(unit.to_string()),
        )
        .child(
            div()
                .font_family(SANS)
                .text_size(px(12.0))
                .text_color(t.faint)
                .child(words.to_string()),
        )
}

/* ----------------------------------------------------------------- health --- */

fn vital_series(ctx: &Ctx, vital: &Vital) -> Option<Vec<f32>> {
    let w = ctx.wellness?;
    let values: Vec<f32> = if vital.key == "sleep" {
        w.sleep
            .week
            .bars
            .iter()
            .filter_map(|b| b.hours.map(|h| h as f32))
            .collect()
    } else {
        w.health
            .metrics
            .iter()
            .find(|m| m.key == vital.key)?
            .points
            .iter()
            .flatten()
            .map(|v| *v as f32)
            .collect()
    };
    (values.len() >= 2).then_some(values)
}

fn health(ctx: &Ctx) -> Option<Block> {
    let vitals = &ctx.today.vitals;
    if vitals.is_empty() {
        return None;
    }
    let cells: Vec<Cell> = vitals
        .iter()
        .map(|v| Cell {
            label: v.label.clone().into(),
            value: v.value.clone().into(),
            unit: v.unit.clone().map(Into::into),
            sub: v.sub.clone().map(|s| {
                let tone = if v.accent { Tone::Warn } else { Tone::Muted };
                (s.into(), tone)
            }),
            spark: vital_series(ctx, v),
            big: true,
        })
        .collect();
    let days = ctx.wellness.map(|w| w.health.days).filter(|d| *d > 0);
    let columns = cells.len();
    block(
        days.map(|d| format!("{d}-day trend")),
        Some(("Health", Go::Screen(Screen::Health))),
        kit::stat_grid(cells, columns, ctx.t),
    )
}

/* ----------------------------------------------------------- last session --- */

fn zone_rows(secs: &[f32; 5], zones: &Option<[f32; 5]>) -> Vec<MeterRow> {
    let longest = secs.iter().copied().fold(0.0_f32, f32::max).max(1.0);
    let total = secs.iter().sum::<f32>().max(1.0);
    (0..5)
        .map(|i| MeterRow {
            name: format!("Z{}", i + 1).into(),
            fraction: secs[i] / longest,
            value: clock(secs[i]).into(),
            share: format!("{:.0}%", zones.map_or(secs[i] / total * 100.0, |z| z[i])).into(),
            tone: match i {
                0 => Tone::Muted,
                3 | 4 => Tone::Warn,
                _ => Tone::Accent,
            },
        })
        .collect()
}

fn zones_section(s: &Session, t: &Theme) -> Option<Div> {
    let secs = s.zone_secs.as_ref()?;
    let meta = s.max_hr.map(|hr| format!("max {hr:.0} bpm"));
    let title = div()
        .flex()
        .justify_between()
        .mb(px(8.0))
        .text_size(px(11.5))
        .text_color(t.faint)
        .child("Time in zones")
        .children(meta.map(|m| div().font_family(MONO).child(m)));
    Some(
        padded(
            div()
                .child(title)
                .child(charts::meter_rows(zone_rows(secs, &s.zones), t)),
        )
        .border_t_1()
        .border_color(t.line2),
    )
}

fn last_session(ctx: &Ctx) -> Option<Block> {
    let s = ctx.today.last_session.as_ref()?;
    let cells: Vec<Cell> = s
        .stats
        .iter()
        .map(|stat| Cell {
            label: stat.label.clone().into(),
            value: stat.value.clone().into(),
            unit: None,
            sub: None,
            spark: None,
            big: false,
        })
        .collect();
    let meta = match &s.when {
        Some(when) => format!("{} · {when}", s.sport),
        None => s.sport.clone(),
    };
    let note = s.zone_note.clone().filter(|_| s.zone_secs.is_none());
    let body = div()
        .child(kit::stat_grid(cells, 4, ctx.t))
        .children(zones_section(s, ctx.t))
        .children(note.map(|n| {
            padded(n)
                .text_size(px(12.0))
                .text_color(ctx.t.faint)
                .border_t_1()
                .border_color(ctx.t.line2)
        }));
    block(
        Some(meta),
        Some(("Full breakdown", Go::Activity(s.id))),
        body,
    )
}

/* -------------------------------------------------------------- attention --- */

fn attention(ctx: &Ctx) -> Option<Block> {
    let items = &ctx.today.attention;
    if items.is_empty() {
        return None;
    }
    let warnings = items.iter().filter(|a| a.accent).count();
    let rows = items
        .iter()
        .map(|a| {
            // A note opens its Insights item, when the item is in the data.
            let id = a
                .insight
                .as_ref()
                .filter(|id| crate::insight::find(ctx.training, id).is_some());
            let sub = match id {
                Some(_) => Some("See the detail".into()),
                None => a.link.as_ref().map(|l| l.label.clone().into()),
            };
            let row = ListRow {
                glyph: if a.accent { Icon::Warning } else { Icon::Info },
                tone: if a.accent { Tone::Warn } else { Tone::Accent },
                title: a.text.clone().into(),
                sub,
                right: None,
                right_sub: None,
            };
            (row, id.map(|id| crate::insight::open_insight(id.clone())))
        })
        .collect();
    let meta = match warnings {
        0 => None,
        1 => Some("1 warning".to_string()),
        n => Some(format!("{n} warnings")),
    };
    block(meta, None, crate::insight::clickable_rows(rows, ctx.t))
}

/* ------------------------------------------------------------------- week --- */

fn week_bars(w: &Week) -> Vec<Bar> {
    w.minutes
        .iter()
        .zip(&w.axis)
        .map(|(m, day)| Bar {
            value: *m,
            label: minutes_label(*m).into(),
            day: day
                .chars()
                .next()
                .map(String::from)
                .unwrap_or_default()
                .into(),
        })
        .collect()
}

fn week_meta(w: &Week) -> String {
    let total = w.total_minutes.round() as i64;
    let mut parts = vec![
        match w.sessions {
            1 => "1 session".to_string(),
            n => format!("{n} sessions"),
        },
        format!("{} h {:02} m", total / 60, total % 60),
    ];
    parts.extend(w.distance.clone());
    parts.join(" · ")
}

fn week(ctx: &Ctx) -> Option<Block> {
    let w = ctx.today.week.as_ref().filter(|w| !w.minutes.is_empty())?;
    let average = w.minutes.iter().sum::<f32>() / w.minutes.len() as f32;
    let avg = (average > 0.0).then(|| (average, format!("avg {}", minutes_label(average))));
    block(
        Some(week_meta(w)),
        None,
        charts::bar_chart(&week_bars(w), avg, ctx.t),
    )
}

/* ------------------------------------------------------------------ drift --- */

fn drift(ctx: &Ctx) -> Option<Block> {
    let d: &Drift = ctx
        .today
        .zone_drift
        .as_ref()
        .filter(|d| d.runs.len() >= 2)?;
    let values: Vec<f32> = d.runs.iter().map(|r| r.above_z2).collect();
    let first = d.runs.first()?;
    let last = d.runs.last()?;
    let head = d.recent_avg.map(|avg| {
        (
            format!("{avg:.0}").into(),
            "%".into(),
            format!("above Z2 over {} {}", d.runs.len(), d.noun).into(),
        )
    });
    let chart = charts::area_chart(
        AreaChart {
            values: &values,
            colour: ctx.t.warn,
            height: Some(96.0),
            head,
            head_colour: Some(ctx.t.warn),
            x_first: first.date.clone().into(),
            x_last: last.date.clone().into(),
        },
        ctx.t,
    );
    let summary = (!d.summary.is_empty()).then(|| {
        padded(d.summary.clone())
            .pt(px(0.0))
            .text_size(px(12.0))
            .text_color(ctx.t.mutd)
    });
    block(
        Some(format!("share above Z2, {}", d.noun)),
        None,
        div().child(chart).children(summary),
    )
}

/* ------------------------------------------------------------------ night --- */

fn stage_colour(key: &str, t: &Theme) -> gpui::Hsla {
    match key {
        "deep" => t.acc,
        "rem" => t.acc.opacity(0.6),
        "awake" => t.warn,
        _ => t.fg.opacity(0.25),
    }
}

fn night(ctx: &Ctx) -> Option<Block> {
    let n = ctx.wellness?.sleep.last_night.as_ref()?;
    let total: f64 = n.stages.iter().map(|s| s.secs).sum();
    if total <= 0.0 {
        return None;
    }
    let t = ctx.t;
    let bar = n.stages.iter().filter(|s| s.secs > 0.0).fold(
        div().flex().h(px(26.0)).gap(px(2.0)),
        |bar, s| {
            bar.child(
                div()
                    .h_full()
                    .min_w(px(2.0))
                    .rounded(px(3.0))
                    .bg(stage_colour(&s.key, t))
                    .w(gpui::relative((s.secs / total) as f32)),
            )
        },
    );
    let legend = n.stages.iter().fold(
        div()
            .flex()
            .flex_wrap()
            .gap_x(px(16.0))
            .gap_y(px(4.0))
            .mt(px(10.0)),
        |row, s| {
            row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .text_color(t.mutd)
                    .child(
                        div()
                            .size(px(8.0))
                            .rounded(px(2.0))
                            .bg(stage_colour(&s.key, t)),
                    )
                    .child(format!("{} {}", s.label, s.text)),
            )
        },
    );
    let words = n.score.map_or(String::new(), |s| format!("score {s:.0}"));
    let meta = match (&n.start, &n.end) {
        (Some(a), Some(b)) => Some(format!("{a} – {b}")),
        _ => None,
    };
    block(
        meta,
        Some(("Sleep", Go::Screen(Screen::Sleep))),
        padded(
            div()
                .child(head(&n.duration_text, "", &words, t))
                .child(div().mt(px(12.0)).child(bar).child(legend)),
        ),
    )
}

/* ------------------------------------------------------------------- fuel --- */

fn kcal(value: f32) -> String {
    let n = value.round() as i64;
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn fuel_rows(eaten: f32, burned: f32) -> Vec<MeterRow> {
    let most = eaten.max(burned).max(1.0);
    vec![
        MeterRow {
            name: "Eaten".into(),
            fraction: eaten / most,
            value: kcal(eaten).into(),
            share: "kcal".into(),
            tone: Tone::Accent,
        },
        MeterRow {
            name: "Burned".into(),
            fraction: burned / most,
            value: kcal(burned).into(),
            share: "kcal".into(),
            tone: Tone::Muted,
        },
    ]
}

fn fuel(ctx: &Ctx) -> Option<Block> {
    let f: &Fuel = ctx.today.fuel.as_ref()?;
    let (eaten, burned) = (f.eaten?, f.burned?);
    let t = ctx.t;
    let pct = if burned > 0.0 {
        format!("{:.0}%", eaten / burned * 100.0)
    } else {
        String::new()
    };
    let body = padded(
        div()
            .child(head(
                &kcal(eaten),
                &format!("of {} kcal", kcal(burned)),
                &pct,
                t,
            ))
            .child(
                div()
                    .mt(px(12.0))
                    .child(charts::meter_rows(fuel_rows(eaten, burned), t)),
            ),
    );
    block(
        Some(f.when.clone()),
        Some(("Food", Go::Screen(Screen::Food))),
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Today;

    #[test]
    fn a_time_reads_short_or_long() {
        assert_eq!(minutes_label(0.0), "");
        assert_eq!(minutes_label(52.0), "52m");
        assert_eq!(minutes_label(74.0), "1:14");
        assert_eq!(clock(318.0), "5:18");
        assert_eq!(clock(3730.0), "1:02:10");
    }

    #[test]
    fn kcal_has_thousands_separators() {
        assert_eq!(kcal(1640.0), "1,640");
        assert_eq!(kcal(950.0), "950");
        assert_eq!(kcal(1_234_567.0), "1,234,567");
    }

    fn persona(name: &str) -> Today {
        crate::data::load_today(&crate::data::Source::Persona(name.into())).unwrap()
    }

    fn shown(name: &str) -> Vec<&'static str> {
        let theme = crate::theme::Palette::Paper.theme();
        let today = persona(name);
        let source = crate::data::Source::Persona(name.into());
        let body = crate::body::load_body(&source).unwrap();
        let places = crate::places::Places::load(&source);
        let store = crate::activities::Store::for_source(&source).unwrap();
        let list = store.list();
        let ctx = Ctx {
            t: &theme,
            today: &today,
            wellness: None,
            focus: crate::focus::Focus::today(),
            screen: crate::screen::Screen::Today,
            charts_view: None,
            tiles_view: None,
            extras_view: None,
            body: &body,
            training: None,
            places: &places,
            store: &store,
            activities: &list,
            activity_count: store.count(),
        };
        BLOCKS
            .iter()
            .filter(|b| (b.render)(&ctx).is_some())
            .map(|b| b.id)
            .collect()
    }

    #[test]
    fn a_block_hides_when_its_data_is_missing() {
        let sample = shown("sample");
        for id in ["health", "last", "week", "drift"] {
            assert!(sample.contains(&id), "{id}");
        }
        // The night needs the Sleep view, which these tests do not load.
        assert!(!sample.contains(&"night"));
        let golfer = shown("golfer");
        assert!(!golfer.contains(&"drift"));
        assert!(!shown("new_user").contains(&"last"));
    }

    #[test]
    fn no_persona_shows_a_dash_where_a_figure_is_missing() {
        for name in crate::data::PERSONAS.map(|(n, _)| n) {
            let today = persona(name);
            for v in &today.vitals {
                assert_ne!(v.value, "—", "{name}: {}", v.label);
            }
            for s in today.last_session.iter().flat_map(|s| &s.stats) {
                assert_ne!(s.value, "—", "{name}: {}", s.label);
            }
        }
    }

    #[test]
    fn a_zone_verdict_shows_only_for_endurance_sport() {
        for name in ["golfer", "gym"] {
            assert!(
                persona(name).last_session.unwrap().zone_secs.is_none(),
                "{name}"
            );
        }
        assert!(persona("sample").last_session.unwrap().zone_secs.is_some());
    }
}
