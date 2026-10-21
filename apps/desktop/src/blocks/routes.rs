//! The blocks of the Routes screen: the saved routes, the most run ones and
//! a few figures.

use crate::scale::px;
use garmin_api::routes::{RouteSort, RoutesView};
use gpui::{canvas, div, fill, point, prelude::*, size, Bounds, Hsla, PathBuilder, Pixels, Window};

use super::{Block, BlockDef, Ctx, Width};
use crate::charts::Tone;
use crate::icons::Icon;
use crate::kit::{self, Cell, ListRow};
use crate::screen::Screen;
use crate::theme::Theme;
use crate::ui::MONO;

/// The blocks that belong to this screen.
pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "routes_saved",
        title: "Saved routes",
        group: Screen::Routes,
        default_width: Width::Full,
        render: saved,
    },
    BlockDef {
        id: "routes_most_run",
        title: "Most used",
        group: Screen::Routes,
        default_width: Width::TwoThirds,
        render: most_run,
    },
    BlockDef {
        id: "routes_numbers",
        title: "Routes in numbers",
        group: Screen::Routes,
        default_width: Width::Third,
        render: numbers,
    },
    BlockDef {
        id: "routes_outings",
        title: "Outings",
        group: Screen::Routes,
        default_width: Width::Full,
        render: outings,
    },
];

/// The default order and width of the blocks.
pub const LAYOUT: &[(&str, Width)] = &[
    ("routes_saved", Width::Full),
    ("routes_most_run", Width::TwoThirds),
    ("routes_numbers", Width::Third),
    ("routes_outings", Width::Full),
];
const MAX_OUTINGS: usize = 20;

pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

pub fn lede(_ctx: &Ctx) -> Option<String> {
    Some(
        "Every route with a GPS trace. Outings that start and end in the same place \
         over a similar distance are one route."
            .into(),
    )
}

pub fn crumb(_ctx: &Ctx) -> Option<String> {
    None
}

/// The most tiles that the grid draws.
const MAX_TILES: usize = 18;
/// The width that a tile asks for, in pixels.
const TILE_WIDTH: f32 = 160.0;
const TRACE_HEIGHT: f32 = 100.0;

fn block(meta: Option<String>, body: impl IntoElement) -> Option<Block> {
    Some(Block {
        meta: meta.map(Into::into),
        link: None,
        body: body.into_any_element(),
    })
}

/// One route on the grid.
struct Saved<'a> {
    name: &'a str,
    distance: Option<&'a str>,
    elevation_m: Option<f64>,
    trace: &'a [[f32; 2]],
    /// `Course` for a Garmin course, else how often the route was run. It
    /// tells two routes with the same name apart.
    tag: String,
    course: bool,
}

/// A route shorter than this is a walk round the block, not a route.
const MIN_ROUTE_M: f64 = 1000.0;

/// The courses that the user saved, then the routes that have a trace, in
/// the order that the user chose. Short ones are left out.
fn saved_routes(view: &RoutesView) -> Vec<Saved<'_>> {
    let courses = view.courses.iter().map(|c| Saved {
        name: &c.name,
        distance: c.distance.as_deref(),
        elevation_m: c.elevation_m,
        trace: &c.trace,
        tag: "Course".into(),
        course: true,
    });
    let routes = view
        .routes
        .iter()
        .filter(|r| r.distance_m >= MIN_ROUTE_M)
        .map(|r| Saved {
            name: &r.name,
            distance: r.distance.as_deref(),
            elevation_m: r.elevation_m,
            trace: &r.trace,
            tag: r.times_line.clone(),
            course: false,
        });
    courses
        .chain(routes)
        .filter(|s| s.trace.len() >= 2)
        .collect()
}

/* ------------------------------------------------------------------ saved --- */

/// The shape on a faint grid of 12 px, with a soft glow under the line.
fn paint_trace(
    bounds: Bounds<Pixels>,
    points: &[[f32; 2]],
    (line, glow, acc): (Hsla, Hsla, Hsla),
    window: &mut Window,
) {
    let (w, h) = (
        crate::scale::dp(bounds.size.width),
        crate::scale::dp(bounds.size.height),
    );
    let mut x = 0.0;
    while x < w {
        let bar = Bounds::new(bounds.origin + point(px(x), px(0.0)), size(px(1.0), px(h)));
        window.paint_quad(fill(bar, line));
        x += 12.0;
    }
    let mut y = 0.0;
    while y < h {
        let bar = Bounds::new(bounds.origin + point(px(0.0), px(y)), size(px(w), px(1.0)));
        window.paint_quad(fill(bar, line));
        y += 12.0;
    }
    let side = w.min(h) - 16.0;
    let (left, top) = ((w - side) / 2.0, (h - side) / 2.0);
    let at = |p: &[f32; 2]| bounds.origin + point(px(left + p[0] * side), px(top + p[1] * side));
    for (width, colour) in [(6.0, glow), (1.8, acc)] {
        let mut path = PathBuilder::stroke(px(width));
        path.move_to(at(&points[0]));
        for p in &points[1..] {
            path.line_to(at(p));
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, colour);
        }
    }
}

fn tile(s: &Saved, t: &Theme) -> gpui::Div {
    use gpui::ColorExt as _;
    let (points, colours) = (s.trace.to_vec(), (t.line2, t.acc.opacity(0.25), t.acc));
    let meta = [
        Some(s.tag.clone()),
        s.distance.map(str::to_string),
        s.elevation_m.map(|m| format!("{} m", m.round())),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    div()
        .flex_basis(px(TILE_WIDTH))
        .flex_grow(1.0)
        .min_w_0()
        .overflow_hidden()
        .rounded(px(7.0))
        .border_1()
        .border_color(if s.course { t.accline } else { t.line2 })
        .bg(t.bg2)
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, (), window, _| paint_trace(bounds, &points, colours, window),
            )
            .w_full()
            .h(px(TRACE_HEIGHT))
            .bg(t.bg),
        )
        .child(
            div()
                .px(px(9.0))
                .py(px(7.0))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(s.name.to_string()),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_family(MONO)
                        .text_size(px(11.0))
                        .text_color(t.mutd)
                        .child(meta),
                ),
        )
}

fn saved(ctx: &Ctx) -> Option<Block> {
    let items = saved_routes(ctx.places.routes()?);
    if items.is_empty() {
        return None;
    }
    let view = ctx.places.routes()?;
    let mut grid = div().flex().flex_wrap().gap(px(10.0)).p(px(12.0));
    for s in items.iter().take(MAX_TILES) {
        grid = grid.child(tile(s, ctx.t));
    }
    // Empty tiles of the same size keep the last row from stretching.
    for _ in 0..8 {
        grid = grid.child(div().h(px(0.0)).flex_basis(px(TILE_WIDTH)).flex_grow(1.0));
    }
    let meta = if items.len() == 1 {
        "1 route".to_string()
    } else {
        format!("{} routes", items.len())
    };
    block(
        Some(meta),
        div().child(sort_tabs(view.sort, ctx.t)).child(grid),
    )
}

const SORTS: [(RouteSort, &str); 3] = [
    (RouteSort::Recent, "Latest"),
    (RouteSort::Repeats, "Most repeated"),
    (RouteSort::Distance, "Longest"),
];

/// The tabs that choose the order of the routes.
fn sort_tabs(active: RouteSort, t: &Theme) -> gpui::Div {
    SORTS.iter().fold(
        div()
            .flex()
            .gap(px(18.0))
            .items_baseline()
            .px(px(12.0))
            .pt(px(10.0))
            .text_size(px(12.0)),
        |tabs, (sort, label)| {
            let (sort, on) = (*sort, *sort == active);
            tabs.child(
                div()
                    .id(*label)
                    .cursor_pointer()
                    .pb(px(3.0))
                    .border_b_1()
                    .border_color(if on { t.acc } else { gpui::transparent_black() })
                    .text_color(if on { t.fg } else { t.faint })
                    .child(*label)
                    .on_click(move |_, _, cx| {
                        let Some(root) = cx.try_global::<super::RootHandle>().map(|h| h.0.clone())
                        else {
                            return;
                        };
                        let _ = root.update(cx, |root, cx| root.set_route_sort(sort, cx));
                    }),
            )
        },
    )
}

/* --------------------------------------------------------------- most used --- */

fn most_run(ctx: &Ctx) -> Option<Block> {
    let view = ctx.places.routes()?;
    let mut routes: Vec<_> = view
        .routes
        .iter()
        .filter(|r| r.times > 1 && r.distance_m >= MIN_ROUTE_M)
        .collect();
    routes.sort_by_key(|r| std::cmp::Reverse(r.times));
    let rows: Vec<ListRow> = routes
        .into_iter()
        .take(5)
        .map(|r| ListRow {
            glyph: Icon::NavRoutes,
            tone: Tone::Muted,
            title: r.name.clone().into(),
            sub: r
                .best_time
                .as_ref()
                .map(|b| format!("best {}", b.text).into()),
            right: Some(format!("{} ×", r.times).into()),
            right_sub: (r.total_m > 0.0)
                .then(|| format!("{:.0} km total", r.total_m / 1000.0).into()),
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    block(None, kit::list_rows(rows, ctx.t))
}

/* ---------------------------------------------------------------- numbers --- */

fn most_climb(items: &[Saved]) -> Option<f64> {
    items
        .iter()
        .filter_map(|s| s.elevation_m)
        .filter(|m| *m > 0.0)
        .max_by(f64::total_cmp)
}

fn cell(label: &str, value: String, unit: Option<&str>) -> Cell {
    Cell {
        label: label.to_string().into(),
        value: value.into(),
        unit: unit.map(|u| u.to_string().into()),
        sub: None,
        spark: None,
        big: false,
    }
}

fn numbers(ctx: &Ctx) -> Option<Block> {
    let view = ctx.places.routes()?;
    let items = saved_routes(view);
    if items.is_empty() {
        return None;
    }
    let mut cells = vec![cell("Saved", items.len().to_string(), None)];
    cells.extend(
        view.longest
            .as_ref()
            .map(|l| cell("Longest", l.distance.clone(), None)),
    );
    cells.extend(
        most_climb(&items).map(|m| cell("Most climb", format!("{}", m.round()), Some("m"))),
    );
    block(None, kit::stat_grid(cells, 2, ctx.t))
}

/* ---------------------------------------------------------------- outings --- */

/// The outings of the routes that repeat, newest route first.
fn outings(ctx: &Ctx) -> Option<Block> {
    let view = ctx.places.routes()?;
    let rows: Vec<(Vec<gpui::SharedString>, Option<kit::RowClick>)> = view
        .routes
        .iter()
        .filter(|r| r.distance_m >= MIN_ROUTE_M)
        .flat_map(|r| {
            r.outings.iter().map(move |o| {
                let cells = vec![
                    r.name.clone().into(),
                    o.date.clone().into(),
                    o.detail.clone().into(),
                ];
                (cells, Some(super::open_activity(o.activity_id)))
            })
        })
        .take(MAX_OUTINGS)
        .collect();
    if rows.is_empty() {
        return None;
    }
    let column = |title: &str, numeric| kit::Column {
        title: title.to_string().into(),
        numeric,
    };
    let columns = [
        column("Route", false),
        column("Date", false),
        column("Outing", true),
    ];
    block(None, kit::table_rows(&columns, rows, ctx.t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(elevation_m: Option<f64>) -> Saved<'static> {
        Saved {
            name: "Loop",
            distance: None,
            elevation_m,
            trace: &[],
            tag: String::new(),
            course: false,
        }
    }

    #[test]
    fn the_most_climb_skips_flat_and_missing_values() {
        let items = [
            item(None),
            item(Some(0.0)),
            item(Some(142.0)),
            item(Some(24.0)),
        ];
        assert_eq!(most_climb(&items), Some(142.0));
        assert_eq!(most_climb(&[item(Some(0.0))]), None);
    }
}
