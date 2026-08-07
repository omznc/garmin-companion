//! Authenticated Garmin Connect data client.
//!
//! Everything here goes through `get_json`, which refreshes the access token on
//! demand and retries once on a 401. Callers never think about token lifetime.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::auth::{self, Tokens, CONNECT_API};

pub struct GarminClient {
    http: reqwest::Client,
    tokens: Arc<Mutex<Tokens>>,
    /// Called whenever the token pair changes, so the caller can persist it.
    /// Garmin rotates refresh tokens, so dropping this loses the session.
    on_tokens_changed: Arc<dyn Fn(&Tokens) + Send + Sync>,
}

impl GarminClient {
    pub fn new(
        tokens: Tokens,
        on_tokens_changed: Arc<dyn Fn(&Tokens) + Send + Sync>,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .timeout(std::time::Duration::from_secs(45))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            http,
            tokens: Arc::new(Mutex::new(tokens)),
            on_tokens_changed,
        })
    }

    /// Return a valid access token, refreshing first if it's expired.
    async fn access_token(&self) -> Result<String> {
        let mut guard = self.tokens.lock().await;
        if guard.is_expired() {
            let fresh = auth::refresh(&self.http, &guard).await?;
            (self.on_tokens_changed)(&fresh);
            *guard = fresh;
        }
        Ok(guard.di_token.clone())
    }

    /// Force a refresh regardless of the cached expiry. Used when the API
    /// rejects a token we believed was still valid — clock skew, or Garmin
    /// invalidating it server-side.
    async fn force_refresh(&self) -> Result<String> {
        let mut guard = self.tokens.lock().await;
        let fresh = auth::refresh(&self.http, &guard).await?;
        (self.on_tokens_changed)(&fresh);
        *guard = fresh;
        Ok(guard.di_token.clone())
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let url = format!("{CONNECT_API}{path}");

        let mut token = self.access_token().await?;
        let mut attempted_refresh = false;

        loop {
            let resp = self
                .http
                .get(&url)
                .headers(auth::native_headers())
                .header("Authorization", format!("Bearer {token}"))
                .header("Accept", "application/json")
                .query(query)
                .send()
                .await
                .with_context(|| format!("request to {path} failed"))?;

            let status = resp.status();

            if status == reqwest::StatusCode::UNAUTHORIZED && !attempted_refresh {
                attempted_refresh = true;
                token = self.force_refresh().await?;
                continue;
            }

            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(anyhow!(
                    "{path} returned {}: {}",
                    status,
                    body.chars().take(300).collect::<String>()
                ));
            }

            let body = resp.text().await.context("failed to read response body")?;
            return serde_json::from_str(&body).with_context(|| {
                format!(
                    "{path} returned unexpected shape: {}",
                    body.chars().take(300).collect::<String>()
                )
            });
        }
    }

    /// Most recent activities, newest first.
    pub async fn activities(&self, start: u32, limit: u32) -> Result<Vec<ActivitySummary>> {
        self.get_json(
            "/activitylist-service/activities/search/activities",
            &[("start", start.to_string()), ("limit", limit.to_string())],
        )
        .await
    }

    /// Full detail for one activity, including the `summaryDTO` block.
    pub async fn activity(&self, id: i64) -> Result<serde_json::Value> {
        self.get_json(&format!("/activity-service/activity/{id}"), &[])
            .await
    }

    /// Per-lap splits. Treadmill runs report these per kilometre by default.
    pub async fn activity_splits(&self, id: i64) -> Result<serde_json::Value> {
        self.get_json(&format!("/activity-service/activity/{id}/splits"), &[])
            .await
    }

    /// Sampled time series for one activity — HR, pace, cadence, elevation and,
    /// outdoors, the GPS polyline.
    ///
    /// Garmin returns one sample per second unless capped, which is megabytes
    /// for a long session. `points` downsamples server-side to roughly that many
    /// samples, which is all a chart 720px wide can show anyway.
    pub async fn activity_details(&self, id: i64, points: u32) -> Result<serde_json::Value> {
        self.get_json(
            &format!("/activity-service/activity/{id}/details"),
            &[
                ("maxChartSize", points.to_string()),
                ("maxPolylineSize", points.to_string()),
            ],
        )
        .await
    }

    /// Registered shoes, bikes and the rest, with their accumulated distance.
    pub async fn gear(&self, profile_id: i64) -> Result<Vec<GearItem>> {
        self.get_json(
            "/gear-service/gear/filterGear",
            &[("userProfilePk", profile_id.to_string())],
        )
        .await
    }

    /// Lifetime distance and activity count for one piece of gear. Garmin keeps
    /// this on a separate endpoint from the gear list itself.
    pub async fn gear_stats(&self, gear_uuid: &str) -> Result<serde_json::Value> {
        self.get_json(&format!("/gear-service/gear/stats/{gear_uuid}"), &[])
            .await
    }

    /// Time-in-zone breakdown — the number this app exists to show.
    pub async fn hr_time_in_zones(&self, id: i64) -> Result<Vec<HrZoneBucket>> {
        self.get_json(
            &format!("/activity-service/activity/{id}/hrTimeInZones"),
            &[],
        )
        .await
    }

    /// Daily wellness rollup: resting HR, steps, stress, body battery.
    pub async fn user_summary(&self, display_name: &str, date: &str) -> Result<serde_json::Value> {
        self.get_json(
            &format!("/usersummary-service/usersummary/daily/{display_name}"),
            &[("calendarDate", date.to_string())],
        )
        .await
    }

    /// Water logged against goal for one day, plus the sweat loss Garmin
    /// credits from that day's activities.
    pub async fn hydration(&self, date: &str) -> Result<serde_json::Value> {
        self.get_json(
            &format!("/usersummary-service/usersummary/hydration/allData/{date}"),
            &[],
        )
        .await
    }

    /// The athlete's saved workouts — structured sessions they built in Garmin
    /// Connect, which is the closest thing the account holds to a plan.
    pub async fn workouts(&self, limit: u32) -> Result<Vec<serde_json::Value>> {
        self.get_json(
            "/workout-service/workouts",
            &[("start", "0".into()), ("limit", limit.to_string())],
        )
        .await
    }

    /// One calendar month. Garmin numbers months from zero here, unlike every
    /// other date in this API, so callers pass what Garmin expects.
    pub async fn calendar(&self, year: i32, month_zero_based: u32) -> Result<serde_json::Value> {
        self.get_json(
            &format!("/calendar-service/year/{year}/month/{month_zero_based}"),
            &[],
        )
        .await
    }

    pub async fn training_readiness(&self, date: &str) -> Result<serde_json::Value> {
        self.get_json(
            &format!("/metrics-service/metrics/trainingreadiness/{date}"),
            &[],
        )
        .await
    }

    /// VO2 max and friends. Stays empty until an outdoor GPS run exists —
    /// treadmill runs never populate it.
    pub async fn max_metrics(&self, date: &str) -> Result<serde_json::Value> {
        self.get_json(
            &format!("/metrics-service/metrics/maxmet/latest/{date}"),
            &[],
        )
        .await
    }

    pub async fn hrv(&self, date: &str) -> Result<serde_json::Value> {
        self.get_json(&format!("/hrv-service/hrv/{date}"), &[])
            .await
    }

    pub async fn sleep(&self, display_name: &str, date: &str) -> Result<serde_json::Value> {
        self.get_json(
            &format!("/wellness-service/wellness/dailySleepData/{display_name}"),
            &[
                ("date", date.to_string()),
                ("nonSleepBufferMinutes", "60".to_string()),
            ],
        )
        .await
    }

    /// The account's `displayName`, which several wellness endpoints need in
    /// their path.
    pub async fn profile(&self) -> Result<Profile> {
        self.get_json("/userprofile-service/socialProfile", &[])
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub display_name: String,
    #[serde(default)]
    pub full_name: Option<String>,
    /// Needed as `userProfilePk` by the gear service. Distinct from `id`, which
    /// is a different key on the same payload and is not accepted there.
    #[serde(default)]
    pub profile_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GearItem {
    pub uuid: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub custom_make_model: Option<String>,
    #[serde(default)]
    pub gear_type_name: Option<String>,
    /// Retirement threshold in metres, if the user set one.
    #[serde(default)]
    pub maximum_meters: Option<f64>,
    #[serde(default)]
    pub gear_status_name: Option<String>,
    #[serde(default)]
    pub date_begin: Option<String>,
    #[serde(default)]
    pub date_end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySummary {
    pub activity_id: i64,
    #[serde(default)]
    pub activity_name: Option<String>,
    /// Local wall-clock start, "YYYY-MM-DD HH:MM:SS".
    #[serde(default)]
    pub start_time_local: Option<String>,
    #[serde(default)]
    pub distance: Option<f64>,
    #[serde(default)]
    pub duration: Option<f64>,
    // Garmin spells these `averageHR`/`maxHR`, not the camelCase the rest of
    // the payload uses — they silently deserialize to None without the rename.
    #[serde(default, rename = "averageHR")]
    pub average_hr: Option<f64>,
    #[serde(default, rename = "maxHR")]
    pub max_hr: Option<f64>,
    #[serde(default)]
    pub average_running_cadence_in_steps_per_minute: Option<f64>,
    #[serde(default)]
    pub max_running_cadence_in_steps_per_minute: Option<f64>,
    #[serde(default)]
    pub average_speed: Option<f64>,
    #[serde(default)]
    pub calories: Option<f64>,
    #[serde(default)]
    pub aerobic_training_effect: Option<f64>,
    #[serde(default)]
    pub anaerobic_training_effect: Option<f64>,
    #[serde(default)]
    pub activity_type: Option<ActivityType>,
    /// Metres climbed. Absent on treadmill and indoor activities.
    #[serde(default)]
    pub elevation_gain: Option<f64>,
    /// Seconds excluding pauses. Usually shorter than `duration` on runs with
    /// walk breaks, which is most of this account's.
    #[serde(default)]
    pub moving_duration: Option<f64>,
    #[serde(default)]
    pub steps: Option<i64>,
    #[serde(default)]
    pub lap_count: Option<i64>,
    /// Whether a GPS trace exists — false for every treadmill session.
    #[serde(default)]
    pub has_polyline: Option<bool>,

    // The list payload already carries the zone split, so a sync doesn't need
    // one extra request per activity just to get it.
    #[serde(default, rename = "hrTimeInZone_1")]
    pub hr_time_in_zone_1: Option<f64>,
    #[serde(default, rename = "hrTimeInZone_2")]
    pub hr_time_in_zone_2: Option<f64>,
    #[serde(default, rename = "hrTimeInZone_3")]
    pub hr_time_in_zone_3: Option<f64>,
    #[serde(default, rename = "hrTimeInZone_4")]
    pub hr_time_in_zone_4: Option<f64>,
    #[serde(default, rename = "hrTimeInZone_5")]
    pub hr_time_in_zone_5: Option<f64>,
}

impl ActivitySummary {
    pub fn type_key(&self) -> &str {
        self.activity_type
            .as_ref()
            .and_then(|t| t.type_key.as_deref())
            .unwrap_or("unknown")
    }

    /// Calendar date portion of `startTimeLocal` ("YYYY-MM-DD HH:MM:SS").
    pub fn local_date(&self) -> Option<&str> {
        self.start_time_local.as_deref()?.split(' ').next()
    }

    pub fn zone_secs(&self) -> [f64; 5] {
        [
            self.hr_time_in_zone_1.unwrap_or(0.0),
            self.hr_time_in_zone_2.unwrap_or(0.0),
            self.hr_time_in_zone_3.unwrap_or(0.0),
            self.hr_time_in_zone_4.unwrap_or(0.0),
            self.hr_time_in_zone_5.unwrap_or(0.0),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityType {
    #[serde(default)]
    pub type_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HrZoneBucket {
    pub zone_number: i64,
    #[serde(default)]
    pub secs_in_zone: Option<f64>,
    #[serde(default)]
    pub zone_low_boundary: Option<i64>,
}
