//! The blocks of the Gear screen: the shoes and the devices.

use crate::scale::px;
use chrono::Local;
use garmin_api::gear::GearRow;
use gpui::{div, prelude::*};

use super::{Block, BlockDef, Ctx, Width};
use crate::charts::{self, MeterRow, Tone};
use crate::icons::Icon;
use crate::kit::{self, ListRow};
use crate::screen::Screen;

/// The blocks that belong to this screen.
pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "gear_shoes",
        title: "Shoes",
        group: Screen::Gear,
        default_width: Width::TwoThirds,
        render: shoes,
    },
    BlockDef {
        id: "gear_devices",
        title: "Devices",
        group: Screen::Gear,
        default_width: Width::Third,
        render: devices,
    },
    BlockDef {
        id: "gear_all",
        title: "All gear",
        group: Screen::Gear,
        default_width: Width::Full,
        render: all_gear,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("gear_shoes", Width::TwoThirds),
    ("gear_devices", Width::Third),
    ("gear_all", Width::Full),
];

pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

pub fn lede(_ctx: &Ctx) -> Option<String> {
    Some("The distance on your gear, and the devices on your account.".into())
}

pub fn crumb(_ctx: &Ctx) -> Option<String> {
    None
}

fn block(meta: Option<String>, body: impl IntoElement) -> Option<Block> {
    Some(Block {
        meta: meta.map(Into::into),
        link: None,
        body: body.into_any_element(),
    })
}

/* ------------------------------------------------------------------ shoes --- */

fn is_shoe(row: &GearRow) -> bool {
    row.kind
        .as_deref()
        .is_some_and(|k| k.to_lowercase().contains("shoe"))
}

/// Retired is muted, a worn pair takes the warning, the rest the accent.
fn shoe_row(g: &GearRow) -> MeterRow {
    let tone = if g.status == "Retired" {
        Tone::Muted
    } else if g.accent {
        Tone::Warn
    } else {
        Tone::Accent
    };
    MeterRow {
        name: g.name.clone().into(),
        fraction: g.wear_pct.map_or(0.0, |p| p as f32 / 100.0),
        value: g.distance.clone().unwrap_or_default().into(),
        share: g
            .wear_pct
            .map(|p| format!("{}%", p.round()))
            .unwrap_or_default()
            .into(),
        tone,
    }
}

fn shoes(ctx: &Ctx) -> Option<Block> {
    let view = ctx.places.gear()?;
    let rows: Vec<MeterRow> = view
        .items
        .iter()
        .filter(|g| is_shoe(g))
        .map(shoe_row)
        .collect();
    if rows.is_empty() {
        return None;
    }
    let body = div()
        .px(px(12.0))
        .py(px(12.0))
        .child(charts::meter_rows(rows, ctx.t));
    block(None, body)
}

/// Every item of gear, with its distance, state and detail. It shows retired gear too.
fn all_gear(ctx: &Ctx) -> Option<Block> {
    let items = &ctx.places.gear()?.items;
    if items.is_empty() {
        return None;
    }
    let column = |title: &str, numeric| kit::Column {
        title: title.to_string().into(),
        numeric,
    };
    let columns = [
        column("Name", false),
        column("Kind", false),
        column("Distance", true),
        column("State", false),
        column("Detail", false),
    ];
    let rows = items
        .iter()
        .map(|g| {
            vec![
                g.name.clone().into(),
                g.kind.clone().unwrap_or_default().into(),
                g.distance.clone().unwrap_or_default().into(),
                g.status.clone().into(),
                g.detail.clone().unwrap_or_default().into(),
            ]
        })
        .collect();
    block(
        Some(format!("{} items", items.len())),
        kit::table(&columns, rows, ctx.t),
    )
}

/* ---------------------------------------------------------------- devices --- */

/// "3 hours ago" for the time `ms` (Unix milliseconds), seen at `now_ms`.
fn ago(ms: i64, now_ms: i64) -> String {
    let minutes = ((now_ms - ms) / 60_000).max(0);
    let plural = |n: i64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match minutes {
        0..=59 => plural(minutes, "minute"),
        60..=2879 => plural(minutes / 60, "hour"),
        _ => plural(minutes / 1440, "day"),
    }
}

fn devices(ctx: &Ctx) -> Option<Block> {
    let list = &ctx.extras_view?.devices;
    if list.is_empty() {
        return None;
    }
    let now = Local::now().timestamp_millis();
    let rows = list
        .iter()
        .map(|d| ListRow {
            glyph: Icon::NavGear,
            tone: Tone::Muted,
            title: d
                .name
                .clone()
                .unwrap_or_else(|| "Garmin device".into())
                .into(),
            sub: d
                .last_upload_ms
                .map(|ms| format!("Last sync {}", ago(ms, now)).into()),
            right: None,
            right_sub: None,
        })
        .collect();
    block(None, kit::list_rows(rows, ctx.t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_age_of_a_sync_reads_in_the_nearest_unit() {
        let now = 10 * 86_400_000;
        assert_eq!(ago(now - 30 * 60_000, now), "30 minutes ago");
        assert_eq!(ago(now - 3 * 3_600_000, now), "3 hours ago");
        assert_eq!(ago(now - 3 * 86_400_000, now), "3 days ago");
        assert_eq!(ago(now - 60_000, now), "1 minute ago");
    }
}
