//! The Routes screen as a view model.
//!
//! A route is a group of outings that start and finish in the same place over a
//! similar distance. The grouping lives in `garmin_core::query`. This module adds
//! the sentences and a drawing-ready shape for each route.
//! The code reads the cache. It never calls Garmin.

use crate::body::EmptyNote;
use crate::places::{km_text, Tile};
use crate::text::{hours_minutes, short_date};
use crate::ApiError;
use garmin_core::query::{self, Route};
use garmin_core::sport::{is_motorised, sport_label};
use garmin_core::Db;
use serde::{Deserialize, Serialize};

pub use garmin_core::query::RouteSort;

/// The most routes that one view carries.
const ROUTE_LIMIT: usize = 60;
/// The most points in the shape of one route. More is sub-pixel on a thumbnail.
const SHAPE_POINTS: usize = 90;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OutingRow {
    pub activity_id: i64,
    pub date: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteRow {
    pub name: String,
    pub sport: String,
    pub distance: Option<String>,
    pub times: usize,
    /// `once` or `5 times`.
    pub times_line: String,
    /// The shape in a unit square, north up, with the true proportions.
    /// Empty when the route has no drawn trace.
    pub trace: Vec<[f32; 2]>,
    /// Listed only when the route repeats. One outing would repeat the card.
    pub outings: Vec<OutingRow>,
    /// The fastest outing on the route. `None` when no outing has a time.
    #[serde(default)]
    pub best_time: Option<BestTime>,
    /// Metres climbed, as the mean over the outings that have it.
    #[serde(default)]
    pub elevation_m: Option<f64>,
    /// The mean distance of one outing, in metres. 0 when unknown.
    #[serde(default)]
    pub distance_m: f64,
    /// The distance of all outings together, in metres. 0 when unknown.
    #[serde(default)]
    pub total_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BestTime {
    pub activity_id: i64,
    pub duration_s: f64,
    /// For example `42 min` or `1h 05m`.
    pub text: String,
    pub date: String,
}

/// The longest route by its mean distance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct LongestRoute {
    pub name: String,
    pub distance_m: f64,
    pub distance: String,
}

/// A course that the athlete saved in Garmin Connect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CourseRow {
    pub course_id: i64,
    pub name: String,
    pub sport: String,
    pub distance: Option<String>,
    pub elevation_m: Option<f64>,
    pub favorite: bool,
    /// The shape in a unit square, as for a route. Empty until the track is
    /// fetched.
    pub trace: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RoutesView {
    pub empty: Option<EmptyNote>,
    pub sort: RouteSort,
    pub tiles: Vec<Tile>,
    pub notes: Vec<String>,
    pub total: usize,
    pub routes: Vec<RouteRow>,
    #[serde(default)]
    pub longest: Option<LongestRoute>,
    #[serde(default)]
    pub courses: Vec<CourseRow>,
}

/// The smallest span of a drawn shape, in degrees.
const MIN_SPAN_DEG: f64 = 0.0008;

/// Average each point with its two neighbours. The ends stay. This removes
/// the small jumps of a GPS fix and keeps the shape of the route.
fn smooth(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = points.to_vec();
    // A short trace is a hand-made shape with corners. Leave it as it is.
    if points.len() < 12 {
        return out;
    }
    for i in 1..points.len().saturating_sub(1) {
        for axis in 0..2 {
            out[i][axis] = (points[i - 1][axis] + points[i][axis] + points[i + 1][axis]) / 3.0;
        }
    }
    out
}

/// Scale a trace into the unit square. Longitude shrinks by cos(latitude), so
/// the shape keeps the proportions that it has on the ground.
pub fn shape(points: &[[f64; 2]]) -> Vec<[f32; 2]> {
    // A NaN or infinite point would reach the screens as null or as a broken path.
    let points: Vec<[f64; 2]> = points
        .iter()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .copied()
        .collect();
    if points.len() < 2 {
        return Vec::new();
    }
    let step = points.len().div_ceil(SHAPE_POINTS);
    let mut kept: Vec<[f64; 2]> = points.iter().step_by(step).copied().collect();
    if let Some(last) = points.last() {
        kept.push(*last);
    }
    let kept = smooth(&kept);
    let fold = |pick: fn(&[f64; 2]) -> f64| {
        kept.iter()
            .map(pick)
            .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
    };
    let ((min_lat, max_lat), (min_lon, max_lon)) = (fold(|p| p[0]), fold(|p| p[1]));
    let k = ((min_lat + max_lat) / 2.0).to_radians().cos();
    let span_lat = (max_lat - min_lat).max(1e-6);
    let span_lon = ((max_lon - min_lon) * k).max(1e-6);
    let span = span_lat.max(span_lon);
    // A trace that never moved has no shape to draw. A span under about 90 m
    // is GPS jitter around one place, so it would draw as spikes.
    if span <= MIN_SPAN_DEG {
        return Vec::new();
    }
    kept.iter()
        .map(|p| {
            let x = ((p[1] - min_lon) * k + (span - span_lon) / 2.0) / span;
            let y = ((max_lat - p[0]) + (span - span_lat) / 2.0) / span;
            [x as f32, y as f32]
        })
        .collect()
}

fn row_of(r: &Route) -> RouteRow {
    let first = &r.outings[0];
    RouteRow {
        name: first.name.clone().unwrap_or_else(|| "Untitled".into()),
        sport: sport_label(r.type_key.as_deref()),
        distance: r.avg_distance_m.filter(|m| *m > 0.0).map(km_text),
        times: r.times,
        times_line: if r.times == 1 {
            "once".into()
        } else {
            format!("{} times", r.times)
        },
        trace: shape(&first.points),
        outings: if r.times > 1 {
            r.outings.iter().map(outing_of).collect()
        } else {
            Vec::new()
        },
        best_time: best_time(&r.outings),
        elevation_m: mean_elevation(&r.outings),
        distance_m: r.avg_distance_m.unwrap_or(0.0),
        total_m: r.avg_distance_m.unwrap_or(0.0) * r.times as f64,
    }
}

/// The outing with the shortest positive time. A tie goes to the older one.
pub fn best_time(outings: &[query::RouteOuting]) -> Option<BestTime> {
    outings
        .iter()
        .filter(|o| o.duration_s.is_some_and(|s| s > 0.0))
        .min_by(|a, b| {
            a.duration_s
                .partial_cmp(&b.duration_s)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.local_date.cmp(&b.local_date))
        })
        .map(|o| {
            let secs = o.duration_s.unwrap_or(0.0);
            BestTime {
                activity_id: o.activity_id,
                duration_s: secs,
                text: crate::reports::duration(secs),
                date: o.local_date.clone().unwrap_or_default(),
            }
        })
}

/// The mean climb over the outings that have one above zero.
pub fn mean_elevation(outings: &[query::RouteOuting]) -> Option<f64> {
    let climbs: Vec<f64> = outings
        .iter()
        .filter_map(|o| o.elevation_gain_m.filter(|m| *m > 0.0))
        .collect();
    (!climbs.is_empty()).then(|| climbs.iter().sum::<f64>() / climbs.len() as f64)
}

/// The route with the greatest mean distance.
pub fn longest_route(routes: &[Route]) -> Option<LongestRoute> {
    routes
        .iter()
        .filter_map(|r| Some((r, r.avg_distance_m.filter(|m| *m > 0.0)?)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(r, m)| LongestRoute {
            name: r.outings[0]
                .name
                .clone()
                .unwrap_or_else(|| "Untitled".into()),
            distance_m: m,
            distance: km_text(m),
        })
}

/// The saved courses, with a drawn shape for each one that has a track.
fn course_rows(db: &Db) -> Result<Vec<CourseRow>, ApiError> {
    Ok(db
        .courses()?
        .into_iter()
        .map(|c| CourseRow {
            course_id: c.summary.course_id,
            name: c.summary.name.unwrap_or_else(|| "Untitled".into()),
            sport: sport_label(c.summary.activity_type.as_deref()),
            distance: c.summary.distance_m.filter(|m| *m > 0.0).map(km_text),
            elevation_m: c.summary.elevation_gain_m.filter(|m| *m > 0.0),
            favorite: c.summary.favorite,
            trace: shape(&c.points),
        })
        .collect())
}

fn outing_of(o: &query::RouteOuting) -> OutingRow {
    let mut parts = Vec::new();
    parts.extend(o.distance_m.filter(|m| *m > 0.0).map(km_text));
    parts.extend(o.duration_s.filter(|s| *s > 0.0).map(hours_minutes));
    OutingRow {
        activity_id: o.activity_id,
        date: o
            .local_date
            .as_deref()
            .and_then(short_date)
            .unwrap_or_else(|| "—".into()),
        detail: parts.join(" · "),
    }
}

/// `a`, `a and b`, `a, b and c`.
fn list_of(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}

fn empty_note() -> EmptyNote {
    EmptyNote {
        title: "No routes yet.".into(),
        body: "A route comes from the GPS track of an outdoor activity, such as a walk, a ride, \
               a hike or a round of golf. No activity in the cache has a track yet. \
               Run a sync from Settings. Any outdoor activity that your watch recorded with GPS \
               then appears here. An indoor session has no position, so it never appears."
            .into(),
    }
}

/// Tracks exist, but a motor made every one of them.
fn motor_only_note() -> EmptyNote {
    EmptyNote {
        title: "No routes to show.".into(),
        body: "Routes shows walks, runs, rides and other workouts. Motor sports are not shown. \
               The tracks in the cache all come from motor sports."
            .into(),
    }
}

fn notes_of(routes: &[Route], shown: &[RouteRow], total: usize) -> Vec<String> {
    let mut notes = Vec::new();
    let mut sports: Vec<String> = routes
        .iter()
        .map(|r| sport_label(r.type_key.as_deref()).to_lowercase())
        .collect();
    sports.sort();
    sports.dedup();
    notes.push(format!("These routes come from {}.", list_of(&sports)));
    if routes.iter().all(|r| r.times == 1) {
        notes.push(
            "Nothing repeats yet. Every trace starts or ends in a different place, or covers a \
             different distance. These are one-off outings, not a route that you return to."
                .into(),
        );
    }
    if shown.iter().any(|r| r.trace.is_empty()) {
        notes.push(format!(
            "Only the first {} routes in this order carry a drawn shape.",
            query::TRACED_ROUTES
        ));
    }
    if total > shown.len() {
        notes.push(format!("The list shows the first {} routes.", shown.len()));
    }
    notes
}

pub fn routes_view(db: &Db, sort: RouteSort) -> Result<RoutesView, ApiError> {
    let all = query::routes(db, sort)?;
    if all.is_empty() {
        let motor_only = db
            .track_headers()?
            .iter()
            .any(|t| t.point_count > 1 && is_motorised(t.type_key.as_deref()));
        return Ok(RoutesView {
            empty: Some(if motor_only {
                motor_only_note()
            } else {
                empty_note()
            }),
            sort,
            tiles: Vec::new(),
            notes: Vec::new(),
            total: 0,
            routes: Vec::new(),
            longest: None,
            courses: course_rows(db)?,
        });
    }
    let total = all.len();
    let shown: Vec<RouteRow> = all.iter().take(ROUTE_LIMIT).map(row_of).collect();
    let outings: usize = all.iter().map(|r| r.times).sum();
    let repeated = all.iter().filter(|r| r.times > 1).count();
    Ok(RoutesView {
        empty: None,
        sort,
        tiles: vec![
            Tile::new("Distinct routes", total.to_string()),
            Tile::new("Outings", outings.to_string()),
            Tile::new("Repeated routes", repeated.to_string()),
        ],
        notes: notes_of(&all, &shown, total),
        total,
        longest: longest_route(&all),
        courses: course_rows(db)?,
        routes: shown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_fills_the_unit_square_and_keeps_its_proportions() {
        // A box 0.002 deg tall and 0.004 deg wide at the equator: twice as wide as tall.
        let s = shape(&[[0.0, 0.0], [0.002, 0.0], [0.002, 0.004], [0.0, 0.004]]);
        assert_eq!(s.len(), 5);
        assert!(s
            .iter()
            .all(|p| (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1])));
        let (min_x, max_x) = (0.0, 1.0);
        let ys: Vec<f32> = s.iter().map(|p| p[1]).collect();
        let height = ys.iter().cloned().fold(f32::MIN, f32::max)
            - ys.iter().cloned().fold(f32::MAX, f32::min);
        assert!(
            (max_x - min_x - 2.0 * height).abs() < 0.01,
            "height {height}"
        );
        // North is up: the first point is the south-west corner, so it sits low.
        assert!(s[0][1] > s[1][1]);
    }

    #[test]
    fn a_trace_that_never_moved_or_has_bad_points_is_safe() {
        assert!(shape(&[[10.0, 20.0], [10.0, 20.0], [10.0, 20.0]]).is_empty());
        let s = shape(&[
            [10.0, 20.0],
            [f64::NAN, 20.1],
            [10.1, f64::INFINITY],
            [10.1, 20.1],
        ]);
        assert_eq!(s.len(), 3);
        assert!(s
            .iter()
            .flatten()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }

    #[test]
    fn gps_jitter_around_one_place_has_no_shape() {
        let jitter: Vec<[f64; 2]> = (0..50)
            .map(|i| {
                [
                    43.0 + f64::from(i % 3) * 1e-5,
                    17.0 + f64::from(i % 5) * 1e-5,
                ]
            })
            .collect();
        assert!(shape(&jitter).is_empty());
    }

    #[test]
    fn a_trace_of_one_point_has_no_shape() {
        assert!(shape(&[[1.0, 1.0]]).is_empty());
        assert!(shape(&[]).is_empty());
    }

    #[test]
    fn a_long_trace_is_thinned_and_keeps_its_last_point() {
        let line: Vec<[f64; 2]> = (0..1_000).map(|i| [f64::from(i) * 1e-5, 0.0]).collect();
        let s = shape(&line);
        assert!(s.len() <= SHAPE_POINTS + 2, "{}", s.len());
        assert_eq!(s.last().unwrap()[1], 0.0);
    }

    #[test]
    fn a_motorcycle_trace_is_not_a_route() {
        let dir = std::env::temp_dir().join(format!("garmin-api-routes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::open(&dir.join("cache.sqlite3")).unwrap();
        for (id, key) in [(1, "walking"), (2, "motorcycling_v2")] {
            let a = serde_json::json!({
                "activityId": id,
                "startTimeLocal": "2026-03-10 09:00:00",
                "distance": 5_000.0,
                "duration": 3_600.0,
                "activityType": { "typeKey": key },
            });
            db.upsert_activity(&serde_json::from_value(a).unwrap())
                .unwrap();
            db.upsert_track(&garmin_core::db::ActivityTrack {
                activity_id: id,
                point_count: 2,
                start_lat: Some(50.0 + id as f64),
                start_lon: Some(10.0),
                end_lat: Some(50.1 + id as f64),
                end_lon: Some(10.1),
                points: vec![[50.0, 10.0], [50.1, 10.1]],
                ..Default::default()
            })
            .unwrap();
        }
        let v = routes_view(&db, RouteSort::default()).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(v.total, 1);
        assert_eq!(v.routes[0].sport, "Walking");
    }

    #[test]
    fn only_motorcycle_tracks_get_the_motor_message() {
        let dir = std::env::temp_dir().join(format!("garmin-api-motor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::open(&dir.join("cache.sqlite3")).unwrap();
        let a = serde_json::json!({
            "activityId": 1,
            "startTimeLocal": "2026-03-10 09:00:00",
            "activityType": { "typeKey": "motorcycling_v2" },
        });
        db.upsert_activity(&serde_json::from_value(a).unwrap())
            .unwrap();
        db.upsert_track(&garmin_core::db::ActivityTrack {
            activity_id: 1,
            point_count: 2,
            start_lat: Some(50.0),
            start_lon: Some(10.0),
            end_lat: Some(50.1),
            end_lon: Some(10.1),
            points: vec![[50.0, 10.0], [50.1, 10.1]],
            ..Default::default()
        })
        .unwrap();
        let v = routes_view(&db, RouteSort::default()).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        assert!(v.empty.unwrap().body.contains("Motor sports are not shown"));
    }

    fn outing(id: i64, date: &str, secs: Option<f64>, climb: Option<f64>) -> query::RouteOuting {
        query::RouteOuting {
            activity_id: id,
            name: Some("Loop".into()),
            local_date: Some(date.into()),
            distance_m: Some(5_000.0),
            duration_s: secs,
            points: Vec::new(),
            elevation_gain_m: climb,
        }
    }

    #[test]
    fn the_best_time_is_the_shortest_positive_one_and_the_older_wins_a_tie() {
        let outings = [
            outing(1, "2026-03-10", Some(1_500.0), Some(40.0)),
            outing(2, "2026-03-05", Some(1_400.0), Some(60.0)),
            outing(3, "2026-03-01", Some(1_400.0), None),
            outing(4, "2026-02-01", Some(0.0), Some(0.0)),
            outing(5, "2026-01-01", None, None),
        ];
        let best = best_time(&outings).unwrap();
        assert_eq!(
            (best.activity_id, best.text.as_str(), best.date.as_str()),
            (3, "23 min", "2026-03-01")
        );
        assert_eq!(mean_elevation(&outings), Some(50.0));
        assert_eq!(best_time(&outings[3..]), None);
        assert_eq!(mean_elevation(&outings[3..]), None);
    }

    #[test]
    fn the_longest_route_is_by_mean_distance() {
        let route = |name: &str, avg: Option<f64>| Route {
            name: None,
            type_key: None,
            times: 1,
            avg_distance_m: avg,
            outings: vec![query::RouteOuting {
                name: Some(name.into()),
                ..outing(1, "2026-03-10", None, None)
            }],
        };
        let longest = longest_route(&[
            route("Short", Some(3_000.0)),
            route("Far", Some(21_100.0)),
            route("None", None),
        ])
        .unwrap();
        assert_eq!(
            (longest.name.as_str(), longest.distance.as_str()),
            ("Far", "21.1 km")
        );
        assert_eq!(longest_route(&[route("None", None)]), None);
    }

    #[test]
    fn a_course_shows_with_its_shape_even_with_no_activity_track() {
        let dir = std::env::temp_dir().join(format!("garmin-api-courses-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::open(&dir.join("cache.sqlite3")).unwrap();
        db.upsert_course(&garmin_core::courses::CourseSummary {
            course_id: 1,
            name: Some("Loop".into()),
            activity_type: Some("running".into()),
            distance_m: Some(5_000.0),
            elevation_gain_m: Some(30.0),
            favorite: true,
            ..Default::default()
        })
        .unwrap();
        db.save_course_track(1, &[[50.0, 10.0], [50.1, 10.1], [50.0, 10.2]])
            .unwrap();
        db.upsert_course(&garmin_core::courses::CourseSummary {
            course_id: 2,
            ..Default::default()
        })
        .unwrap();
        let v = routes_view(&db, RouteSort::default()).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        assert!(v.empty.is_some() && v.longest.is_none());
        assert_eq!(v.courses.len(), 2);
        let c = &v.courses[0];
        assert_eq!(
            (c.name.as_str(), c.sport.as_str(), c.distance.as_deref()),
            ("Loop", "Running", Some("5.0 km"))
        );
        assert_eq!(c.trace.len(), 4);
        assert_eq!(
            (v.courses[1].name.as_str(), v.courses[1].trace.is_empty()),
            ("Untitled", true)
        );
    }

    #[test]
    fn a_list_reads_as_a_phrase() {
        let v = |a: &[&str]| list_of(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(v(&["walking"]), "walking");
        assert_eq!(v(&["walking", "golf"]), "walking and golf");
        assert_eq!(v(&["a", "b", "c"]), "a, b and c");
    }
}
