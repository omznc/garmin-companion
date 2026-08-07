//! Local SQLite cache.
//!
//! Everything the app and the MCP server read comes from here, never straight
//! from Garmin. That keeps queries instant, keeps history around after Garmin
//! inevitably changes something, and means an LLM tool call can't stall on a
//! network round trip.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::client::ActivitySummary;

pub struct Db {
    conn: Connection,
}

/// Column list shared by every `CachedActivity` query, so the indices in
/// `map_activity` only have to line up in one place.
const ACTIVITY_COLS: &str = "activity_id, name, type_key, start_time_local, local_date,
     distance_m, duration_s, moving_duration_s, avg_hr, max_hr, avg_cadence,
     calories, elevation_gain, steps, aerobic_te, anaerobic_te,
     z1_secs, z2_secs, z3_secs, z4_secs, z5_secs";

fn map_activity(r: &rusqlite::Row) -> rusqlite::Result<CachedActivity> {
    Ok(CachedActivity {
        activity_id: r.get(0)?,
        name: r.get(1)?,
        type_key: r.get(2)?,
        start_time_local: r.get(3)?,
        local_date: r.get(4)?,
        distance_m: r.get(5)?,
        duration_s: r.get(6)?,
        moving_duration_s: r.get(7)?,
        avg_hr: r.get(8)?,
        max_hr: r.get(9)?,
        avg_cadence: r.get(10)?,
        calories: r.get(11)?,
        elevation_gain: r.get(12)?,
        steps: r.get(13)?,
        aerobic_te: r.get(14)?,
        anaerobic_te: r.get(15)?,
        zone_secs: [r.get(16)?, r.get(17)?, r.get(18)?, r.get(19)?, r.get(20)?],
    })
}

/// Default on-disk location, alongside the other app data for this user.
pub fn default_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("garmin-coach").join("cache.sqlite3"))
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("could not open {}", path.display()))?;
        // WAL so the desktop app and the MCP server can both hold the cache
        // open without blocking each other.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_default() -> Result<Self> {
        let path = default_path().context("could not locate a data directory")?;
        Self::open(&path)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS activities (
                activity_id     INTEGER PRIMARY KEY,
                name            TEXT,
                type_key        TEXT,
                start_time_local TEXT,
                local_date      TEXT,
                distance_m      REAL,
                duration_s      REAL,
                avg_hr          REAL,
                max_hr          REAL,
                avg_cadence     REAL,
                max_cadence     REAL,
                avg_speed       REAL,
                calories        REAL,
                aerobic_te      REAL,
                anaerobic_te    REAL,
                z1_secs         REAL NOT NULL DEFAULT 0,
                z2_secs         REAL NOT NULL DEFAULT 0,
                z3_secs         REAL NOT NULL DEFAULT 0,
                z4_secs         REAL NOT NULL DEFAULT 0,
                z5_secs         REAL NOT NULL DEFAULT 0,
                raw             TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_activities_date
                ON activities(local_date DESC);
            CREATE INDEX IF NOT EXISTS idx_activities_type_date
                ON activities(type_key, local_date DESC);

            CREATE TABLE IF NOT EXISTS daily_metrics (
                date                TEXT PRIMARY KEY,
                resting_hr          REAL,
                hrv_last_night      REAL,
                hrv_weekly_avg      REAL,
                hrv_status          TEXT,
                training_readiness  REAL,
                sleep_secs          REAL,
                sleep_score         REAL,
                steps               INTEGER,
                stress_avg          REAL,
                body_battery_high   REAL,
                body_battery_low    REAL,
                raw                 TEXT
            );

            CREATE TABLE IF NOT EXISTS sync_state (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- Structured sessions the athlete built in Garmin Connect. Garmin
            -- holds no training plan for this account, so these plus the
            -- calendar are what a "plan" can honestly be built from.
            CREATE TABLE IF NOT EXISTS workouts (
                workout_id   INTEGER PRIMARY KEY,
                name         TEXT,
                sport_type   TEXT,
                description  TEXT,
                est_duration_s REAL,
                est_distance_m REAL,
                updated_at   TEXT,
                raw          TEXT
            );

            -- One row per activity that actually carries GPS. Kept apart from
            -- `activities` because the trace is orders of magnitude larger than
            -- the summary and only a fraction of activities have one.
            CREATE TABLE IF NOT EXISTS activity_tracks (
                activity_id  INTEGER PRIMARY KEY,
                point_count  INTEGER NOT NULL,
                start_lat    REAL, start_lon REAL,
                end_lat      REAL, end_lon   REAL,
                min_lat      REAL, max_lat   REAL,
                min_lon      REAL, max_lon   REAL,
                -- Downsampled [[lat,lon],…] for drawing; not survey-grade.
                points       TEXT NOT NULL
            );
            "#,
        )?;

        // Columns added after the first release. SQLite has no
        // `ADD COLUMN IF NOT EXISTS`, and a duplicate column is the expected
        // outcome on every run after the first, so it isn't an error here.
        for ddl in [
            "ALTER TABLE activities ADD COLUMN elevation_gain REAL",
            "ALTER TABLE activities ADD COLUMN moving_duration_s REAL",
            "ALTER TABLE activities ADD COLUMN steps INTEGER",
            "ALTER TABLE activities ADD COLUMN has_polyline INTEGER",
            // Nutrition and hydration. These ride along on the daily summary
            // the sync already fetches, so keeping them costs one column each
            // and no extra request.
            "ALTER TABLE daily_metrics ADD COLUMN consumed_kcal REAL",
            "ALTER TABLE daily_metrics ADD COLUMN total_burn_kcal REAL",
            "ALTER TABLE daily_metrics ADD COLUMN active_kcal REAL",
            "ALTER TABLE daily_metrics ADD COLUMN bmr_kcal REAL",
            "ALTER TABLE daily_metrics ADD COLUMN net_calorie_goal REAL",
            "ALTER TABLE daily_metrics ADD COLUMN hydration_ml REAL",
            "ALTER TABLE daily_metrics ADD COLUMN hydration_goal_ml REAL",
            "ALTER TABLE daily_metrics ADD COLUMN sweat_loss_ml REAL",
        ] {
            match self.conn.execute(ddl, []) {
                Ok(_) => {}
                Err(e) if e.to_string().contains("duplicate column name") => {}
                Err(e) => return Err(e).context("could not migrate the cache schema"),
            }
        }
        Ok(())
    }

    pub fn upsert_activity(&self, a: &ActivitySummary) -> Result<()> {
        let z = a.zone_secs();
        let raw = serde_json::to_string(a).unwrap_or_default();
        self.conn.execute(
            r#"
            INSERT INTO activities (
                activity_id, name, type_key, start_time_local, local_date,
                distance_m, duration_s, avg_hr, max_hr, avg_cadence, max_cadence,
                avg_speed, calories, aerobic_te, anaerobic_te,
                z1_secs, z2_secs, z3_secs, z4_secs, z5_secs, raw,
                elevation_gain, moving_duration_s, steps, has_polyline
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21,
                ?22, ?23, ?24, ?25
            )
            ON CONFLICT(activity_id) DO UPDATE SET
                name=excluded.name, type_key=excluded.type_key,
                start_time_local=excluded.start_time_local,
                local_date=excluded.local_date, distance_m=excluded.distance_m,
                duration_s=excluded.duration_s, avg_hr=excluded.avg_hr,
                max_hr=excluded.max_hr, avg_cadence=excluded.avg_cadence,
                max_cadence=excluded.max_cadence, avg_speed=excluded.avg_speed,
                calories=excluded.calories, aerobic_te=excluded.aerobic_te,
                anaerobic_te=excluded.anaerobic_te,
                z1_secs=excluded.z1_secs, z2_secs=excluded.z2_secs,
                z3_secs=excluded.z3_secs, z4_secs=excluded.z4_secs,
                z5_secs=excluded.z5_secs, raw=excluded.raw,
                elevation_gain=excluded.elevation_gain,
                moving_duration_s=excluded.moving_duration_s,
                steps=excluded.steps, has_polyline=excluded.has_polyline
            "#,
            params![
                a.activity_id,
                a.activity_name,
                a.type_key(),
                a.start_time_local,
                a.local_date(),
                a.distance,
                a.duration,
                a.average_hr,
                a.max_hr,
                a.average_running_cadence_in_steps_per_minute,
                a.max_running_cadence_in_steps_per_minute,
                a.average_speed,
                a.calories,
                a.aerobic_training_effect,
                a.anaerobic_training_effect,
                z[0],
                z[1],
                z[2],
                z[3],
                z[4],
                raw,
                a.elevation_gain,
                a.moving_duration,
                a.steps,
                a.has_polyline,
            ],
        )?;
        Ok(())
    }

    /// True when this activity is already cached — lets an incremental sync
    /// stop walking backwards through history.
    pub fn has_activity(&self, activity_id: i64) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT activity_id FROM activities WHERE activity_id = ?1",
                params![activity_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    pub fn activity_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM activities", [], |r| r.get(0))?)
    }

    /// Recent activities, newest first. `type_key` filters to one sport
    /// (`"running"`, `"treadmill_running"`, …); `None` returns everything.
    pub fn recent_activities(
        &self,
        limit: u32,
        type_key: Option<&str>,
    ) -> Result<Vec<CachedActivity>> {
        let sql = format!(
            "SELECT {ACTIVITY_COLS}
             FROM activities
             {}
             ORDER BY start_time_local DESC
             LIMIT ?1",
            if type_key.is_some() {
                "WHERE type_key LIKE '%' || ?2 || '%'"
            } else {
                ""
            }
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = match type_key {
            Some(t) => stmt
                .query_map(params![limit, t], map_activity)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            None => stmt
                .query_map(params![limit], map_activity)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        };
        Ok(rows)
    }

    /// Every activity on or after `from` (inclusive, `YYYY-MM-DD`), newest
    /// first. Used by the screens that reason over a window rather than a count
    /// — weekly reports, load ratios, correlations.
    pub fn activities_since(&self, from: &str) -> Result<Vec<CachedActivity>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {ACTIVITY_COLS} FROM activities
             WHERE local_date >= ?1
             ORDER BY start_time_local DESC"
        ))?;
        let rows = stmt
            .query_map(params![from], map_activity)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn activity(&self, activity_id: i64) -> Result<Option<CachedActivity>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {ACTIVITY_COLS} FROM activities WHERE activity_id = ?1"),
                params![activity_id],
                map_activity,
            )
            .optional()?)
    }

    pub fn upsert_daily(&self, d: &DailyMetrics) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO daily_metrics (
                date, resting_hr, hrv_last_night, hrv_weekly_avg, hrv_status,
                training_readiness, sleep_secs, sleep_score, steps, stress_avg,
                body_battery_high, body_battery_low, raw,
                consumed_kcal, total_burn_kcal, active_kcal, bmr_kcal,
                net_calorie_goal, hydration_ml, hydration_goal_ml, sweat_loss_ml
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,
                      ?14,?15,?16,?17,?18,?19,?20,?21)
            ON CONFLICT(date) DO UPDATE SET
                resting_hr=COALESCE(excluded.resting_hr, resting_hr),
                hrv_last_night=COALESCE(excluded.hrv_last_night, hrv_last_night),
                hrv_weekly_avg=COALESCE(excluded.hrv_weekly_avg, hrv_weekly_avg),
                hrv_status=COALESCE(excluded.hrv_status, hrv_status),
                training_readiness=COALESCE(excluded.training_readiness, training_readiness),
                sleep_secs=COALESCE(excluded.sleep_secs, sleep_secs),
                sleep_score=COALESCE(excluded.sleep_score, sleep_score),
                steps=COALESCE(excluded.steps, steps),
                stress_avg=COALESCE(excluded.stress_avg, stress_avg),
                body_battery_high=COALESCE(excluded.body_battery_high, body_battery_high),
                body_battery_low=COALESCE(excluded.body_battery_low, body_battery_low),
                raw=COALESCE(excluded.raw, raw),
                consumed_kcal=COALESCE(excluded.consumed_kcal, consumed_kcal),
                total_burn_kcal=COALESCE(excluded.total_burn_kcal, total_burn_kcal),
                active_kcal=COALESCE(excluded.active_kcal, active_kcal),
                bmr_kcal=COALESCE(excluded.bmr_kcal, bmr_kcal),
                net_calorie_goal=COALESCE(excluded.net_calorie_goal, net_calorie_goal),
                hydration_ml=COALESCE(excluded.hydration_ml, hydration_ml),
                hydration_goal_ml=COALESCE(excluded.hydration_goal_ml, hydration_goal_ml),
                sweat_loss_ml=COALESCE(excluded.sweat_loss_ml, sweat_loss_ml)
            "#,
            params![
                d.date,
                d.resting_hr,
                d.hrv_last_night,
                d.hrv_weekly_avg,
                d.hrv_status,
                d.training_readiness,
                d.sleep_secs,
                d.sleep_score,
                d.steps,
                d.stress_avg,
                d.body_battery_high,
                d.body_battery_low,
                d.raw,
                d.consumed_kcal,
                d.total_burn_kcal,
                d.active_kcal,
                d.bmr_kcal,
                d.net_calorie_goal,
                d.hydration_ml,
                d.hydration_goal_ml,
                d.sweat_loss_ml,
            ],
        )?;
        Ok(())
    }

    /// Daily metrics for the last `days` calendar days, newest first.
    pub fn recent_daily(&self, days: u32) -> Result<Vec<DailyMetrics>> {
        let mut stmt = self.conn.prepare(
            "SELECT date, resting_hr, hrv_last_night, hrv_weekly_avg, hrv_status,
                    training_readiness, sleep_secs, sleep_score, steps,
                    stress_avg, body_battery_high, body_battery_low,
                    consumed_kcal, total_burn_kcal, active_kcal, bmr_kcal,
                    net_calorie_goal, hydration_ml, hydration_goal_ml, sweat_loss_ml
             FROM daily_metrics ORDER BY date DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![days], |r| {
                Ok(DailyMetrics {
                    date: r.get(0)?,
                    resting_hr: r.get(1)?,
                    hrv_last_night: r.get(2)?,
                    hrv_weekly_avg: r.get(3)?,
                    hrv_status: r.get(4)?,
                    training_readiness: r.get(5)?,
                    sleep_secs: r.get(6)?,
                    sleep_score: r.get(7)?,
                    steps: r.get(8)?,
                    stress_avg: r.get(9)?,
                    body_battery_high: r.get(10)?,
                    body_battery_low: r.get(11)?,
                    consumed_kcal: r.get(12)?,
                    total_burn_kcal: r.get(13)?,
                    active_kcal: r.get(14)?,
                    bmr_kcal: r.get(15)?,
                    net_calorie_goal: r.get(16)?,
                    hydration_ml: r.get(17)?,
                    hydration_goal_ml: r.get(18)?,
                    sweat_loss_ml: r.get(19)?,
                    raw: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn upsert_workout(&self, w: &Workout) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO workouts (
                workout_id, name, sport_type, description,
                est_duration_s, est_distance_m, updated_at, raw
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
            ON CONFLICT(workout_id) DO UPDATE SET
                name=excluded.name, sport_type=excluded.sport_type,
                description=excluded.description,
                est_duration_s=excluded.est_duration_s,
                est_distance_m=excluded.est_distance_m,
                updated_at=excluded.updated_at, raw=excluded.raw
            "#,
            params![
                w.workout_id,
                w.name,
                w.sport_type,
                w.description,
                w.est_duration_s,
                w.est_distance_m,
                w.updated_at,
                w.raw,
            ],
        )?;
        Ok(())
    }

    pub fn workouts(&self) -> Result<Vec<Workout>> {
        let mut stmt = self.conn.prepare(
            "SELECT workout_id, name, sport_type, description,
                    est_duration_s, est_distance_m, updated_at
             FROM workouts ORDER BY updated_at DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Workout {
                    workout_id: r.get(0)?,
                    name: r.get(1)?,
                    sport_type: r.get(2)?,
                    description: r.get(3)?,
                    est_duration_s: r.get(4)?,
                    est_distance_m: r.get(5)?,
                    updated_at: r.get(6)?,
                    raw: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn upsert_track(&self, t: &ActivityTrack) -> Result<()> {
        let points = serde_json::to_string(&t.points).unwrap_or_else(|_| "[]".into());
        self.conn.execute(
            r#"
            INSERT INTO activity_tracks (
                activity_id, point_count, start_lat, start_lon, end_lat, end_lon,
                min_lat, max_lat, min_lon, max_lon, points
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
            ON CONFLICT(activity_id) DO UPDATE SET
                point_count=excluded.point_count,
                start_lat=excluded.start_lat, start_lon=excluded.start_lon,
                end_lat=excluded.end_lat, end_lon=excluded.end_lon,
                min_lat=excluded.min_lat, max_lat=excluded.max_lat,
                min_lon=excluded.min_lon, max_lon=excluded.max_lon,
                points=excluded.points
            "#,
            params![
                t.activity_id,
                t.point_count,
                t.start_lat,
                t.start_lon,
                t.end_lat,
                t.end_lon,
                t.min_lat,
                t.max_lat,
                t.min_lon,
                t.max_lon,
                points,
            ],
        )?;
        Ok(())
    }

    pub fn has_track(&self, activity_id: i64) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT activity_id FROM activity_tracks WHERE activity_id = ?1",
                params![activity_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// Every cached trace, joined to the summary fields the routes screen needs
    /// to label and compare them.
    ///
    /// `with_points` exists because grouping outings into routes only reads the
    /// endpoints and the distance — the coordinates are dead weight until
    /// something actually draws one, and there are hundreds of these.
    pub fn tracks(&self) -> Result<Vec<ActivityTrack>> {
        self.tracks_inner(true)
    }

    /// The same rows with `points` left empty. Cheap enough to call on every
    /// route computation.
    pub fn track_headers(&self) -> Result<Vec<ActivityTrack>> {
        self.tracks_inner(false)
    }

    fn tracks_inner(&self, with_points: bool) -> Result<Vec<ActivityTrack>> {
        // Selected as a literal empty array rather than omitted, so both shapes
        // share one row mapper and the column indices can't drift apart.
        let points_col = if with_points { "t.points" } else { "'[]'" };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT t.activity_id, t.point_count, t.start_lat, t.start_lon,
                    t.end_lat, t.end_lon, t.min_lat, t.max_lat, t.min_lon,
                    t.max_lon, {points_col},
                    a.name, a.type_key, a.local_date, a.distance_m, a.duration_s
             FROM activity_tracks t
             JOIN activities a USING (activity_id)
             ORDER BY a.start_time_local DESC"
        ))?;
        let rows = stmt
            .query_map([], |r| {
                let raw: String = r.get(10)?;
                Ok(ActivityTrack {
                    activity_id: r.get(0)?,
                    point_count: r.get(1)?,
                    start_lat: r.get(2)?,
                    start_lon: r.get(3)?,
                    end_lat: r.get(4)?,
                    end_lon: r.get(5)?,
                    min_lat: r.get(6)?,
                    max_lat: r.get(7)?,
                    min_lon: r.get(8)?,
                    max_lon: r.get(9)?,
                    points: serde_json::from_str(&raw).unwrap_or_default(),
                    name: r.get(11)?,
                    type_key: r.get(12)?,
                    local_date: r.get(13)?,
                    distance_m: r.get(14)?,
                    duration_s: r.get(15)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// One trace's coordinates. Empty when the activity has no cached track.
    pub fn track_points(&self, activity_id: i64) -> Result<Vec<[f64; 2]>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT points FROM activity_tracks WHERE activity_id = ?1",
                params![activity_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default())
    }

    pub fn track_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM activity_tracks", [], |r| r.get(0))?)
    }

    /// Activities Garmin flagged as having GPS but whose trace isn't cached yet.
    pub fn activities_missing_tracks(&self) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT activity_id FROM activities
             WHERE has_polyline = 1
               AND activity_id NOT IN (SELECT activity_id FROM activity_tracks)
             ORDER BY start_time_local DESC",
        )?;
        let rows = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn set_sync_state(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn sync_state(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM sync_state WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedActivity {
    pub activity_id: i64,
    pub name: Option<String>,
    pub type_key: Option<String>,
    pub start_time_local: Option<String>,
    pub local_date: Option<String>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub moving_duration_s: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub calories: Option<f64>,
    pub elevation_gain: Option<f64>,
    pub steps: Option<i64>,
    pub aerobic_te: Option<f64>,
    pub anaerobic_te: Option<f64>,
    /// Seconds in HR zones 1–5.
    pub zone_secs: [f64; 5],
}

impl CachedActivity {
    pub fn zone_total_secs(&self) -> f64 {
        self.zone_secs.iter().sum()
    }

    /// Percentage of tracked HR time spent in each zone. Returns zeros when the
    /// activity has no HR data at all, rather than dividing by zero.
    pub fn zone_percentages(&self) -> [f64; 5] {
        let total = self.zone_total_secs();
        if total <= 0.0 {
            return [0.0; 5];
        }
        self.zone_secs.map(|s| s / total * 100.0)
    }

    /// Pace in minutes per kilometre. `None` for activities without distance,
    /// which is most strength and jump-rope sessions.
    pub fn pace_min_per_km(&self) -> Option<f64> {
        let (d, t) = (self.distance_m?, self.duration_s?);
        if d < 1.0 {
            return None;
        }
        Some((t / 60.0) / (d / 1000.0))
    }
}

/// A structured session saved on the Garmin account.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workout {
    pub workout_id: i64,
    pub name: Option<String>,
    pub sport_type: Option<String>,
    pub description: Option<String>,
    pub est_duration_s: Option<f64>,
    pub est_distance_m: Option<f64>,
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

/// A GPS trace, downsampled for drawing, plus the summary fields needed to
/// label it. Only activities Garmin flags with `hasPolyline` have one.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityTrack {
    pub activity_id: i64,
    pub point_count: i64,
    pub start_lat: Option<f64>,
    pub start_lon: Option<f64>,
    pub end_lat: Option<f64>,
    pub end_lon: Option<f64>,
    pub min_lat: Option<f64>,
    pub max_lat: Option<f64>,
    pub min_lon: Option<f64>,
    pub max_lon: Option<f64>,
    /// `[[lat, lon], …]`, already thinned.
    pub points: Vec<[f64; 2]>,
    pub name: Option<String>,
    pub type_key: Option<String>,
    pub local_date: Option<String>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyMetrics {
    pub date: String,
    pub resting_hr: Option<f64>,
    pub hrv_last_night: Option<f64>,
    pub hrv_weekly_avg: Option<f64>,
    pub hrv_status: Option<String>,
    pub training_readiness: Option<f64>,
    pub sleep_secs: Option<f64>,
    pub sleep_score: Option<f64>,
    pub steps: Option<i64>,
    pub stress_avg: Option<f64>,
    pub body_battery_high: Option<f64>,
    pub body_battery_low: Option<f64>,
    /// Calories eaten, per whatever food log feeds Garmin. `None` on days the
    /// athlete logged nothing — which is not the same as a zero-calorie day,
    /// so the screens have to keep the two apart.
    pub consumed_kcal: Option<f64>,
    /// Everything burned that day: `active_kcal` + `bmr_kcal`.
    pub total_burn_kcal: Option<f64>,
    pub active_kcal: Option<f64>,
    pub bmr_kcal: Option<f64>,
    pub net_calorie_goal: Option<f64>,
    pub hydration_ml: Option<f64>,
    pub hydration_goal_ml: Option<f64>,
    pub sweat_loss_ml: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

impl DailyMetrics {
    /// Eaten minus burned. Negative is a deficit. `None` unless both sides are
    /// known — a deficit computed against an empty food log is fiction.
    pub fn calorie_balance(&self) -> Option<f64> {
        Some(self.consumed_kcal? - self.total_burn_kcal?)
    }
}
