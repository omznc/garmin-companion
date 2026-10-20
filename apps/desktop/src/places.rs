//! The data of the Routes and Gear screens.
//!
//! The API builds the numbers and the sentences. The blocks lay them out.
//! Routes read the cache. Gear reads Garmin live on a worker thread,
//! because Garmin keeps gear on its own service.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use chrono::NaiveDate;
use garmin_api::gear::{cached_gear, fetch_gear, gear_view, store_gear, GearView};
use garmin_api::personas::{sample_day, seed, Persona};
use garmin_api::places::{persona_gear, seed_places};
use garmin_api::plan::{plan_view, PlanView};
use garmin_api::routes::{routes_view, RouteSort, RoutesView};
use gpui::Context;

use crate::data::Source;
use crate::shell::Root;

/// Where the cache and the gear come from.
#[derive(Clone, Copy, PartialEq)]
enum Origin {
    /// The real cache. Gear comes live from Garmin.
    Live,
    /// A copy of the cache. It shows the gear that was kept in it.
    Copy,
    Persona(Persona),
}

pub enum Gear {
    Idle,
    Loading,
    Ready(GearView),
    Failed,
}

pub struct Places {
    origin: Origin,
    path: PathBuf,
    today: NaiveDate,
    plan: Result<PlanView, String>,
    routes: Result<RoutesView, String>,
    gear: Gear,
}

type PlacesRead = (Result<PlanView, String>, Result<RoutesView, String>);

/// A persona is a made-up cache that the app seeds in a temp folder.
fn persona_cache(name: &str) -> Result<(PathBuf, Persona), String> {
    let persona = Persona::from_name(name).ok_or_else(|| format!("no persona {name}"))?;
    // Each load gets its own folder, so two loads never share a file.
    static LOADS: AtomicUsize = AtomicUsize::new(0);
    let n = LOADS.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("gc-places-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("cache.sqlite3");
    let db = garmin_api::open_db(&path).map_err(|e| e.to_string())?;
    seed(&db, persona, sample_day()).map_err(|e| e.to_string())?;
    seed_places(&db, persona).map_err(|e| e.to_string())?;
    Ok((path, persona))
}

/// The gear that was kept in the cache. `Failed` when none was kept.
fn kept_gear(path: &std::path::Path) -> Gear {
    garmin_api::open_db(path)
        .ok()
        .and_then(|db| cached_gear(&db))
        .map_or(Gear::Failed, |entries| Gear::Ready(gear_view(&entries)))
}

/// Ask Garmin and keep the answer. When Garmin cannot answer, use the kept gear.
fn live_or_kept_gear(path: &std::path::Path) -> Result<GearView, String> {
    match fetch_gear() {
        Ok(entries) => {
            if let Err(e) = garmin_api::open_db(path).and_then(|db| store_gear(&db, &entries)) {
                eprintln!("could not keep the gear: {e}");
            }
            Ok(gear_view(&entries))
        }
        Err(e) => match kept_gear(path) {
            Gear::Ready(view) => Ok(view),
            _ => Err(e.to_string()),
        },
    }
}

impl Places {
    pub fn load(source: &Source) -> Self {
        let today = chrono::Local::now().date_naive();
        let (origin, path, today) = match source {
            Source::Default => (
                Origin::Live,
                garmin_core::db::default_path().unwrap_or_default(),
                today,
            ),
            Source::Db { path, date } => (Origin::Copy, path.clone(), date.unwrap_or(today)),
            Source::Persona(name) => match persona_cache(name) {
                Ok((path, p)) => (Origin::Persona(p), path, sample_day()),
                Err(_) => (Origin::Copy, PathBuf::new(), today),
            },
        };
        let mut places = Self {
            origin,
            path,
            today,
            plan: Err(String::new()),
            routes: Err(String::new()),
            gear: Gear::Idle,
        };
        places.reload();
        places
    }

    /// The cache file that this source reads, and the day it reads for.
    pub fn cache(&self) -> (PathBuf, NaiveDate) {
        (self.path.clone(), self.today)
    }

    /// Read for another day. A persona keeps its day.
    pub fn set_day(&mut self, day: NaiveDate) {
        if !matches!(self.origin, Origin::Persona(_)) {
            self.today = day;
        }
    }

    /// The saved workouts. `None` until they have loaded, and when the read fails.
    pub fn plan(&self) -> Option<&PlanView> {
        self.plan.as_ref().ok()
    }

    /// The saved routes. `None` until they have loaded, and when the read fails.
    pub fn routes(&self) -> Option<&RoutesView> {
        self.routes.as_ref().ok()
    }

    /// The gear, once Garmin has answered.
    pub fn gear(&self) -> Option<&GearView> {
        match &self.gear {
            Gear::Ready(view) => Some(view),
            _ => None,
        }
    }

    /// The number of saved routes, for the sidebar.
    pub fn route_count(&self) -> Option<i64> {
        self.routes.as_ref().ok().map(|r| r.total as i64)
    }

    /// The number of gear items, once Garmin has answered.
    pub fn gear_count(&self) -> Option<i64> {
        self.gear().map(|view| view.items.len() as i64)
    }

    /// Drop the gear of the old account, so the next visit asks Garmin again.
    pub fn forget_gear(&mut self) {
        self.gear = Gear::Idle;
    }

    /// Read the routes again, after a sync.
    pub fn reload(&mut self) {
        (self.plan, self.routes) = self.reader()();
    }

    /// A job that reads the plan and the routes. It owns its inputs, so it can run on a thread.
    fn reader(&self) -> impl FnOnce() -> PlacesRead + Send + 'static {
        let (path, today) = (self.path.clone(), self.today);
        move || match garmin_api::open_db(&path).map_err(|e| e.to_string()) {
            Ok(db) => (
                plan_view(&db, today).map_err(|e| e.to_string()),
                routes_view(&db, RouteSort::Recent).map_err(|e| e.to_string()),
            ),
            Err(e) => (Err(e.clone()), Err(e)),
        }
    }
}

impl Root {
    /// Read the routes on the background executor. The screens keep the old
    /// figures until the new ones arrive.
    pub fn reload_places(&mut self, cx: &mut Context<Self>) {
        let read = self.places.reader();
        crate::account::load_async(self, "places", cx, read, |root, (plan, routes), cx| {
            root.places.plan = plan;
            root.places.routes = routes;
            cx.notify();
        });
    }

    /// Fill the Gear screen. A persona answers at once. A live account asks Garmin on a thread.
    pub fn load_gear(&mut self, cx: &mut Context<Self>) {
        if matches!(self.places.gear, Gear::Loading | Gear::Ready(_)) {
            return;
        }
        match self.places.origin {
            Origin::Persona(p) => {
                self.places.gear = Gear::Ready(gear_view(&persona_gear(p)));
            }
            Origin::Copy => self.places.gear = kept_gear(&self.places.path),
            Origin::Live if !self.account.connected => {
                self.places.gear = kept_gear(&self.places.path);
            }
            Origin::Live => self.ask_garmin_for_gear(cx),
        }
        cx.notify();
    }

    fn ask_garmin_for_gear(&mut self, cx: &mut Context<Self>) {
        self.places.gear = Gear::Loading;
        let (tx, rx) = mpsc::channel();
        let path = self.places.path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(live_or_kept_gear(&path));
        });
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let done = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("The task stopped without a result.".into()))
                }
            };
            let Some(result) = done else { continue };
            let _ = this.update(cx, |root, cx| {
                root.places.gear = match result {
                    Ok(view) => Gear::Ready(view),
                    Err(e) => {
                        eprintln!("could not read the gear: {e}");
                        Gear::Failed
                    }
                };
                cx.notify();
            });
            break;
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_persona_loads_routes_and_a_new_user_is_empty() {
        for p in Persona::ALL {
            let places = Places::load(&Source::Persona(p.name().into()));
            let routes = places.routes.unwrap();
            if p == Persona::NewUser {
                assert!(routes.empty.is_some());
            }
            if p == Persona::Sample {
                assert_eq!(routes.routes.len(), 2);
            }
        }
    }
}
