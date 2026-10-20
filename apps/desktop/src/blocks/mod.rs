//! The blocks: the units that a screen is built from, and the registry that
//! lists them.
//!
//! A block draws from the shared data of the window (`Ctx`), never from the
//! state of one screen, so any screen can show any block. A block with no
//! data returns `None` and does not show. A screen is a list of block ids with
//! a width for each. The grid has 12 columns with a gap of 14 px.

pub mod activities;
mod daily;
mod fitness;
mod food;
mod gear;
mod health;
mod insights;
mod plan;
mod reports;
mod routes;
mod sleep;
mod strength;
mod today;
mod weight;

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, Div, SharedString};

use garmin_api::charts::ChartsView;
use garmin_api::extras::ExtrasView;
use garmin_api::tiles::TilesView;
use garmin_api::ActivitiesView;

use crate::activities::Store;
use crate::body::BodyData;
use crate::data::Today;
use crate::focus::Focus;
use crate::kit;
use crate::places::Places;
use crate::screen::Screen;
use crate::shell::Root;
use crate::theme::Theme;
use crate::training::Training;
use crate::wellness::Wellness;

/// The gap between blocks, in pixels.
pub const GAP: f32 = 14.0;
const COLUMNS: f32 = 12.0;

/// The width of a block, in columns of the 12 column grid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Width {
    /// 1/3
    Third,
    /// 1/2
    #[allow(dead_code)] // The other screens of the rewrite use it.
    Half,
    /// 2/3
    TwoThirds,
    Full,
}

impl Width {
    pub fn columns(self) -> u8 {
        match self {
            Width::Third => 4,
            Width::Half => 6,
            Width::TwoThirds => 8,
            Width::Full => 12,
        }
    }

    #[cfg(test)]
    /// The width in pixels in a grid that is `available` pixels wide. A
    /// narrow window gives each block the whole row.
    pub fn pixels(self, available: f32, narrow: bool) -> f32 {
        span_pixels(self.columns(), available, narrow)
    }
}

/// The width in pixels of `cols` columns of the 12 column grid.
fn span_pixels(cols: u8, available: f32, narrow: bool) -> f32 {
    if narrow {
        return available;
    }
    let column = (available - GAP * (COLUMNS - 1.0)) / COLUMNS;
    let span = f32::from(cols);
    column * span + GAP * (span - 1.0)
}

/// One lane of a segment: its width in columns, and the indexes of its cards.
#[derive(Debug, PartialEq)]
struct Lane {
    cols: u8,
    items: Vec<usize>,
}

/// The first blocks of a segment that fill 12 columns. Each one starts a lane.
fn start_lanes(cols: &[u8], from: usize) -> Vec<Lane> {
    let mut lanes: Vec<Lane> = Vec::new();
    let mut sum = 0;
    for (i, &c) in cols.iter().enumerate().skip(from) {
        if c >= 12 || sum + c > 12 {
            break;
        }
        lanes.push(Lane {
            cols: c,
            items: vec![i],
        });
        sum += c;
        if sum == 12 {
            break;
        }
    }
    lanes
}

/// Put the blocks from `from` on, in order, into the next lane of the same
/// width. Returns the index of the first block that fits no lane.
fn fill_lanes(cols: &[u8], from: usize, lanes: &mut [Lane]) -> usize {
    let mut next = 0;
    for (i, &c) in cols.iter().enumerate().skip(from) {
        let count = lanes.len();
        let Some(at) = (0..count)
            .map(|step| (next + step) % count)
            .find(|&at| lanes[at].cols == c)
        else {
            return i;
        };
        lanes[at].items.push(i);
        next = (at + 1) % count;
    }
    cols.len()
}

/// Split the widths into segments of lanes. A full width block is a segment of
/// its own. The blocks keep their order inside a lane.
fn pack(cols: &[u8]) -> Vec<Vec<Lane>> {
    let mut segments = Vec::new();
    let mut i = 0;
    while i < cols.len() {
        if cols[i] >= 12 {
            segments.push(vec![Lane {
                cols: 12,
                items: vec![i],
            }]);
            i += 1;
            continue;
        }
        let mut lanes = start_lanes(cols, i);
        let started = lanes.len();
        i = fill_lanes(cols, i + started, &mut lanes);
        // A lone lane has no partner, because the partner hid. It takes the
        // full width, so no hole stands beside it.
        if let [only] = lanes.as_mut_slice() {
            only.cols = 12;
        }
        segments.push(lanes);
    }
    segments
}

/// The shared data that a block draws from.
#[allow(dead_code)] // The screens of the rewrite read the new fields.
pub struct Ctx<'a> {
    pub t: &'a Theme,
    pub today: &'a Today,
    /// Health and Sleep. `None` until they have loaded.
    pub wellness: Option<&'a Wellness>,
    /// The day or the range that the page looks at. The views above and below
    /// are built for `focus.end`. A block that shows a series must clip it to
    /// the range when the range is longer than one day.
    pub focus: Focus,
    /// `None` until the first read has ended, and when the read fails.
    pub charts_view: Option<&'a ChartsView>,
    pub tiles_view: Option<&'a TilesView>,
    pub extras_view: Option<&'a ExtrasView>,
    /// Food, Weight and Strength.
    pub body: &'a BodyData,
    /// Insights, Fitness and the report. `None` until they have loaded.
    pub training: Option<&'a Training>,
    /// The plan, the routes and the gear.
    pub places: &'a Places,
    /// The activities store, for a read that no view has.
    pub store: &'a Store,
    /// The list of the activities, read at start and after a sync.
    pub activities: &'a anyhow::Result<ActivitiesView>,
}

impl Root {
    /// The shared data of the window, for the blocks.
    pub(crate) fn ctx<'a>(&'a self, t: &'a Theme) -> Ctx<'a> {
        Ctx {
            t,
            today: &self.today,
            wellness: self.wellness.as_ref(),
            focus: self.focus,
            charts_view: self.views.charts.as_ref(),
            tiles_view: self.views.tiles.as_ref(),
            extras_view: self.views.extras.as_ref(),
            body: &self.body,
            training: self.training.as_ref(),
            places: &self.places,
            store: &self.store,
            activities: &self.list,
        }
    }
}

/// Where the link in the header of a block leads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Go {
    Screen(Screen),
    /// The detail of one session.
    Activity(i64),
    /// The table of every session, in pages.
    AllActivities,
}

/// The window, for a click in a block. A block has no `Context<Root>`.
pub struct RootHandle(pub gpui::WeakEntity<Root>);

impl gpui::Global for RootHandle {}

/// A click on a row that opens the detail of one session.
pub fn open_activity(id: i64) -> kit::RowClick {
    Box::new(move |_, cx| {
        let Some(root) = cx.try_global::<RootHandle>().map(|h| h.0.clone()) else {
            return;
        };
        let _ = root.update(cx, |root, cx| root.open_activity(id, cx));
    })
}

/// What a block returns: the header text and the body. The registry adds the
/// title, so a block never names itself.
pub struct Block {
    pub meta: Option<SharedString>,
    pub link: Option<(SharedString, Go)>,
    pub body: AnyElement,
}

/// A block in the registry.
#[allow(dead_code)] // The add panel reads `group` and `default_width`.
pub struct BlockDef {
    /// The saved name. It never changes.
    pub id: &'static str,
    pub title: &'static str,
    /// The screen that the block belongs to. The add panel groups by it.
    pub group: Screen,
    pub default_width: Width,
    /// `None` when the block has no data. The block then hides.
    pub render: fn(&Ctx) -> Option<Block>,
}

/// The blocks of each screen. One file for each screen lists its own blocks.
const ALL_BLOCKS: &[&[BlockDef]] = &[
    today::BLOCKS,
    health::BLOCKS,
    sleep::BLOCKS,
    food::BLOCKS,
    weight::BLOCKS,
    activities::BLOCKS,
    strength::BLOCKS,
    fitness::BLOCKS,
    plan::BLOCKS,
    routes::BLOCKS,
    gear::BLOCKS,
    insights::BLOCKS,
    reports::BLOCKS,
];

/// Every block. The add panel lists them.
pub fn registry() -> impl Iterator<Item = &'static BlockDef> {
    ALL_BLOCKS.iter().flat_map(|blocks| blocks.iter())
}

pub fn def(id: &str) -> Option<&'static BlockDef> {
    registry().find(|d| d.id == id)
}

/// What a screen file gives to the page of the screen.
pub struct ScreenDef {
    pub layout: &'static [(&'static str, Width)],
    pub title: fn(&Ctx) -> Option<String>,
    pub lede: fn(&Ctx) -> Option<String>,
    pub crumb: fn(&Ctx) -> Option<String>,
}

/// The block page of a screen. `None` for Ask and Settings, which have no blocks.
pub fn screen_def(screen: Screen) -> Option<ScreenDef> {
    macro_rules! def {
        ($m:ident) => {
            ScreenDef {
                layout: $m::LAYOUT,
                title: $m::title,
                lede: $m::lede,
                crumb: $m::crumb,
            }
        };
    }
    Some(match screen {
        Screen::Today => def!(today),
        Screen::Health => def!(health),
        Screen::Sleep => def!(sleep),
        Screen::Food => def!(food),
        Screen::Weight => def!(weight),
        Screen::Activities => def!(activities),
        Screen::Strength => def!(strength),
        Screen::Fitness => def!(fitness),
        Screen::Plan => def!(plan),
        Screen::Routes => def!(routes),
        Screen::Gear => def!(gear),
        Screen::Insights => def!(insights),
        Screen::Reports => def!(reports),
        Screen::Ask | Screen::Settings => return None,
    })
}

/// The default layout of a screen: block ids with a width each. Empty when
/// the screen still uses its old page.
pub fn default_layout(screen: Screen) -> &'static [(&'static str, Width)] {
    screen_def(screen).map_or(&[], |d| d.layout)
}

/// The breadcrumb text from the file of the screen, when it gives one.
pub fn crumb(screen: Screen, ctx: &Ctx) -> Option<String> {
    screen_def(screen).and_then(|d| (d.crumb)(ctx))
}

/// The grid for a layout. A block that returns `None`, and an id that no
/// block has, leave no hole. `available` is the width of the grid in pixels.
pub fn grid(
    layout: &[(&str, Width)],
    ctx: &Ctx,
    available: f32,
    narrow: bool,
    cx: &mut Context<Root>,
) -> Div {
    let mut cards: Vec<(u8, Div)> = Vec::new();
    for (n, (id, width)) in layout.iter().enumerate() {
        let Some(def) = def(id) else {
            continue;
        };
        let Some(block) = (def.render)(ctx) else {
            continue;
        };
        let link = block.link.map(|(label, go)| {
            kit::link(("block-link", n), label, ctx.t)
                .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.go(go, cx)))
        });
        let card = kit::card(def.title, block.meta, link, block.body, ctx.t);
        cards.push((width.columns(), card));
    }
    lay_out(cards, available, narrow)
}

/// The grid for cards that are built already, each with its width in columns.
pub fn lay_out(cards: Vec<(u8, Div)>, available: f32, narrow: bool) -> Div {
    let mut cards: Vec<(u8, Option<Div>)> =
        cards.into_iter().map(|(c, card)| (c, Some(card))).collect();
    let widths: Vec<u8> = cards.iter().map(|(c, _)| *c).collect();
    // A narrow window has one lane with every block, in order.
    let segments = if narrow {
        vec![vec![Lane {
            cols: 12,
            items: (0..cards.len()).collect(),
        }]]
    } else {
        pack(&widths)
    };
    let mut grid = div().flex().flex_col().gap(px(GAP));
    for segment in segments {
        let mut row = div().flex().items_stretch().gap(px(GAP));
        for lane in segment {
            let last = lane.items.len().saturating_sub(1);
            let mut column = div()
                .flex_none()
                .w(px(span_pixels(lane.cols, available, narrow)))
                .flex()
                .flex_col()
                .gap(px(GAP));
            for (at, i) in lane.items.into_iter().enumerate() {
                if let Some(card) = cards[i].1.take() {
                    // The last card grows, so the bottoms of the lanes line up.
                    column = column.child(card.when(at == last, |c| c.flex_grow(1.0)));
                }
            }
            row = row.child(column);
        }
        grid = grid.child(row);
    }
    grid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_widths_fill_a_row_of_twelve_columns() {
        let row = Width::TwoThirds.pixels(1080.0, false) + GAP + Width::Third.pixels(1080.0, false);
        assert!((row - 1080.0).abs() < 1e-3);
        let halves = 2.0 * Width::Half.pixels(1080.0, false) + GAP;
        assert!((halves - 1080.0).abs() < 1e-3);
        assert_eq!(Width::Full.pixels(1080.0, false), 1080.0);
        assert_eq!(Width::Third.pixels(500.0, true), 500.0);
    }

    fn items(segments: &[Vec<Lane>]) -> Vec<Vec<Vec<usize>>> {
        segments
            .iter()
            .map(|s| s.iter().map(|l| l.items.clone()).collect())
            .collect()
    }

    #[test]
    fn the_blocks_pack_into_lanes() {
        // 8,4,8,4 gives two lanes that take the blocks in turn.
        assert_eq!(items(&pack(&[8, 4, 8, 4])), [[vec![0, 2], vec![1, 3]]]);
        // A full block is a segment of its own.
        assert_eq!(
            items(&pack(&[12, 4, 4, 4, 12])),
            [
                vec![vec![0]],
                vec![vec![1], vec![2], vec![3]],
                vec![vec![4]]
            ]
        );
        // The first blocks that fill 12 columns set the lanes: 4,8 here.
        let lanes = pack(&[4, 8, 8, 4]);
        assert_eq!(lanes[0].iter().map(|l| l.cols).collect::<Vec<_>>(), [4, 8]);
        assert_eq!(items(&lanes), [[vec![0, 3], vec![1, 2]]]);
        // A width with no lane starts a new segment at that block.
        assert_eq!(
            items(&pack(&[6, 6, 4, 4, 4])),
            [vec![vec![0], vec![1]], vec![vec![2], vec![3], vec![4]]]
        );
        assert!(pack(&[]).is_empty());
        // A block whose partner hid takes the full width.
        assert_eq!(
            pack(&[6]).iter().map(|s| s[0].cols).collect::<Vec<_>>(),
            [12]
        );
    }

    #[test]
    fn the_ids_of_the_registry_are_unique() {
        let all: Vec<_> = registry().collect();
        for (i, d) in all.iter().enumerate() {
            assert!(all[i + 1..].iter().all(|o| o.id != d.id), "{}", d.id);
        }
    }

    #[test]
    fn every_layout_id_has_a_block() {
        for screen in Screen::ALL {
            for (id, _) in default_layout(screen) {
                assert!(def(id).is_some(), "{id}");
            }
        }
    }
}
