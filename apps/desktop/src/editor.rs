//! The model of a multi-line text field: a string, a caret and a selection.
//!
//! gpui has no text field. This is a small port of `editor.rs` from mbrd-rs.
//! It knows nothing about pixels. The one thing that needs a font, the width of
//! a character, comes in through [`Advance`]. So every rule here is tested with
//! plain strings.
//!
//! Offsets are byte offsets into a UTF-8 string. Every move goes through
//! [`Editor::step`] or [`Editor::boundary`], so the caret never lands inside a
//! character.
//!
//! Left out of the port, on purpose: undo, input methods (a composing keyboard
//! for Japanese or Korean), a length limit, and up/down by wrapped row (the
//! arrows walk the lines that Enter made).

/// What a key press meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// Dealt with. Keep editing.
    Held,
    /// Enter without Shift: send the text.
    Send,
    /// Not a text key. The caller may use it, for the clipboard for example.
    Ignored,
}

/// How wide a character is, in pixels, at a font size.
pub trait Advance {
    fn of(&self, c: char, size: f32) -> f32;

    fn width(&self, text: &str, size: f32) -> f32 {
        text.chars().map(|c| self.of(c, size)).sum()
    }
}

/// The modifiers that the editor cares about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    /// Ctrl on Linux and Windows, Command on macOS.
    pub secondary: bool,
    pub alt: bool,
    /// Move by words: Alt on macOS, Ctrl elsewhere.
    pub word: bool,
}

impl From<gpui::Modifiers> for Mods {
    fn from(m: gpui::Modifiers) -> Self {
        Self {
            shift: m.shift,
            secondary: m.secondary(),
            alt: m.alt,
            word: if cfg!(target_os = "macos") {
                m.alt
            } else {
                m.control
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Word,
    Punctuation,
}

fn class(c: char) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Punctuation
    }
}

#[derive(Debug, Clone, Default)]
pub struct Editor {
    text: String,
    /// Byte offset of the caret. Always on a character boundary.
    caret: usize,
    /// The other end of the selection. Equal to `caret` when nothing is selected.
    anchor: usize,
    /// The column that an up/down run started from, so a short line in the
    /// middle does not move the column.
    goal: Option<usize>,
}

impl Editor {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// Take the text out and leave the editor empty.
    pub fn take(&mut self) -> String {
        self.caret = 0;
        self.anchor = 0;
        self.goal = None;
        std::mem::take(&mut self.text)
    }

    /// The selection, low end first, or `None`.
    pub fn selection(&self) -> Option<(usize, usize)> {
        (self.caret != self.anchor)
            .then(|| (self.caret.min(self.anchor), self.caret.max(self.anchor)))
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(from, to)| &self.text[from..to])
    }

    /// The text as the visual rows of a box `width` pixels wide: one byte span
    /// per row. A row breaks at Enter and when the words run out of room.
    /// Every row holds at least one character.
    pub fn wrapped(&self, width: f32, size: f32, adv: &dyn Advance) -> Vec<(usize, usize)> {
        let width = width.max(0.0);
        let mut out = Vec::new();
        for (start, end) in self.line_spans() {
            let mut row = start;
            let mut used = 0.0f32;
            // The byte after the last space of this row, where a break prefers to land.
            let mut space: Option<usize> = None;
            for (at, c) in self.text[start..end].char_indices() {
                let at = start + at;
                let wide = adv.of(c, size);
                if c == ' ' {
                    used += wide;
                    space = Some(at + 1);
                    continue;
                }
                if used + wide > width && at > row {
                    let cut = space.take().filter(|&s| s > row).unwrap_or(at);
                    out.push((row, cut));
                    row = cut;
                    used = adv.width(&self.text[cut..at], size);
                }
                used += wide;
            }
            out.push((row, end));
        }
        out
    }

    /// The row of the caret, and its byte offset in that row. A caret on a soft
    /// break belongs to the row after it: that is where the next letter lands.
    pub fn caret_in(&self, rows: &[(usize, usize)]) -> (usize, usize) {
        let row = rows
            .iter()
            .rposition(|&(start, _)| start <= self.caret)
            .unwrap_or(0);
        (row, self.caret - rows[row].0)
    }

    /// The selection cut up by rows: `(row, start, end)` in bytes within each row.
    pub fn highlight_in(&self, rows: &[(usize, usize)]) -> Vec<(usize, usize, usize)> {
        let Some((from, to)) = self.selection() else {
            return Vec::new();
        };
        rows.iter()
            .enumerate()
            .filter_map(|(row, &(start, end))| {
                let (lit_from, lit_to) = (from.max(start), to.min(end));
                // `then`, not `then_some`: the subtraction needs the test first.
                (lit_from < lit_to).then(|| (row, lit_from - start, lit_to - start))
            })
            .collect()
    }

    /// Put the caret at a byte offset, held to a character boundary. For a
    /// mouse click, which is not wired yet.
    #[allow(dead_code)]
    pub fn place(&mut self, at: usize, extend: bool) {
        self.caret = self.boundary(at);
        if !extend {
            self.anchor = self.caret;
        }
        self.goal = None;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
        self.goal = None;
    }

    /// Take one key press. `text` is what the key would type, which is not the
    /// same as the key: Alt+S is one key and the letter that it types.
    pub fn key(&mut self, key: &str, mods: Mods, text: Option<&str>) -> Reply {
        match key {
            "enter" if mods.shift => self.insert("\n"),
            "enter" => return Reply::Send,
            "left" => self.walk(false, mods),
            "right" => self.walk(true, mods),
            "up" => self.vertical(-1, mods.shift),
            "down" => self.vertical(1, mods.shift),
            "home" => self.jump_to_edge(false, mods.shift),
            "end" => self.jump_to_edge(true, mods.shift),
            "backspace" => self.erase(false, mods.word),
            "delete" => self.erase(true, mods.word),
            "a" if mods.secondary => self.select_all(),
            "c" | "x" | "v" if mods.secondary => return Reply::Ignored,
            _ => {
                let Some(text) = text.filter(|t| !t.is_empty()) else {
                    return Reply::Ignored;
                };
                if mods.secondary || mods.alt || text.chars().all(char::is_control) {
                    return Reply::Ignored;
                }
                self.insert(text);
            }
        }
        Reply::Held
    }

    /// Put text in and replace the selection.
    pub fn insert(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if let Some((from, to)) = self.selection() {
            self.text.replace_range(from..to, "");
            self.caret = from;
        }
        self.text.insert_str(self.caret, &text);
        self.caret += text.len();
        self.anchor = self.caret;
        self.goal = None;
    }

    /// Delete the selection if there is one, else one character or one word.
    fn erase(&mut self, forward: bool, word: bool) {
        let (from, to) = self.selection().unwrap_or_else(|| {
            let to = if word {
                self.word_edge(forward)
            } else {
                self.step(self.caret, forward)
            };
            (self.caret.min(to), self.caret.max(to))
        });
        if from == to {
            return;
        }
        self.text.replace_range(from..to, "");
        self.caret = from;
        self.anchor = from;
        self.goal = None;
    }

    fn walk(&mut self, forward: bool, mods: Mods) {
        // An arrow with a selection collapses to the edge of it.
        if let (false, Some((from, to))) = (mods.shift, self.selection()) {
            self.caret = if forward { to } else { from };
            self.anchor = self.caret;
            self.goal = None;
            return;
        }
        self.caret = if mods.word {
            self.word_edge(forward)
        } else {
            self.step(self.caret, forward)
        };
        if !mods.shift {
            self.anchor = self.caret;
        }
        self.goal = None;
    }

    /// Up or down one line, which keeps the column.
    fn vertical(&mut self, by: i32, extend: bool) {
        let lines = self.line_spans();
        let (row, column) = self.caret_line();
        let column = self.goal.unwrap_or(column);
        let target = row as i32 + by;
        self.caret = if target < 0 || target as usize >= lines.len() {
            if by < 0 {
                0
            } else {
                self.text.len()
            }
        } else {
            let (start, end) = lines[target as usize];
            self.boundary((start + column).min(end))
        };
        if !extend {
            self.anchor = self.caret;
        }
        self.goal = Some(column);
    }

    fn jump_to_edge(&mut self, end: bool, extend: bool) {
        let (start, stop) = self.line_spans()[self.caret_line().0];
        self.caret = if end { stop } else { start };
        if !extend {
            self.anchor = self.caret;
        }
        self.goal = None;
    }

    /// The line of the caret and its column in bytes, with no wrap.
    fn caret_line(&self) -> (usize, usize) {
        let before = &self.text[..self.caret];
        let row = before.matches('\n').count();
        let column = before
            .rfind('\n')
            .map_or(self.caret, |at| self.caret - at - 1);
        (row, column)
    }

    /// Where each line starts and ends, newlines excluded.
    fn line_spans(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = 0;
        for (at, _) in self.text.match_indices('\n') {
            out.push((start, at));
            start = at + 1;
        }
        out.push((start, self.text.len()));
        out
    }

    fn step(&self, from: usize, forward: bool) -> usize {
        if forward {
            self.text[from..]
                .chars()
                .next()
                .map_or(from, |c| from + c.len_utf8())
        } else {
            self.text[..from]
                .chars()
                .next_back()
                .map_or(from, |c| from - c.len_utf8())
        }
    }

    /// The far side of the run of like characters next to the caret. Spaces
    /// first, then one run of one class.
    fn word_edge(&self, forward: bool) -> usize {
        let peek = |at: usize| -> Option<char> {
            if forward {
                self.text[at..].chars().next()
            } else {
                self.text[..at].chars().next_back()
            }
        };
        let mut at = self.caret;
        while peek(at).is_some_and(|c| class(c) == Class::Space) {
            at = self.step(at, forward);
        }
        let Some(run) = peek(at).map(class) else {
            return at;
        };
        while peek(at).is_some_and(|c| class(c) == run) {
            at = self.step(at, forward);
        }
        at
    }

    /// The nearest character boundary at or before `at`.
    fn boundary(&self, at: usize) -> usize {
        let mut at = at.min(self.text.len());
        while at > 0 && !self.text.is_char_boundary(at) {
            at -= 1;
        }
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character is ten pixels wide, so tests do not depend on a font.
    struct Ten;
    impl Advance for Ten {
        fn of(&self, _: char, _: f32) -> f32 {
            10.0
        }
    }

    fn plain() -> Mods {
        Mods::default()
    }

    fn shift() -> Mods {
        Mods {
            shift: true,
            ..Mods::default()
        }
    }

    fn word() -> Mods {
        Mods {
            word: true,
            ..Mods::default()
        }
    }

    fn type_in(e: &mut Editor, text: &str) {
        for c in text.chars() {
            let s = c.to_string();
            e.key(&s, plain(), Some(&s));
        }
    }

    fn rows(e: &Editor, columns: usize) -> Vec<&str> {
        e.wrapped(columns as f32 * 10.0, 16.0, &Ten)
            .into_iter()
            .map(|(a, b)| &e.text()[a..b])
            .collect()
    }

    #[test]
    fn typing_goes_in_at_the_caret() {
        let mut e = Editor::default();
        type_in(&mut e, "helo");
        e.key("left", plain(), None);
        type_in(&mut e, "l");
        assert_eq!(e.text(), "hello");
    }

    #[test]
    fn enter_sends_and_shift_enter_is_a_newline() {
        let mut e = Editor::default();
        type_in(&mut e, "a");
        assert_eq!(e.key("enter", shift(), None), Reply::Held);
        type_in(&mut e, "b");
        assert_eq!(e.text(), "a\nb");
        assert_eq!(e.key("enter", plain(), None), Reply::Send);
        assert_eq!(e.text(), "a\nb");
    }

    #[test]
    fn take_leaves_the_editor_empty() {
        let mut e = Editor::default();
        type_in(&mut e, "hi");
        assert_eq!(e.take(), "hi");
        assert!(e.is_blank());
        type_in(&mut e, "x");
        assert_eq!(e.text(), "x");
    }

    #[test]
    fn backspace_removes_a_whole_character() {
        let mut e = Editor::default();
        type_in(&mut e, "ač");
        e.key("backspace", plain(), None);
        assert_eq!(e.text(), "a");
    }

    #[test]
    fn a_paste_normalises_line_ends_and_replaces_the_selection() {
        let mut e = Editor::default();
        type_in(&mut e, "old");
        e.select_all();
        e.insert("a\r\nb\rc");
        assert_eq!(e.text(), "a\nb\nc");
    }

    #[test]
    fn arrows_collapse_a_selection_to_its_edge() {
        let mut e = Editor::default();
        type_in(&mut e, "abc");
        e.select_all();
        e.key("left", plain(), None);
        type_in(&mut e, "x");
        assert_eq!(e.text(), "xabc");
    }

    #[test]
    fn shift_and_an_arrow_extend_the_selection() {
        let mut e = Editor::default();
        type_in(&mut e, "abc");
        e.key("left", shift(), None);
        e.key("left", shift(), None);
        assert_eq!(e.selected_text(), Some("bc"));
        e.key("backspace", plain(), None);
        assert_eq!(e.text(), "a");
    }

    #[test]
    fn a_word_step_stops_at_punctuation() {
        let mut e = Editor::default();
        type_in(&mut e, "foo.bar baz");
        e.key("left", word(), None);
        e.key("backspace", plain(), None);
        assert_eq!(e.text(), "foo.barbaz");
        e.key("left", word(), None);
        e.key("left", word(), None);
        type_in(&mut e, "|");
        assert_eq!(e.text(), "foo|.barbaz");
    }

    #[test]
    fn up_and_down_keep_the_column() {
        let mut e = Editor::default();
        type_in(&mut e, "abcd");
        e.key("enter", shift(), None);
        type_in(&mut e, "x");
        e.key("enter", shift(), None);
        type_in(&mut e, "efgh");
        e.key("up", plain(), None);
        e.key("up", plain(), None);
        e.key("down", plain(), None);
        e.key("down", plain(), None);
        type_in(&mut e, "!");
        assert_eq!(e.text(), "abcd\nx\nefgh!");
    }

    #[test]
    fn home_and_end_are_about_the_line() {
        let mut e = Editor::default();
        type_in(&mut e, "ab");
        e.key("enter", shift(), None);
        type_in(&mut e, "cd");
        e.key("home", plain(), None);
        type_in(&mut e, ">");
        e.key("end", plain(), None);
        type_in(&mut e, "<");
        assert_eq!(e.text(), "ab\n>cd<");
    }

    #[test]
    fn a_control_character_is_not_typed() {
        let mut e = Editor::default();
        assert_eq!(e.key("tab", plain(), Some("\t")), Reply::Ignored);
        assert_eq!(e.text(), "");
    }

    #[test]
    fn the_clipboard_keys_are_handed_back() {
        let secondary = Mods {
            secondary: true,
            ..Mods::default()
        };
        let mut e = Editor::default();
        assert_eq!(e.key("v", secondary, Some("v")), Reply::Ignored);
        assert_eq!(e.key("a", secondary, Some("a")), Reply::Held);
    }

    #[test]
    fn rows_break_at_enter_and_where_the_room_ends() {
        let mut e = Editor::default();
        type_in(&mut e, "aaa bbb ccc");
        e.key("enter", shift(), None);
        type_in(&mut e, "d");
        assert_eq!(rows(&e, 8), vec!["aaa bbb ", "ccc", "d"]);
    }

    #[test]
    fn a_word_longer_than_the_row_is_cut() {
        let mut e = Editor::default();
        type_in(&mut e, "abcdefghij");
        assert_eq!(rows(&e, 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn a_row_always_holds_a_character() {
        let mut e = Editor::default();
        type_in(&mut e, "abc");
        assert_eq!(rows(&e, 0), vec!["a", "b", "c"]);
    }

    #[test]
    fn the_caret_on_a_soft_break_waits_on_the_next_row() {
        let mut e = Editor::default();
        type_in(&mut e, "abcdefgh");
        let rows = e.wrapped(40.0, 16.0, &Ten);
        e.place(4, false);
        assert_eq!(e.caret_in(&rows), (1, 0));
    }

    #[test]
    fn a_selection_over_two_rows_is_two_pieces() {
        let mut e = Editor::default();
        type_in(&mut e, "abcdefgh");
        let rows = e.wrapped(40.0, 16.0, &Ten);
        e.place(2, false);
        e.place(6, true);
        assert_eq!(e.highlight_in(&rows), vec![(0, 2, 4), (1, 0, 2)]);
    }

    #[test]
    fn a_click_inside_a_character_lands_on_its_edge() {
        let mut e = Editor::default();
        type_in(&mut e, "č");
        e.place(1, false);
        type_in(&mut e, "x");
        assert_eq!(e.text(), "xč");
    }
}
