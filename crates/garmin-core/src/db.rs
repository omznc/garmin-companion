//! Local SQLite cache.
//!
//! Everything the app and the MCP server read comes from here, never straight
//! from Garmin. That keeps queries instant, keeps history around after Garmin
//! inevitably changes something, and means an LLM tool call can't stall on a
//! network round trip.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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

/// Ceilings on tagging. Neither is a data-integrity rule — they exist so that a
/// paste accident can't put a paragraph, or four hundred labels, into a column
/// the activity screen renders as chips.
const MAX_TAG_CHARS: usize = 32;
const MAX_TAGS_PER_ACTIVITY: usize = 12;

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

fn map_weigh_in(r: &rusqlite::Row) -> rusqlite::Result<WeighIn> {
    Ok(WeighIn {
        sample_pk: r.get(0)?,
        calendar_date: r.get(1)?,
        weight_g: r.get(2)?,
        bmi: r.get(3)?,
        body_fat: r.get(4)?,
        body_water: r.get(5)?,
        bone_mass: r.get(6)?,
        muscle_mass: r.get(7)?,
        source_type: r.get(8)?,
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

            -- Saved Ask conversations. Kept in the cache rather than in
            -- localStorage because they're a record of your own history, and
            -- the frontend's storage is disposable by design.
            CREATE TABLE IF NOT EXISTS chat_sessions (
                session_id  TEXT PRIMARY KEY,
                started_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                -- First question asked, so a list can be read without loading
                -- every message body.
                title       TEXT NOT NULL,
                message_count INTEGER NOT NULL DEFAULT 0,
                -- The whole conversation as JSON. These are a few KB each and
                -- are only ever read whole.
                messages    TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated
                ON chat_sessions(updated_at DESC);

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

            -- Body weight. Its own table rather than a column on
            -- `daily_metrics` because weigh-ins don't line up with days: most
            -- days have none, and a day can have two if one was a correction.
            -- Keyed by Garmin's `samplePk` so a re-weighed day updates its
            -- entry instead of adding a second one.
            CREATE TABLE IF NOT EXISTS weigh_ins (
                sample_pk     INTEGER PRIMARY KEY,
                calendar_date TEXT NOT NULL,
                weight_g      REAL NOT NULL,
                -- Body composition. Populated only by a smart scale; a phone
                -- app or a hand-typed entry leaves every one of these null.
                bmi           REAL,
                body_fat      REAL,
                body_water    REAL,
                bone_mass     REAL,
                muscle_mass   REAL,
                source_type   TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_weigh_ins_date
                ON weigh_ins(calendar_date DESC);

            -- Labels the athlete puts on their own sessions. Garmin has no tag
            -- concept to sync with, so these are local and stay local; they
            -- exist so a question like "how do my tempo sessions compare" has
            -- something to mean.
            --
            -- One row per tag rather than a delimited column: the interesting
            -- read is "every activity tagged X", and that shouldn't be a LIKE
            -- over a joined string that matches 'tempo' inside 'tempo-fail'.
            CREATE TABLE IF NOT EXISTS activity_tags (
                activity_id INTEGER NOT NULL,
                tag         TEXT NOT NULL,
                PRIMARY KEY (activity_id, tag)
            );
            CREATE INDEX IF NOT EXISTS idx_activity_tags_tag
                ON activity_tags(tag);

            -- The computed analysis for one session, kept because building it
            -- costs three Garmin requests and the samples behind a finished
            -- session never change. `key` is what it was computed from, so a
            -- re-sync that corrects a duration — or a newly written tag —
            -- invalidates it and merely reopening the screen does not.
            CREATE TABLE IF NOT EXISTS activity_analysis (
                activity_id INTEGER PRIMARY KEY,
                key         TEXT NOT NULL,
                computed_at TEXT NOT NULL,
                json        TEXT NOT NULL
            );

            -- The written critique of one session, kept under its original
            -- table name. Separate from the analysis above because it is
            -- invalidated by different things: the analysis survives a model
            -- change, the prose does not survive the numbers moving underneath
            -- it — or the prompt that wrote it changing, which `key` carries.
            CREATE TABLE IF NOT EXISTS activity_summaries (
                activity_id  INTEGER PRIMARY KEY,
                key          TEXT NOT NULL,
                generated_at TEXT NOT NULL,
                text         TEXT NOT NULL
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

    /// The local start time of the oldest cached activity, `YYYY-MM-DD…`.
    ///
    /// A full sync uses this to decide how far back the wellness walk has to
    /// go, rather than trusting a fixed number of days to cover everything.
    pub fn earliest_activity_date(&self) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT MIN(start_time_local) FROM activities", [], |r| {
                r.get::<_, Option<String>>(0)
            })?)
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

    /// The dates on or after `from` that already have a row.
    ///
    /// A row is only written for a day that came back with something in it, so
    /// a date missing from this set is one the cache has never had data for —
    /// either it was never fetched, or it was fetched before the watch had
    /// uploaded it. Both are worth asking about again.
    pub fn daily_dates_since(&self, from: &str) -> Result<HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT date FROM daily_metrics WHERE date >= ?1")?;
        let rows = stmt
            .query_map(params![from], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
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

    pub fn upsert_weigh_in(&self, w: &WeighIn) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO weigh_ins (
                sample_pk, calendar_date, weight_g,
                bmi, body_fat, body_water, bone_mass, muscle_mass, source_type
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
            ON CONFLICT(sample_pk) DO UPDATE SET
                calendar_date=excluded.calendar_date, weight_g=excluded.weight_g,
                bmi=excluded.bmi, body_fat=excluded.body_fat,
                body_water=excluded.body_water, bone_mass=excluded.bone_mass,
                muscle_mass=excluded.muscle_mass, source_type=excluded.source_type
            "#,
            params![
                w.sample_pk,
                w.calendar_date,
                w.weight_g,
                w.bmi,
                w.body_fat,
                w.body_water,
                w.bone_mass,
                w.muscle_mass,
                w.source_type,
            ],
        )?;
        Ok(())
    }

    /// Weigh-ins on or after `from`, oldest first.
    ///
    /// Ordered ascending because everything downstream — the smoothed trend,
    /// the rate of change, the chart — reads left to right in time, and one
    /// reversal here saves every caller doing its own.
    ///
    /// Days with two entries keep both. Which one to believe is a judgement the
    /// query layer makes; the cache's job is to not lose either.
    pub fn weigh_ins_since(&self, from: &str) -> Result<Vec<WeighIn>> {
        let mut stmt = self.conn.prepare(
            "SELECT sample_pk, calendar_date, weight_g, bmi, body_fat,
                    body_water, bone_mass, muscle_mass, source_type
             FROM weigh_ins WHERE calendar_date >= ?1
             ORDER BY calendar_date ASC, sample_pk ASC",
        )?;
        let rows = stmt
            .query_map(params![from], map_weigh_in)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The most recent weigh-in of all, however far back it is.
    ///
    /// Separate from `weigh_ins_since` so a screen can say "last weighed in
    /// four months ago" rather than showing nothing because the window it asked
    /// for happened to be empty.
    pub fn latest_weigh_in(&self) -> Result<Option<WeighIn>> {
        Ok(self
            .conn
            .query_row(
                "SELECT sample_pk, calendar_date, weight_g, bmi, body_fat,
                        body_water, bone_mass, muscle_mass, source_type
                 FROM weigh_ins ORDER BY calendar_date DESC, sample_pk DESC LIMIT 1",
                [],
                map_weigh_in,
            )
            .optional()?)
    }

    pub fn weigh_in_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM weigh_ins", [], |r| r.get(0))?)
    }

    /* ---------------------------------------------------------------- tags --- */

    /// The tags on one activity, alphabetically.
    pub fn activity_tags(&self, activity_id: i64) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag FROM activity_tags WHERE activity_id = ?1 ORDER BY tag")?;
        let rows = stmt.query_map(params![activity_id], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<String>, _>>()?)
    }

    /// Replace the whole tag set for one activity.
    ///
    /// Set-at-a-time rather than add/remove: the editor on the activity screen
    /// holds the complete list either way, and two calls to keep in step is one
    /// more chance for a half-applied edit.
    ///
    /// Tags are trimmed, lowercased and deduplicated on the way in. A tag is a
    /// label for grouping, and `Tempo`, `tempo ` and `tempo` being three
    /// different groups is a bug people find by accident a month later.
    pub fn set_activity_tags(&self, activity_id: i64, tags: &[String]) -> Result<Vec<String>> {
        let mut clean: Vec<String> = tags
            .iter()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty() && t.chars().count() <= MAX_TAG_CHARS)
            .collect();
        clean.sort();
        clean.dedup();
        clean.truncate(MAX_TAGS_PER_ACTIVITY);

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM activity_tags WHERE activity_id = ?1",
            params![activity_id],
        )?;
        {
            let mut stmt =
                tx.prepare("INSERT INTO activity_tags (activity_id, tag) VALUES (?1, ?2)")?;
            for tag in &clean {
                stmt.execute(params![activity_id, tag])?;
            }
        }
        tx.commit()?;
        Ok(clean)
    }

    /// Every tag in use, with how many activities carry it, commonest first.
    pub fn all_tags(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT tag, COUNT(*) AS n FROM activity_tags
             GROUP BY tag ORDER BY n DESC, tag",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Activities carrying a tag, newest first.
    pub fn activities_with_tag(&self, tag: &str, limit: u32) -> Result<Vec<CachedActivity>> {
        // A subquery rather than a join: `ACTIVITY_COLS` names its columns
        // unqualified, and a join would make `activity_id` ambiguous.
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {ACTIVITY_COLS} FROM activities
             WHERE activity_id IN (SELECT activity_id FROM activity_tags WHERE tag = ?1)
             ORDER BY start_time_local DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![tag.trim().to_lowercase(), limit], map_activity)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Tags for several activities at once, so a list screen isn't one query
    /// per row.
    pub fn tags_for(&self, activity_ids: &[i64]) -> Result<HashMap<i64, Vec<String>>> {
        let mut out: HashMap<i64, Vec<String>> = HashMap::new();
        if activity_ids.is_empty() {
            return Ok(out);
        }
        let mut stmt = self
            .conn
            .prepare("SELECT activity_id, tag FROM activity_tags ORDER BY activity_id, tag")?;
        let wanted: std::collections::HashSet<i64> = activity_ids.iter().copied().collect();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, tag) = row?;
            if wanted.contains(&id) {
                out.entry(id).or_default().push(tag);
            }
        }
        Ok(out)
    }

    /* ------------------------------------------------------------ analysis --- */

    /// The stored analysis for one activity, if it was computed from `key`.
    ///
    /// A mismatched key returns `None` rather than the stale row: the caller
    /// wants an analysis of the data as it is now, and being handed one built
    /// from an earlier version of it would be worse than recomputing.
    pub fn activity_analysis(&self, activity_id: i64, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT json FROM activity_analysis WHERE activity_id = ?1 AND key = ?2",
                params![activity_id, key],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn save_activity_analysis(
        &self,
        activity_id: i64,
        key: &str,
        computed_at: &str,
        json: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO activity_analysis (activity_id, key, computed_at, json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(activity_id) DO UPDATE SET
                 key = excluded.key,
                 computed_at = excluded.computed_at,
                 json = excluded.json",
            params![activity_id, key, computed_at, json],
        )?;
        Ok(())
    }

    /// The written critique of one activity: `(text, generated_at)`, and only
    /// when it was written about `key`.
    pub fn activity_critique(
        &self,
        activity_id: i64,
        key: &str,
    ) -> Result<Option<(String, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT text, generated_at FROM activity_summaries
                 WHERE activity_id = ?1 AND key = ?2",
                params![activity_id, key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn save_activity_critique(
        &self,
        activity_id: i64,
        key: &str,
        generated_at: &str,
        text: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO activity_summaries (activity_id, key, generated_at, text)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(activity_id) DO UPDATE SET
                 key = excluded.key,
                 generated_at = excluded.generated_at,
                 text = excluded.text",
            params![activity_id, key, generated_at, text],
        )?;
        Ok(())
    }

    pub fn set_sync_state(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Insert or replace a saved conversation.
    pub fn save_chat_session(&self, s: &ChatSession) -> Result<()> {
        self.conn.execute(
            "INSERT INTO chat_sessions
                 (session_id, started_at, updated_at, title, message_count, messages)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id) DO UPDATE SET
                 updated_at = excluded.updated_at,
                 title = excluded.title,
                 message_count = excluded.message_count,
                 messages = excluded.messages",
            params![
                s.session_id,
                s.started_at,
                s.updated_at,
                s.title,
                s.message_count,
                s.messages,
            ],
        )?;
        Ok(())
    }

    /// A page of saved conversations, newest first, without their bodies —
    /// the list only needs titles, and the bodies are the expensive part.
    pub fn chat_sessions(&self, limit: u32, offset: u32) -> Result<Vec<ChatSessionMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, started_at, updated_at, title, message_count
             FROM chat_sessions
             ORDER BY updated_at DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt
            .query_map(params![limit, offset], |r| {
                Ok(ChatSessionMeta {
                    session_id: r.get(0)?,
                    started_at: r.get(1)?,
                    updated_at: r.get(2)?,
                    title: r.get(3)?,
                    message_count: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// One conversation, bodies and all.
    pub fn chat_session(&self, session_id: &str) -> Result<Option<ChatSession>> {
        Ok(self
            .conn
            .query_row(
                "SELECT session_id, started_at, updated_at, title, message_count, messages
                 FROM chat_sessions WHERE session_id = ?1",
                params![session_id],
                |r| {
                    Ok(ChatSession {
                        session_id: r.get(0)?,
                        started_at: r.get(1)?,
                        updated_at: r.get(2)?,
                        title: r.get(3)?,
                        message_count: r.get(4)?,
                        messages: r.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn delete_chat_session(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM chat_sessions WHERE session_id = ?1",
            params![session_id],
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

/// One weigh-in, as Garmin holds it.
///
/// Grams, not kilograms, because that is the unit Garmin sends and converting
/// at the boundary is one more place for a factor of a thousand to go wrong.
/// The query layer converts once, on the way out.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeighIn {
    pub sample_pk: i64,
    pub calendar_date: String,
    pub weight_g: f64,
    pub bmi: Option<f64>,
    pub body_fat: Option<f64>,
    pub body_water: Option<f64>,
    pub bone_mass: Option<f64>,
    pub muscle_mass: Option<f64>,
    pub source_type: Option<String>,
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

    /// Whether any figure was actually recorded for this day.
    ///
    /// Garmin answers 200 for dates before you owned the watch, with every
    /// field null — so a successful request is no evidence that a day happened,
    /// and the sync needs to be able to tell the difference.
    pub fn has_data(&self) -> bool {
        self.resting_hr.is_some()
            || self.hrv_last_night.is_some()
            || self.hrv_weekly_avg.is_some()
            || self.training_readiness.is_some()
            || self.sleep_secs.is_some()
            || self.sleep_score.is_some()
            || self.steps.is_some()
            || self.stress_avg.is_some()
            || self.body_battery_high.is_some()
            || self.body_battery_low.is_some()
            || self.consumed_kcal.is_some()
            || self.total_burn_kcal.is_some()
            || self.active_kcal.is_some()
            || self.bmr_kcal.is_some()
            || self.hydration_ml.is_some()
            || self.sweat_loss_ml.is_some()
    }
}

/// A saved Ask conversation. `messages` is opaque JSON here — the shape is the
/// frontend's `ChatMessage[]`, and the cache has no reason to care what's in
/// it beyond storing and returning it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
    pub session_id: String,
    pub started_at: String,
    pub updated_at: String,
    pub title: String,
    pub message_count: i64,
    pub messages: String,
}

/// A conversation's headline, for listing without loading bodies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionMeta {
    pub session_id: String,
    pub started_at: String,
    pub updated_at: String,
    pub title: String,
    pub message_count: i64,
}
