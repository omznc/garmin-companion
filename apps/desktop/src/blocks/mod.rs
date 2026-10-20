//! The blocks: the units that a screen is built from, and the registry that
//! lists them.
//!
//! A block draws from the shared data of the window (`Ctx`), never from the
//! state of one screen, so any screen can show any block. A block with no
//! data returns `None` and does not show. A screen is a list of block ids with
//! a width for each. The grid has 12 columns with a gap of 14 px.

mod today;

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, Div, SharedString};

use crate::data::Today;
use crate::kit;
use crate::screen::Screen;
use crate::shell::Root;
use crate::theme::Theme;
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

    /// The width in pixels in a grid that is `available` pixels wide. A
    /// narrow window gives each block the whole row.
    pub fn pixels(self, available: f32, narrow: bool) -> f32 {
        if narrow {
            return available;
        }
        let column = (available - GAP * (COLUMNS - 1.0)) / COLUMNS;
        let span = f32::from(self.columns());
        column * span + GAP * (span - 1.0)
    }
}

/// The shared data that a block draws from.
pub struct Ctx<'a> {
    pub t: &'a Theme,
    pub today: &'a Today,
    /// Health and Sleep. `None` until they have loaded.
    pub wellness: Option<&'a Wellness>,
}

/// Where the link in the header of a block leads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Go {
    Screen(Screen),
    /// The detail of one session.
    Activity(i64),
}

/// What a block returns: the header text and the body. The registry adds the
/// title, so a block never names itself.
pub struct Block {
    pub meta: Option<SharedString>,
    pub link: Option<(SharedString, Go)>,
    pub body: AnyElement,
}

/// A block in the registry.
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

/// Every block. Track 4 lists them in the add panel.
pub static REGISTRY: &[BlockDef] = &today::BLOCKS;

pub fn def(id: &str) -> Option<&'static BlockDef> {
    REGISTRY.iter().find(|d| d.id == id)
}

/// The default layout of a screen: block ids with a width each.
pub fn default_layout(screen: Screen) -> Vec<(&'static str, Width)> {
    REGISTRY
        .iter()
        .filter(|d| d.group == screen)
        .map(|d| (d.id, d.default_width))
        .collect()
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
    let mut grid = div().flex().flex_wrap().items_start().gap(px(GAP));
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
        grid = grid.child(
            div()
                .w(px(width.pixels(available, narrow)))
                .child(kit::card(def.title, block.meta, link, block.body, ctx.t)),
        );
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

    #[test]
    fn the_ids_of_the_registry_are_unique() {
        for (i, d) in REGISTRY.iter().enumerate() {
            assert!(REGISTRY[i + 1..].iter().all(|o| o.id != d.id), "{}", d.id);
        }
    }
}
