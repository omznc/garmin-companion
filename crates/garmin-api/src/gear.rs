//! The Gear screen as a view model.
//!
//! Garmin keeps gear, such as shoes, bikes and other items, on its own service.
//! The cache holds none of it. [`fetch_gear`] reads it live, and [`gear_view`]
//! turns the entries into the view. `gear_view` is pure, so a test needs no network.

use crate::body::EmptyNote;
use crate::places::km_text;
use crate::ApiError;
use garmin_core::client::GearItem;
use serde::{Deserialize, Serialize};

/// One item and its lifetime totals. The totals come from a second endpoint,
/// so a failure there leaves them out and keeps the item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GearEntry {
    pub item: GearItem,
    pub distance_m: Option<f64>,
    pub activities: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GearRow {
    pub name: String,
    pub kind: Option<String>,
    pub distance: Option<String>,
    /// `Good`, `Watch`, `Retire soon` or `Retired`. Empty when no limit exists.
    pub status: String,
    /// Only an item close to its limit takes the accent.
    pub accent: bool,
    /// Wear against the limit that the user set, from 0 to 100.
    pub wear_pct: Option<f64>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GearView {
    pub empty: Option<EmptyNote>,
    pub items: Vec<GearRow>,
    pub note: String,
}

/// Read the gear list and the totals of each item from Garmin.
///
/// This calls the network and blocks. Call it from a worker thread.
pub fn fetch_gear() -> Result<Vec<GearEntry>, ApiError> {
    let client = garmin_core::client_from_keyring()?.ok_or(ApiError::NotConnected)?;
    crate::session::block_on(async {
        let id = client
            .profile()
            .await?
            .profile_id
            .ok_or_else(|| ApiError::Internal("Garmin did not return a profile id.".into()))?;
        let mut out = Vec::new();
        for item in client.gear(id).await? {
            let stats = client.gear_stats(&item.uuid).await.ok();
            let num = |key: &str| stats.as_ref().and_then(|s| s.get(key)?.as_f64());
            out.push(GearEntry {
                distance_m: num("totalDistance"),
                activities: num("totalActivities").map(|n| n as u64),
                item,
            });
        }
        Ok(out)
    })
}

/// `SHOES` and `road_bike` become `Shoes` and `Road bike`.
fn kind_label(raw: &str) -> String {
    let text = raw.replace('_', " ").to_lowercase();
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn status_of(pct: Option<f64>, retired: bool) -> &'static str {
    match pct {
        _ if retired => "Retired",
        None => "",
        Some(p) if p >= 90.0 => "Retire soon",
        Some(p) if p >= 70.0 => "Watch",
        Some(_) => "Good",
    }
}

fn row_of(e: &GearEntry) -> GearRow {
    let item = &e.item;
    let distance = e.distance_m.unwrap_or(0.0);
    let limit = item.maximum_meters.filter(|m| *m > 0.0);
    let pct = limit.map(|l| (distance / l * 100.0).min(100.0));
    let retired = item
        .gear_status_name
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("retired"));
    let status = status_of(pct, retired);
    let mut detail = Vec::new();
    if let Some(n) = e.activities {
        detail.push(format!(
            "{n} {}",
            if n == 1 { "activity" } else { "activities" }
        ));
    }
    detail.extend(limit.map(|l| format!("limit {}", km_text(l))));
    GearRow {
        name: item
            .display_name
            .clone()
            .or_else(|| item.custom_make_model.clone())
            .unwrap_or_else(|| "Unnamed".into()),
        kind: item.gear_type_name.as_deref().map(kind_label),
        distance: (distance > 0.0).then(|| km_text(distance)),
        accent: !retired && pct.is_some_and(|p| p >= 90.0),
        status: status.into(),
        wear_pct: pct,
        detail: (!detail.is_empty()).then(|| detail.join(" · ")),
    }
}

pub fn gear_view(entries: &[GearEntry]) -> GearView {
    let note = "Distances come from the totals that Garmin keeps for each item. The wear bar \
                shows the distance against the limit that you set. An item with no limit has \
                no bar, because the app does not guess a limit."
        .to_string();
    if entries.is_empty() {
        return GearView {
            empty: Some(EmptyNote {
                title: "No gear registered.".into(),
                body: "Garmin tracks the distance of shoes, bikes and other gear that you add in \
                       Garmin Connect and attach to your activities. Nothing is registered on \
                       this account yet, so there is no wear to report. This screen fills in \
                       when you add gear."
                    .into(),
            }),
            items: Vec::new(),
            note: String::new(),
        };
    }
    GearView {
        empty: None,
        items: entries.iter().map(row_of).collect(),
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(limit: Option<f64>, distance: Option<f64>, status: &str) -> GearEntry {
        GearEntry {
            item: serde_json::from_value(serde_json::json!({
                "uuid": "u", "displayName": "Pair", "gearTypeName": "SHOES",
                "maximumMeters": limit, "gearStatusName": status,
            }))
            .unwrap(),
            distance_m: distance,
            activities: Some(1),
        }
    }

    #[test]
    fn wear_has_three_states_and_only_with_a_limit() {
        let state = |pct, retired| status_of(pct, retired);
        assert_eq!(state(Some(10.0), false), "Good");
        assert_eq!(state(Some(70.0), false), "Watch");
        assert_eq!(state(Some(95.0), false), "Retire soon");
        assert_eq!(state(None, false), "");
        assert_eq!(state(Some(10.0), true), "Retired");
    }

    #[test]
    fn a_row_caps_the_bar_and_marks_a_worn_item() {
        let row = row_of(&entry(Some(800_000.0), Some(900_000.0), "active"));
        assert_eq!(row.wear_pct, Some(100.0));
        assert!(row.accent);
        assert_eq!(row.kind.as_deref(), Some("Shoes"));
        assert_eq!(row.detail.as_deref(), Some("1 activity · limit 800 km"));
    }

    #[test]
    fn an_item_with_no_limit_and_no_distance_is_plain() {
        let row = row_of(&entry(None, None, "active"));
        assert_eq!(
            (row.wear_pct, row.distance, row.accent),
            (None, None, false)
        );
        assert_eq!(row.status, "");
    }

    #[test]
    fn no_gear_is_a_state_with_a_reason() {
        let v = gear_view(&[]);
        assert!(v.empty.is_some() && v.items.is_empty());
    }
}
