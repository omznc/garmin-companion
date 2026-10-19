//! The Plan, Routes and Gear screens, from `app/src/screens/Plan.tsx`,
//! `Routes.tsx` and `Gear.tsx`.
//!
//! The API builds the numbers and the sentences. This file lays them out.
//! Plan and Routes read the cache. Gear reads Garmin live on a worker thread,
//! because Garmin keeps gear on its own service.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use chrono::NaiveDate;
use garmin_api::gear::{fetch_gear, gear_view, GearRow, GearView};
use garmin_api::personas::{sample_day, seed, Persona};
use garmin_api::places::{persona_gear, seed_places, Tile};
use garmin_api::plan::{plan_view, PlanView, WorkoutRow};
use garmin_api::routes::{routes_view, OutingRow, RouteRow, RouteSort, RoutesView};
use gpui::{
    canvas, div, point, prelude::*, px, relative, AnyElement, ClickEvent, Context, Div, PathBuilder,
};

use crate::body::{empty_state, head, note, row, section};
use crate::data::{Metric, Part, Source};
use crate::shell::Root;
use crate::theme::{Theme, FS_BASE, FS_CAPTION, FS_LG, FS_MD, FS_SMALL};
use crate::ui::{block_in, body, ch, metric, MONO, SANS, SERIF};

/// Where the cache and the gear come from.
#[derive(Clone, Copy, PartialEq)]
enum Origin {
    /// The real cache. Gear comes live from Garmin.
    Live,
    /// A copy of the cache. It has no gear.
    Copy,
    Persona(Persona),
}

pub enum Gear {
    Idle,
    Loading,
    Ready(GearView),
    Failed(String),
}

pub struct Places {
    origin: Origin,
    path: PathBuf,
    today: NaiveDate,
    sort: RouteSort,
    plan: Result<PlanView, String>,
    routes: Result<RoutesView, String>,
    gear: Gear,
}

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
            sort: RouteSort::Recent,
            plan: Err(String::new()),
            routes: Err(String::new()),
            gear: Gear::Idle,
        };
        places.reload();
        places
    }

    /// Drop the gear of the old account, so the next visit asks Garmin again.
    pub fn forget_gear(&mut self) {
        self.gear = Gear::Idle;
    }

    /// Read Plan and Routes again, after a sync or a new order.
    pub fn reload(&mut self) {
        (self.plan, self.routes) = self.reader()();
    }

    /// A job that reads Plan and Routes. It owns its inputs, so it can run on a thread.
    fn reader(&self) -> impl FnOnce() -> PlacesRead + Send + 'static {
        let (path, today, sort) = (self.path.clone(), self.today, self.sort);
        move || match garmin_api::open_db(&path).map_err(|e| e.to_string()) {
            Ok(db) => (
                plan_view(&db, today).map_err(|e| e.to_string()),
                routes_view(&db, sort).map_err(|e| e.to_string()),
            ),
            Err(e) => (Err(e.clone()), Err(e)),
        }
    }
}

type PlacesRead = (Result<PlanView, String>, Result<RoutesView, String>);

impl Root {
    /// Read Plan and Routes on the background executor. The screens keep the
    /// old figures until the new ones arrive.
    pub fn reload_places(&mut self, cx: &mut Context<Self>) {
        let read = self.places.reader();
        crate::account::load_async(self, "places", cx, read, |root, (plan, routes), cx| {
            root.places.plan = plan;
            root.places.routes = routes;
            cx.notify();
        });
    }

    /// Choose the order of the routes and read them again.
    fn set_route_sort(&mut self, sort: RouteSort, cx: &mut Context<Self>) {
        self.places.sort = sort;
        self.reload_places(cx);
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
            Origin::Copy => {
                self.places.gear = Gear::Failed(
                    "Gear comes live from Garmin, and a copy of the cache holds none.".into(),
                );
            }
            Origin::Live if !self.account.connected => {
                self.places.gear = Gear::Failed("Sign in to Garmin to see your gear.".into());
            }
            Origin::Live => self.ask_garmin_for_gear(cx),
        }
        cx.notify();
    }

    fn ask_garmin_for_gear(&mut self, cx: &mut Context<Self>) {
        self.places.gear = Gear::Loading;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(
                fetch_gear()
                    .map(|entries| gear_view(&entries))
                    .map_err(|e| e.to_string()),
            );
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
                    Err(e) => Gear::Failed(e),
                };
                cx.notify();
            });
            break;
        })
        .detach();
    }
}

/// The screen for a sidebar label, or `None` for a label that this file does not draw.
pub fn screen(label: &str, root: &Root, t: &Theme, cx: &mut Context<Root>) -> Option<AnyElement> {
    let key = root.visits;
    Some(match label {
        "Plan" => plan(&root.places.plan, key, t),
        "Routes" => routes(&root.places, key, t, cx),
        "Gear" => gear(&root.places.gear, key, t, cx),
        _ => return None,
    })
}

fn failed(title: &str, message: &str, key: usize, t: &Theme) -> AnyElement {
    div()
        .child(head("Garmin Companion".into(), title, "", key, t))
        .child(block_in(
            key,
            1,
            body(message.to_string(), FS_MD, t.mutd).max_w(ch(62.0, FS_MD)),
        ))
        .into_any_element()
}

fn tiles(items: &[Tile], t: &Theme) -> Div {
    items.iter().fold(
        div().flex().flex_wrap().gap_x(px(44.0)).gap_y(px(24.0)),
        |r, tile| {
            let m = Metric {
                label: tile.label.clone(),
                value: vec![Part {
                    text: tile.value.clone(),
                    unit: false,
                }],
                accent: false,
            };
            r.child(metric(&m, t))
        },
    )
}

/* -------------------------------------------------------------------- plan --- */

fn plan(view: &Result<PlanView, String>, key: usize, t: &Theme) -> AnyElement {
    let v = match view {
        Ok(v) => v,
        Err(e) => return failed("Plan", e, key, t),
    };
    let eyebrow = format!("Last {} days", v.window_days);
    let lede = "The workouts you have built, and how your training compares with them.";
    if let Some(e) = &v.empty {
        return div()
            .child(head(eyebrow, "Plan", lede, key, t))
            .child(block_in(key, 2, empty_state(e, t)))
            .child(note(&v.gap_note, t).mt(px(40.0)))
            .into_any_element();
    }
    let mut page = div()
        .child(head(eyebrow, "Plan", lede, key, t))
        .child(block_in(key, 2, tiles(&v.tiles, t)));
    if let Some(s) = &v.sentence {
        page = page.child(
            body(s.clone(), FS_MD, t.mutd)
                .mt(px(20.0))
                .max_w(ch(62.0, FS_MD)),
        );
    }
    page = page.child(section("Saved workouts", t));
    for w in &v.workouts {
        page = page.child(workout_row(w, t));
    }
    page.child(section("What Garmin does not have", t))
        .child(body(v.gap_note.clone(), FS_MD, t.mutd).max_w(ch(62.0, FS_MD)))
        .into_any_element()
}

fn workout_row(w: &WorkoutRow, t: &Theme) -> Div {
    let size = w.size.clone().unwrap_or_else(|| "—".into());
    let line = row(
        w.name.clone(),
        &[(w.sport.clone(), t.faint), (size, t.fg)],
        t,
    );
    match &w.description {
        Some(d) => div().child(line).child(
            body(d.clone(), FS_CAPTION, t.faint)
                .pt(px(4.0))
                .pb(px(10.0)),
        ),
        None => div().child(line),
    }
}

/* ------------------------------------------------------------------ routes --- */

const SORTS: [(RouteSort, &str); 3] = [
    (RouteSort::Recent, "Latest"),
    (RouteSort::Repeats, "Most repeated"),
    (RouteSort::Distance, "Longest"),
];

fn routes(places: &Places, key: usize, t: &Theme, cx: &mut Context<Root>) -> AnyElement {
    let v = match &places.routes {
        Ok(v) => v,
        Err(e) => return failed("Routes", e, key, t),
    };
    let lede = "Every route you have a GPS trace for. Outings that start and finish in the \
                same place over a similar distance are one route.";
    let mut page = div().child(head("All cached traces".into(), "Routes", lede, key, t));
    if let Some(e) = &v.empty {
        return page
            .child(block_in(key, 2, empty_state(e, t)))
            .into_any_element();
    }
    page = page.child(block_in(key, 2, tiles(&v.tiles, t)));
    for n in &v.notes {
        page = page.child(note(n, t));
    }
    page = page.child(sort_tabs(v.sort, t, cx));
    for r in &v.routes {
        page = page.child(route_card(r, t, cx));
    }
    page.into_any_element()
}

fn sort_tabs(active: RouteSort, t: &Theme, cx: &mut Context<Root>) -> Div {
    SORTS.iter().fold(
        div()
            .flex()
            .gap(px(18.0))
            .items_baseline()
            .mt(px(40.0))
            .font_family(SANS)
            .text_size(px(FS_SMALL)),
        |tabs, (sort, label)| {
            let sort = *sort;
            let on = sort == active;
            tabs.child(
                div()
                    .id(*label)
                    .cursor_pointer()
                    .pb(px(3.0))
                    .border_b_1()
                    .border_color(if on { t.acc } else { gpui::transparent_black() })
                    .text_color(if on { t.fg } else { t.faint })
                    .child(*label)
                    .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| {
                        root.set_route_sort(sort, cx)
                    })),
            )
        },
    )
}

const BOX: f32 = 78.0;

fn route_card(r: &RouteRow, t: &Theme, cx: &mut Context<Root>) -> Div {
    let mut text = div()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .flex()
                .justify_between()
                .items_baseline()
                .gap(px(16.0))
                .child(
                    div()
                        .font_family(SANS)
                        .text_size(px(FS_LG))
                        .child(r.name.clone()),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(FS_BASE))
                        .child(r.distance.clone().unwrap_or_else(|| "—".into())),
                ),
        )
        .child(body(format!("{} · {}", r.sport, r.times_line), FS_SMALL, t.faint).mt(px(5.0)));
    if !r.outings.is_empty() {
        let list = r
            .outings
            .iter()
            .fold(div().mt(px(12.0)), |l, o| l.child(outing_row(o, t, cx)));
        text = text.child(list);
    }
    div()
        .flex()
        .gap(px(26.0))
        .items_start()
        .py(px(22.0))
        .border_b_1()
        .border_color(t.line2)
        .child(trace(&r.trace, t))
        .child(text)
}

fn outing_row(o: &OutingRow, t: &Theme, cx: &mut Context<Root>) -> impl IntoElement {
    let id = o.activity_id;
    div()
        .id(("outing", id as u64))
        .flex()
        .justify_between()
        .gap(px(14.0))
        .py(px(4.0))
        .cursor_pointer()
        .text_size(px(FS_SMALL))
        .text_color(t.mutd)
        .child(o.date.clone())
        .child(div().font_family(MONO).child(o.detail.clone()))
        .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.start_at_activity(id, cx)))
}

/// The shape of a route, in the accent colour. A route with no trace leaves the box blank.
fn trace(points: &[[f32; 2]], t: &Theme) -> impl IntoElement {
    let (points, acc) = (points.to_vec(), t.acc);
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            if points.len() < 2 {
                return;
            }
            let inner = BOX - 8.0;
            let at = |p: &[f32; 2]| {
                point(
                    bounds.origin.x + px(4.0 + p[0] * inner),
                    bounds.origin.y + px(4.0 + p[1] * inner),
                )
            };
            let mut path = PathBuilder::stroke(px(1.4));
            path.move_to(at(&points[0]));
            for p in &points[1..] {
                path.line_to(at(p));
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, acc);
            }
        },
    )
    .size(px(BOX))
    .flex_none()
}

/* -------------------------------------------------------------------- gear --- */

fn gear(state: &Gear, key: usize, t: &Theme, cx: &mut Context<Root>) -> AnyElement {
    let lede = "Shoes, bikes and other gear registered on your account, with the distance logged against each.";
    let page = div().child(head("Live from Garmin".into(), "Gear", lede, key, t));
    match state {
        Gear::Idle | Gear::Loading => page
            .child(body("Asking Garmin about your gear.", FS_MD, t.mutd))
            .into_any_element(),
        Gear::Failed(message) => page
            .child(body(message.clone(), FS_MD, t.mutd).max_w(ch(62.0, FS_MD)))
            .child(
                div()
                    .id("gear-retry")
                    .mt(px(18.0))
                    .cursor_pointer()
                    .text_size(px(FS_SMALL))
                    .text_color(t.acc)
                    .child("Try again")
                    .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.load_gear(cx))),
            )
            .into_any_element(),
        Gear::Ready(v) => match &v.empty {
            Some(e) => page
                .child(block_in(key, 2, empty_state(e, t)))
                .into_any_element(),
            None => v
                .items
                .iter()
                .fold(page, |p, item| p.child(gear_item(item, t)))
                .child(note(&v.note, t).mt(px(26.0)))
                .into_any_element(),
        },
    }
}

fn gear_item(g: &GearRow, t: &Theme) -> Div {
    let colour = if g.accent { t.acc } else { t.mutd };
    let title = div()
        .flex_1()
        .min_w_0()
        .font_family(SANS)
        .text_size(px(FS_LG))
        .child(g.name.clone())
        .children(g.kind.clone().map(|k| {
            div()
                .ml(px(10.0))
                .text_size(px(FS_SMALL))
                .text_color(t.faint)
                .child(k)
        }));
    let top = div()
        .flex()
        .items_baseline()
        .gap(px(16.0))
        .child(title)
        .child(
            div()
                .font_family(SERIF)
                .text_size(px(24.0))
                .child(g.distance.clone().unwrap_or_else(|| "—".into())),
        )
        .child(
            div()
                .w(px(92.0))
                .text_size(px(FS_SMALL))
                .text_color(colour)
                .child(g.status.clone()),
        );
    let mut item = div()
        .py(px(22.0))
        .border_b_1()
        .border_color(t.line2)
        .font_family(SANS)
        .line_height(relative(1.26))
        .child(top);
    if let Some(pct) = g.wear_pct {
        item = item.child(
            div().mt(px(14.0)).h(px(3.0)).w_full().bg(t.line2).child(
                div()
                    .h_full()
                    .w(relative(pct as f32 / 100.0))
                    .bg(if g.accent { t.acc } else { t.mutd }),
            ),
        );
    }
    item.children(
        g.detail
            .clone()
            .map(|d| body(d, FS_CAPTION, t.faint).mt(px(10.0))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_persona_loads_plan_and_routes_and_a_new_user_is_empty() {
        for p in Persona::ALL {
            let places = Places::load(&Source::Persona(p.name().into()));
            let (plan, routes) = (places.plan.unwrap(), places.routes.unwrap());
            if p == Persona::NewUser {
                assert!(plan.empty.is_some() && routes.empty.is_some());
            }
            if p == Persona::Sample {
                assert_eq!(routes.routes.len(), 2);
            }
        }
    }

    #[test]
    fn a_new_order_changes_the_first_route() {
        let mut places = Places::load(&Source::Persona("sample".into()));
        places.sort = RouteSort::Distance;
        places.reload();
        let first = places.routes.unwrap().routes[0].distance.clone();
        assert_eq!(first.as_deref(), Some("8.0 km"));
    }
}
