//! The blocks of the Activities screen. To build the screen, fill `BLOCKS`
//! and `LAYOUT`. The screen then uses the block page for the list. Edit only
//! this file.
//!
//! The detail page of one session (`Page::Detail` in `crate::activities`)
//! stays on the old code path. `Root::screen` in `shell.rs` skips the block
//! page when `Root::page` is not `Page::List`. The detail hook is there.

use super::{BlockDef, Ctx, Width};

/// The blocks that belong to this screen. Each `group` is `Screen::Activities`.
pub const BLOCKS: &[BlockDef] = &[];

/// The default order and width of the blocks. An empty list keeps the old page.
pub const LAYOUT: &[(&str, Width)] = &[];

/// The heading of the page. `None` uses the name of the screen.
pub fn title(_ctx: &Ctx) -> Option<String> {
    None
}

/// The line under the heading. `None` shows no line.
pub fn lede(_ctx: &Ctx) -> Option<String> {
    None
}

/// The text after the screen name in the breadcrumb bar. `None` uses the old text.
pub fn crumb(_ctx: &Ctx) -> Option<String> {
    None
}
