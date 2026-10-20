//! Saved courses: the routes that the athlete planned in Garmin Connect.
//!
//! The list carries the figures. The shape of a course needs one more request
//! for each course, so the sync asks for it once for a new course and again
//! only when Garmin says the course changed.

use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;

pub(crate) const SCHEMA: &str = r#"
    -- A course the athlete saved. `points` is the simplified track as a JSON
    -- list of [lat, lon], the same pairs as `activity_tracks`, or null while
    -- the detail is not fetched. `detail_for` is the `updated_ms` that the
    -- points belong to, so a changed course is fetched again.
    CREATE TABLE IF NOT EXISTS courses (
        course_id        INTEGER PRIMARY KEY,
        name             TEXT,
        activity_type    TEXT,
        distance_m       REAL,
        elevation_gain_m REAL,
        created_ms       INTEGER,
        updated_ms       INTEGER,
        favorite         INTEGER NOT NULL DEFAULT 0,
        start_lat        REAL,
        start_lon        REAL,
        points           TEXT,
        detail_for       INTEGER
    );
"#;

/// The most points that a stored course keeps. A thumbnail needs a recognisable
/// shape, not a survey.
pub const COURSE_POINTS: usize = 120;

/// A course as the list sends it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseSummary {
    pub course_id: i64,
    pub name: Option<String>,
    pub activity_type: Option<String>,
    pub distance_m: Option<f64>,
    pub elevation_gain_m: Option<f64>,
    /// Milliseconds since the epoch.
    pub created_ms: Option<i64>,
    pub updated_ms: Option<i64>,
    pub favorite: bool,
    pub start_lat: Option<f64>,
    pub start_lon: Option<f64>,
}

/// A stored course with its simplified track.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    #[serde(flatten)]
    pub summary: CourseSummary,
    /// `[lat, lon]` pairs, at most [`COURSE_POINTS`]. Empty until fetched.
    pub points: Vec<[f64; 2]>,
}

/// The courses in a list reply. The reply is a list. An object that holds the
/// list under `coursesForUser` is read as well. An item with no id is skipped.
pub fn parse_course_list(v: &Value) -> Vec<CourseSummary> {
    let list = v.as_array().or_else(|| v.get("coursesForUser")?.as_array());
    let Some(list) = list else {
        return Vec::new();
    };
    list.iter().filter_map(parse_course).collect()
}

fn parse_course(c: &Value) -> Option<CourseSummary> {
    let f = |k: &str| c.get(k).and_then(Value::as_f64);
    Some(CourseSummary {
        course_id: c.get("courseId")?.as_i64()?,
        name: c
            .get("courseName")
            .and_then(Value::as_str)
            .map(str::to_owned),
        activity_type: c
            .pointer("/activityType/typeKey")
            .and_then(Value::as_str)
            .map(str::to_owned),
        distance_m: f("distanceInMeters"),
        elevation_gain_m: f("elevationGainInMeters"),
        created_ms: c.get("createdDate").and_then(Value::as_i64),
        updated_ms: c.get("updatedDate").and_then(Value::as_i64),
        favorite: c.get("favorite").and_then(Value::as_bool).unwrap_or(false),
        start_lat: f("startLatitude"),
        start_lon: f("startLongitude"),
    })
}

/// The simplified track in a course detail reply.
pub fn parse_course_track(v: &Value) -> Vec<[f64; 2]> {
    let Some(list) = v.get("geoPoints").and_then(Value::as_array) else {
        return Vec::new();
    };
    let points: Vec<[f64; 2]> = list
        .iter()
        .filter_map(|p| {
            let lat = p.get("latitude")?.as_f64()?;
            let lon = p.get("longitude")?.as_f64()?;
            (lat.is_finite() && lon.is_finite()).then_some([lat, lon])
        })
        .collect();
    thin(&points, COURSE_POINTS)
}

/// Keep at most `max` points, spread evenly, with the first and the last.
fn thin(points: &[[f64; 2]], max: usize) -> Vec<[f64; 2]> {
    if points.len() <= max || max < 2 {
        return points.to_vec();
    }
    let last = points.len() - 1;
    (0..max).map(|i| points[i * last / (max - 1)]).collect()
}

impl Db {
    /// Write the figures of a course. The stored track stays, and is marked
    /// stale when `updated_ms` differs from the one that the track belongs to.
    pub fn upsert_course(&self, c: &CourseSummary) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO courses (
                course_id, name, activity_type, distance_m, elevation_gain_m,
                created_ms, updated_ms, favorite, start_lat, start_lon
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
            ON CONFLICT(course_id) DO UPDATE SET
                name=excluded.name, activity_type=excluded.activity_type,
                distance_m=excluded.distance_m,
                elevation_gain_m=excluded.elevation_gain_m,
                created_ms=excluded.created_ms, updated_ms=excluded.updated_ms,
                favorite=excluded.favorite, start_lat=excluded.start_lat,
                start_lon=excluded.start_lon
            "#,
            params![
                c.course_id,
                c.name,
                c.activity_type,
                c.distance_m,
                c.elevation_gain_m,
                c.created_ms,
                c.updated_ms,
                c.favorite,
                c.start_lat,
                c.start_lon,
            ],
        )?;
        Ok(())
    }

    /// Remove the courses that the list no longer holds.
    pub fn retain_courses(&self, keep: &[i64]) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT course_id FROM courses")?;
        let have: Vec<i64> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for id in have.into_iter().filter(|id| !keep.contains(id)) {
            self.conn
                .execute("DELETE FROM courses WHERE course_id = ?1", params![id])?;
        }
        Ok(())
    }

    /// Store the track of a course, for the `updated_ms` that it was read at.
    pub fn save_course_track(&self, course_id: i64, points: &[[f64; 2]]) -> Result<()> {
        self.conn.execute(
            "UPDATE courses SET points = ?2, detail_for = updated_ms WHERE course_id = ?1",
            params![course_id, serde_json::to_string(points)?],
        )?;
        Ok(())
    }

    /// The courses with no track, or with a track older than the course, newest
    /// first and at most `limit`.
    pub fn courses_missing_track(&self, limit: usize) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT course_id FROM courses
             WHERE points IS NULL OR detail_for IS NOT updated_ms
             ORDER BY created_ms DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Every stored course, the favourites first and then the newest.
    pub fn courses(&self) -> Result<Vec<Course>> {
        let mut stmt = self.conn.prepare(
            "SELECT course_id, name, activity_type, distance_m, elevation_gain_m,
                    created_ms, updated_ms, favorite, start_lat, start_lon, points
             FROM courses ORDER BY favorite DESC, created_ms DESC, course_id DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let raw: Option<String> = r.get(10)?;
                Ok(Course {
                    summary: CourseSummary {
                        course_id: r.get(0)?,
                        name: r.get(1)?,
                        activity_type: r.get(2)?,
                        distance_m: r.get(3)?,
                        elevation_gain_m: r.get(4)?,
                        created_ms: r.get(5)?,
                        updated_ms: r.get(6)?,
                        favorite: r.get(7)?,
                        start_lat: r.get(8)?,
                        start_lon: r.get(9)?,
                    },
                    points: raw
                        .and_then(|j| serde_json::from_str(&j).ok())
                        .unwrap_or_default(),
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

    fn listed(id: i64, updated: i64) -> Value {
        json!({
            "courseId": id, "courseName": "Loop", "activityType": { "typeKey": "running" },
            "distanceInMeters": 5000.0, "elevationGainInMeters": 40.0,
            "createdDate": 1000, "updatedDate": updated, "favorite": id == 1,
            "startLatitude": 10.0, "startLongitude": 20.0,
        })
    }

    #[test]
    fn an_odd_item_does_not_drop_the_list() {
        let v = json!([listed(1, 5), { "courseName": "No id" }, "junk", { "courseId": 2 }]);
        let got = parse_course_list(&v);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].activity_type.as_deref(), Some("running"));
        assert!(got[0].favorite && !got[1].favorite);
        assert_eq!(got[1].name, None);
        assert!(parse_course_list(&json!({ "coursesForUser": [listed(3, 1)] })).len() == 1);
        assert!(parse_course_list(&json!(null)).is_empty());
    }

    #[test]
    fn a_long_track_is_thinned_to_the_limit_and_keeps_both_ends() {
        let points: Vec<Value> = (0..1000)
            .map(|i| json!({ "latitude": i as f64, "longitude": 0.5 }))
            .chain([json!({ "latitude": null, "longitude": 1.0 })])
            .collect();
        let got = parse_course_track(&json!({ "geoPoints": points }));
        assert_eq!(got.len(), COURSE_POINTS);
        assert_eq!(got[0][0], 0.0);
        assert_eq!(got[COURSE_POINTS - 1][0], 999.0);
        assert_eq!(thin(&[[1.0, 1.0]], 120).len(), 1);
        assert!(parse_course_track(&json!({})).is_empty());
    }

    #[test]
    fn a_track_is_fetched_once_and_again_only_for_a_changed_course() {
        let db = crate::db::scratch_db("courses");
        let course = |id, updated| parse_course_list(&json!([listed(id, updated)])).remove(0);
        db.upsert_course(&course(1, 5)).unwrap();
        db.upsert_course(&course(2, 5)).unwrap();
        assert_eq!(db.courses_missing_track(10).unwrap().len(), 2);
        db.save_course_track(1, &[[1.0, 2.0], [3.0, 4.0]]).unwrap();
        db.save_course_track(2, &[]).unwrap();
        assert!(db.courses_missing_track(10).unwrap().is_empty());

        db.upsert_course(&course(1, 9)).unwrap();
        assert_eq!(db.courses_missing_track(10).unwrap(), vec![1]);
        let all = db.courses().unwrap();
        assert_eq!(all[0].summary.course_id, 1, "the favourite is first");
        assert_eq!(
            all[0].points.len(),
            2,
            "the old track stays until the new one"
        );

        db.retain_courses(&[2]).unwrap();
        assert_eq!(db.courses().unwrap().len(), 1);
    }
}
