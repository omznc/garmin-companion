//! The watches and other devices on the account.
//!
//! No endpoint sends a battery level, so this holds a name, a firmware version
//! and the time of the last upload. That is what the Gear screen can show.

use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;

pub(crate) const SCHEMA: &str = r#"
    CREATE TABLE IF NOT EXISTS devices (
        device_id      INTEGER PRIMARY KEY,
        name           TEXT,
        firmware       TEXT,
        -- Milliseconds since the epoch. Null for a device that never uploaded
        -- as the last used one.
        last_upload_ms INTEGER
    );
"#;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub device_id: i64,
    pub name: Option<String>,
    pub firmware: Option<String>,
    pub last_upload_ms: Option<i64>,
}

/// The devices in a registration reply. An item with no id is skipped.
pub fn parse_devices(v: &Value) -> Vec<DeviceInfo> {
    let Some(list) = v.as_array() else {
        return Vec::new();
    };
    let text = |d: &Value, k: &str| d.get(k).and_then(Value::as_str).map(str::to_owned);
    list.iter()
        .filter_map(|d| {
            Some(DeviceInfo {
                device_id: d.get("deviceId")?.as_i64()?,
                name: text(d, "productDisplayName").or_else(|| text(d, "displayName")),
                firmware: text(d, "currentFirmwareVersion"),
                last_upload_ms: None,
            })
        })
        .collect()
}

/// Put the upload time of the last used device onto its entry. The device id
/// is the key. The name is the fallback, because the two endpoints do not name
/// a device the same way in every reply.
pub fn apply_last_used(devices: &mut [DeviceInfo], last: &Value) {
    let Some(at) = last.get("lastUsedDeviceUploadTime").and_then(Value::as_i64) else {
        return;
    };
    let id = last.get("userDeviceId").and_then(Value::as_i64);
    let name = last.get("lastUsedDeviceName").and_then(Value::as_str);
    let target = devices
        .iter()
        .position(|d| Some(d.device_id) == id)
        .or_else(|| name.and_then(|n| devices.iter().position(|d| d.name.as_deref() == Some(n))));
    if let Some(i) = target {
        devices[i].last_upload_ms = Some(at);
    }
}

impl Db {
    /// Replace the device list. An empty list writes nothing: the account
    /// kept its devices, and the reply only had none to show.
    pub fn replace_devices(&self, devices: &[DeviceInfo]) -> Result<()> {
        if devices.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM devices", [])?;
        for d in devices {
            tx.execute(
                "INSERT OR REPLACE INTO devices (device_id, name, firmware, last_upload_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![d.device_id, d.name, d.firmware, d.last_upload_ms],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The devices, the one that uploaded last first.
    pub fn devices(&self) -> Result<Vec<DeviceInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT device_id, name, firmware, last_upload_ms FROM devices
             ORDER BY last_upload_ms DESC, device_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DeviceInfo {
                    device_id: r.get(0)?,
                    name: r.get(1)?,
                    firmware: r.get(2)?,
                    last_upload_ms: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn listed() -> Value {
        json!([
            { "deviceId": 11_000_000_000_i64, "productDisplayName": "Watch A", "currentFirmwareVersion": "1.2" },
            { "deviceId": 22, "displayName": "Band B" },
            { "productDisplayName": "No id" },
            "junk",
        ])
    }

    #[test]
    fn a_device_keeps_a_big_id_and_an_item_with_no_id_is_skipped() {
        let got = parse_devices(&listed());
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].device_id, 11_000_000_000);
        assert_eq!(got[0].firmware.as_deref(), Some("1.2"));
        assert_eq!(got[1].name.as_deref(), Some("Band B"));
        assert!(parse_devices(&json!({})).is_empty());
    }

    #[test]
    fn the_upload_time_goes_to_the_device_by_id_and_then_by_name() {
        let mut d = parse_devices(&listed());
        apply_last_used(
            &mut d,
            &json!({ "userDeviceId": 22, "lastUsedDeviceName": "Watch A", "lastUsedDeviceUploadTime": 500 }),
        );
        assert_eq!(
            (d[0].last_upload_ms, d[1].last_upload_ms),
            (None, Some(500))
        );

        let mut d = parse_devices(&listed());
        apply_last_used(
            &mut d,
            &json!({ "userDeviceId": 99, "lastUsedDeviceName": "Watch A", "lastUsedDeviceUploadTime": 700 }),
        );
        assert_eq!(d[0].last_upload_ms, Some(700));

        let mut d = parse_devices(&listed());
        apply_last_used(&mut d, &json!({ "userDeviceId": 22 }));
        assert!(d.iter().all(|x| x.last_upload_ms.is_none()));
    }

    #[test]
    fn the_list_is_replaced_and_an_empty_one_keeps_it() {
        let db = crate::db::scratch_db("devices");
        let mut d = parse_devices(&listed());
        apply_last_used(
            &mut d,
            &json!({ "userDeviceId": 22, "lastUsedDeviceUploadTime": 5 }),
        );
        db.replace_devices(&d).unwrap();
        db.replace_devices(&[]).unwrap();
        let got = db.devices().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].device_id, 22, "the last upload is first");
        db.replace_devices(&d[..1]).unwrap();
        assert_eq!(db.devices().unwrap().len(), 1);
    }
}
