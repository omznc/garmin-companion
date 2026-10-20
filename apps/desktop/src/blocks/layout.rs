//! The layout of the Today screen that the user arranges: an ordered list of
//! block ids, each with a width. It has no view code, so the tests cover it.
//!
//! The list is saved in `window.json`. A saved id that no block has is dropped
//! on load, so a block can get a new id later.

use serde::{Deserialize, Serialize};

use super::{def, default_layout, Width};
use crate::screen::Screen;

/// One block in the saved file: the id and the width in columns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub id: String,
    pub cols: u8,
}

/// The width for a number of columns. `None` for a number that no width has.
fn width_of(cols: u8) -> Option<Width> {
    [Width::Third, Width::Half, Width::TwoThirds, Width::Full]
        .into_iter()
        .find(|w| w.columns() == cols)
}

/// The width after this one, from the widest to the narrowest and around.
pub fn next_width(width: Width) -> Width {
    match width {
        Width::Full => Width::TwoThirds,
        Width::TwoThirds => Width::Half,
        Width::Half => Width::Third,
        Width::Third => Width::Full,
    }
}

/// The text of a width in the size button and in the add panel.
pub fn width_label(width: Width) -> &'static str {
    match width {
        Width::Third => "1/3",
        Width::Half => "1/2",
        Width::TwoThirds => "2/3",
        Width::Full => "Full",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout(Vec<(String, Width)>);

impl Layout {
    /// The shipped order and widths of Today.
    pub fn standard() -> Self {
        Self(
            default_layout(Screen::Today)
                .iter()
                .map(|(id, width)| (id.to_string(), *width))
                .collect(),
        )
    }

    /// The layout from the saved list. No saved list gives the standard one.
    /// An unknown id, a repeated id and a bad width are dropped. A saved list
    /// that has nothing left gives the standard layout, but an empty list
    /// stays empty: the user removed every block.
    pub fn load(saved: Option<&[Saved]>) -> Self {
        let Some(saved) = saved else {
            return Self::standard();
        };
        let mut blocks: Vec<(String, Width)> = Vec::new();
        for entry in saved {
            let known = def(&entry.id).is_some();
            let placed = blocks.iter().any(|(id, _)| *id == entry.id);
            if let Some(width) = width_of(entry.cols).filter(|_| known && !placed) {
                blocks.push((entry.id.clone(), width));
            }
        }
        if blocks.is_empty() && !saved.is_empty() {
            return Self::standard();
        }
        Self(blocks)
    }

    pub fn to_saved(&self) -> Vec<Saved> {
        self.0
            .iter()
            .map(|(id, width)| Saved {
                id: id.clone(),
                cols: width.columns(),
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.0.iter().any(|(placed, _)| placed == id)
    }

    /// The blocks in order, for the grid.
    pub fn pairs(&self) -> Vec<(&str, Width)> {
        self.0
            .iter()
            .map(|(id, width)| (id.as_str(), *width))
            .collect()
    }

    /// Take the block at `from` out and put it at `to`. Returns the new
    /// place, or `None` when a place is out of range.
    pub fn move_to(&mut self, from: usize, to: usize) -> Option<usize> {
        if from >= self.0.len() || to >= self.0.len() {
            return None;
        }
        let block = self.0.remove(from);
        self.0.insert(to, block);
        Some(to)
    }

    /// Move the block by one step or more. A step past an end does nothing.
    pub fn move_by(&mut self, at: usize, step: isize) -> Option<usize> {
        let to = at.checked_add_signed(step)?;
        self.move_to(at, to)
    }

    /// Give the block the next width. Returns the new width.
    pub fn cycle_width(&mut self, at: usize) -> Option<Width> {
        let entry = self.0.get_mut(at)?;
        entry.1 = next_width(entry.1);
        Some(entry.1)
    }

    pub fn remove(&mut self, at: usize) -> bool {
        if at >= self.0.len() {
            return false;
        }
        self.0.remove(at);
        true
    }

    /// Add a block at the end, with its default width. A block that is
    /// placed already, and an unknown id, do nothing.
    pub fn add(&mut self, id: &str) -> bool {
        let Some(def) = def(id) else {
            return false;
        };
        if self.contains(id) {
            return false;
        }
        self.0.push((def.id.to_string(), def.default_width));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(layout: &Layout) -> Vec<&str> {
        layout.pairs().into_iter().map(|(id, _)| id).collect()
    }

    fn standard_ids() -> Vec<&'static str> {
        default_layout(Screen::Today).iter().map(|e| e.0).collect()
    }

    #[test]
    fn a_block_moves_to_a_place_and_by_a_step() {
        let mut layout = Layout::standard();
        let first = standard_ids()[0];
        assert_eq!(layout.move_to(0, 3), Some(3));
        assert_eq!(ids(&layout)[3], first);
        assert_eq!(layout.move_by(3, -1), Some(2));
        assert_eq!(ids(&layout)[2], first);
        // A step past an end and a place out of range do nothing.
        assert_eq!(layout.move_by(0, -1), None);
        assert_eq!(layout.move_by(layout.len() - 1, 1), None);
        assert_eq!(layout.move_to(0, 99), None);
        assert_eq!(layout.len(), standard_ids().len());
    }

    #[test]
    fn the_width_cycles_from_full_down_and_around() {
        let mut layout = Layout::standard();
        assert_eq!(layout.pairs()[0].1, Width::Full);
        let seen: Vec<_> = (0..4).filter_map(|_| layout.cycle_width(0)).collect();
        assert_eq!(
            seen,
            [Width::TwoThirds, Width::Half, Width::Third, Width::Full]
        );
        assert_eq!(layout.cycle_width(99), None);
    }

    #[test]
    fn a_block_is_removed_and_added_with_its_default_width() {
        let mut layout = Layout::standard();
        let first = standard_ids()[0];
        assert!(layout.remove(0));
        assert!(!layout.contains(first));
        assert!(!layout.remove(99));
        assert!(layout.add(first));
        assert_eq!(layout.pairs().last().copied(), Some((first, Width::Full)));
        // A block that is placed, and an unknown id, are not added.
        assert!(!layout.add(first));
        assert!(!layout.add("no_such_block"));
    }

    #[test]
    fn a_saved_layout_loads_without_unknown_blocks() {
        let saved = vec![
            Saved {
                id: "week".into(),
                cols: 12,
            },
            Saved {
                id: "renamed_away".into(),
                cols: 4,
            },
            Saved {
                id: "week".into(),
                cols: 4,
            },
            Saved {
                id: "fuel".into(),
                cols: 7,
            },
            Saved {
                id: "fuel".into(),
                cols: 6,
            },
        ];
        let layout = Layout::load(Some(&saved));
        assert_eq!(
            layout.pairs(),
            [("week", Width::Full), ("fuel", Width::Half)]
        );
        assert_eq!(Layout::load(Some(&layout.to_saved())), layout);
    }

    #[test]
    fn a_missing_file_gives_the_standard_layout_and_an_empty_one_stays() {
        assert_eq!(Layout::load(None), Layout::standard());
        assert!(Layout::load(Some(&[])).is_empty());
        let junk = [Saved {
            id: "gone".into(),
            cols: 4,
        }];
        assert_eq!(Layout::load(Some(&junk)), Layout::standard());
    }

    #[test]
    fn a_reset_gives_the_standard_layout_back() {
        let mut layout = Layout::standard();
        layout.remove(0);
        layout.cycle_width(0);
        layout = Layout::standard();
        assert_eq!(ids(&layout), standard_ids());
        assert_eq!(layout.to_saved().len(), standard_ids().len());
    }
}
