//! A single line text field. gpui ships no text input.
//!
//! This one is small on purpose. It has a caret, a selection, copy, cut,
//! paste, a password mask and the usual keys. It has no undo and no IME
//! composition. A long text does not scroll. Use it for a path, a name or a
//! key. A chat box needs a bigger editor.
//!
//! `Line` is the model. It knows no pixels, so the unit tests need no window.

use std::{cell::Cell, rc::Rc};

use gpui::{
    canvas, div, font, prelude::*, px, relative, App, ClipboardItem, Context, Div, FocusHandle,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, TextRun,
    Window,
};

use crate::theme::{Theme, FS_SMALL};
use crate::ui::{MONO, SANS};

/// The text, the caret and the selection. The caret and the anchor are counts
/// of characters, so they never fall inside a multi-byte character. The
/// selection is the span between the anchor and the caret.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Line {
    text: String,
    caret: usize,
    anchor: Option<usize>,
}

/// What a key meant for the owner of the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Edited,
    Submit,
    /// Put this text on the clipboard. A cut has already removed it.
    Clip(String),
    Ignored,
}

/// The modifiers that are held, and whether the field hides its text.
#[derive(Debug, Clone, Copy, Default)]
pub struct Held {
    pub command: bool,
    pub shift: bool,
    /// A masked field never copies or cuts.
    pub mask: bool,
}

impl Line {
    pub fn text(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    fn byte(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map_or(self.text.len(), |(i, _)| i)
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// The selected span as (start, end) in characters. None when it is empty.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor.filter(|a| *a != self.caret)?;
        Some((anchor.min(self.caret), anchor.max(self.caret)))
    }

    fn selected(&self) -> &str {
        self.selection()
            .map_or("", |(a, b)| &self.text[self.byte(a)..self.byte(b)])
    }

    /// Remove the selection. True when there was one.
    fn remove_selection(&mut self) -> bool {
        let span = self.selection();
        self.anchor = None;
        let Some((a, b)) = span else { return false };
        let (from, to) = (self.byte(a), self.byte(b));
        self.text.replace_range(from..to, "");
        self.caret = a;
        true
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.len();
    }

    /// Move the caret. `extend` keeps or starts a selection.
    fn go(&mut self, to: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = to.min(self.len());
    }

    /// A mouse press at a character index. Shift extends the selection.
    pub fn press(&mut self, at: usize, extend: bool) {
        if extend {
            self.go(at, true);
        } else {
            self.caret = at.min(self.len());
            self.anchor = Some(self.caret);
        }
    }

    /// A mouse drag to a character index. The press set the anchor.
    pub fn drag(&mut self, at: usize) {
        self.caret = at.min(self.len());
    }

    /// Put text at the caret. Control characters and line breaks are dropped,
    /// so a pasted block stays on one line. The text replaces the selection.
    pub fn insert(&mut self, text: &str) {
        self.remove_selection();
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        let at = self.byte(self.caret);
        self.text.insert_str(at, &clean);
        self.caret += clean.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.remove_selection() {
            return;
        }
        if self.caret > 0 {
            self.caret -= 1;
            let at = self.byte(self.caret);
            self.text.remove(at);
        }
    }

    pub fn delete(&mut self) {
        if self.remove_selection() {
            return;
        }
        if self.caret < self.len() {
            let at = self.byte(self.caret);
            self.text.remove(at);
        }
    }

    /// Apply one key. `paste` is the clipboard text, read by the caller.
    pub fn key(&mut self, key: &str, key_char: Option<&str>, held: Held, paste: &str) -> Reply {
        if key == "enter" {
            return Reply::Submit;
        }
        if self.step(key, held.shift) {
            return Reply::Edited;
        }
        if held.command {
            return self.shortcut(key, held.mask, paste);
        }
        match key_char {
            Some(text) if !text.is_empty() => {
                self.insert(text);
                Reply::Edited
            }
            _ => Reply::Ignored,
        }
    }

    fn shortcut(&mut self, key: &str, mask: bool, paste: &str) -> Reply {
        match key {
            "v" => self.insert(paste),
            "a" => self.select_all(),
            "c" | "x" if !mask && self.selection().is_some() => {
                let copied = self.selected().to_string();
                if key == "x" {
                    self.remove_selection();
                }
                return Reply::Clip(copied);
            }
            _ => return Reply::Ignored,
        }
        Reply::Edited
    }

    /// The keys that edit or move without text. False when `key` is not one.
    fn step(&mut self, key: &str, shift: bool) -> bool {
        match key {
            "backspace" => self.backspace(),
            "delete" => self.delete(),
            "left" => match self.selection() {
                Some((start, _)) if !shift => self.go(start, false),
                _ => self.go(self.caret.saturating_sub(1), shift),
            },
            "right" => match self.selection() {
                Some((_, end)) if !shift => self.go(end, false),
                _ => self.go(self.caret + 1, shift),
            },
            "home" => self.go(0, shift),
            "end" => self.go(self.len(), shift),
            _ => return false,
        }
        true
    }
}

/// A field that a view owns. The owner passes `get` to find it again.
pub struct Field {
    pub line: Line,
    pub focus: FocusHandle,
    pub placeholder: &'static str,
    /// Draw dots in place of the characters.
    pub mask: bool,
    /// Plain text, such as a search, in the sans face. A key or a token stays mono.
    pub sans: bool,
    /// The left padding in pixels. An icon that stands over the field needs more.
    pub pad_left: f32,
    /// The left edge of the text on the window. The last paint sets it.
    origin: Rc<Cell<Pixels>>,
    /// True while the left button is down after a press in the field.
    dragging: bool,
}

impl Field {
    pub fn new(placeholder: &'static str, mask: bool, cx: &mut App) -> Self {
        Self {
            line: Line::default(),
            focus: cx.focus_handle(),
            placeholder,
            mask,
            sans: false,
            pad_left: 12.0,
            origin: Rc::new(Cell::new(px(0.0))),
            dragging: false,
        }
    }

    fn shown(&self) -> String {
        if self.mask {
            "\u{2022}".repeat(self.line.len())
        } else {
            self.line.text.clone()
        }
    }

    fn family(&self) -> &'static str {
        if self.mask || self.sans {
            SANS
        } else {
            MONO
        }
    }

    /// The character index under a window x position.
    fn index_at(&self, x: Pixels, window: &Window) -> usize {
        let shown = self.shown();
        if shown.is_empty() {
            return 0;
        }
        let run = TextRun {
            len: shown.len(),
            font: font(self.family()),
            color: Default::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
            letter_spacing: None,
        };
        let shaped = window
            .text_system()
            .shape_line(shown.clone().into(), px(FS_SMALL), &[run], None);
        let byte = shaped.closest_index_for_x(x - self.origin.get());
        shown[..byte.min(shown.len())].chars().count()
    }

    /// The text row: before, the selection, after, and the caret.
    fn text_row(&self, t: &Theme, focused: bool) -> Div {
        let shown = self.shown();
        let at = |chars: usize| {
            shown
                .char_indices()
                .nth(chars)
                .map_or(shown.len(), |(i, _)| i)
        };
        let (start, end) = match self.line.selection() {
            Some(span) if focused => span,
            _ => (self.line.caret, self.line.caret),
        };
        let (a, b) = (at(start), at(end));
        let caret = || div().w(px(1.5)).mx(px(-0.75)).h(px(17.0)).bg(t.acc);
        let mut row = div()
            .relative()
            .flex()
            .items_center()
            .font_family(self.family())
            .text_size(px(FS_SMALL))
            .line_height(relative(1.3))
            .text_color(t.fg)
            .child(shown[..a].to_string());
        let caret_first = self.line.caret == start;
        if focused && caret_first {
            row = row.child(caret());
        }
        row = row.child(div().bg(t.acctint).child(shown[a..b].to_string()));
        if focused && !caret_first {
            row = row.child(caret());
        }
        row = row.child(shown[b..].to_string());
        if shown.is_empty() {
            let gap = if focused { 6.0 } else { 0.0 };
            row = row.child(
                div()
                    .ml(px(gap))
                    .text_color(t.faint)
                    .child(self.placeholder),
            );
        }
        let origin = self.origin.clone();
        row.child(
            canvas(
                |_, _, _| (),
                move |bounds, _, _, _| origin.set(bounds.origin.x),
            )
            .absolute()
            .size_full(),
        )
    }

    /// Draw the field. `submit` runs when the user presses Enter.
    pub fn render<V: 'static>(
        &self,
        id: &'static str,
        t: &Theme,
        cx: &mut Context<V>,
        window: &Window,
        get: fn(&mut V) -> &mut Field,
        submit: fn(&mut V, &mut Window, &mut Context<V>),
    ) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        div()
            .id(id)
            .track_focus(&self.focus)
            .cursor_text()
            .w_full()
            .px(px(12.0))
            .pl(px(self.pad_left))
            .py(px(9.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(if focused { t.acc } else { t.line })
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, ev: &MouseDownEvent, window, cx| {
                    let field = get(view);
                    field.focus.focus(window, cx);
                    let at = field.index_at(ev.position.x, window);
                    if ev.click_count >= 2 {
                        field.line.select_all();
                    } else {
                        field.line.press(at, ev.modifiers.shift);
                    }
                    field.dragging = true;
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |view, ev: &MouseMoveEvent, window, cx| {
                let field = get(view);
                if field.dragging && ev.pressed_button == Some(MouseButton::Left) {
                    let at = field.index_at(ev.position.x, window);
                    field.line.drag(at);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseUpEvent, _, _| get(view).dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseUpEvent, _, _| get(view).dragging = false),
            )
            .on_key_down(cx.listener(move |view, ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                let paste = cx
                    .read_from_clipboard()
                    .and_then(|c| c.text())
                    .unwrap_or_default();
                let field = get(view);
                let held = Held {
                    command: k.modifiers.secondary(),
                    shift: k.modifiers.shift,
                    mask: field.mask,
                };
                match field.line.key(&k.key, k.key_char.as_deref(), held, &paste) {
                    Reply::Submit => submit(view, window, cx),
                    Reply::Edited => cx.notify(),
                    Reply::Clip(text) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        cx.notify();
                    }
                    Reply::Ignored => {}
                }
            }))
            .child(self.text_row(t, focused))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Held = Held {
        command: false,
        shift: false,
        mask: false,
    };
    const SHIFT: Held = Held { shift: true, ..PLAIN };
    const CMD: Held = Held {
        command: true,
        ..PLAIN
    };

    fn typed(text: &str) -> Line {
        let mut line = Line::default();
        line.insert(text);
        line
    }

    #[test]
    fn typing_goes_in_at_the_caret() {
        let mut line = typed("ac");
        line.key("left", None, PLAIN, "");
        line.key("b", Some("b"), PLAIN, "");
        assert_eq!((line.text(), line.caret()), ("abc", 2));
    }

    #[test]
    fn the_caret_counts_characters_not_bytes() {
        let mut line = typed("šđ");
        assert_eq!(line.caret(), 2);
        line.backspace();
        assert_eq!((line.text(), line.caret()), ("š", 1));
        line.key("home", None, PLAIN, "");
        line.delete();
        assert_eq!(line.text(), "");
    }

    #[test]
    fn a_paste_stays_on_one_line() {
        let mut line = Line::default();
        assert_eq!(line.key("v", Some("v"), CMD, "a\nb\r\tc"), Reply::Edited);
        assert_eq!(line.text(), "abc");
    }

    #[test]
    fn other_shortcuts_and_enter_are_not_text() {
        let mut line = typed("x");
        assert_eq!(line.key("z", Some("z"), CMD, ""), Reply::Ignored);
        assert_eq!(line.key("enter", None, PLAIN, ""), Reply::Submit);
        assert_eq!(line.key("escape", None, PLAIN, ""), Reply::Ignored);
        assert_eq!(line.text(), "x");
    }

    #[test]
    fn the_keys_stop_at_the_ends() {
        let mut line = typed("a");
        line.key("right", None, PLAIN, "");
        line.key("end", None, PLAIN, "");
        assert_eq!(line.caret(), 1);
        line.key("home", None, PLAIN, "");
        line.key("left", None, PLAIN, "");
        line.backspace();
        assert_eq!((line.text(), line.caret()), ("a", 0));
    }

    #[test]
    fn select_all_then_type_replaces_the_text() {
        let mut line = typed("hello");
        assert_eq!(line.key("a", Some("a"), CMD, ""), Reply::Edited);
        assert_eq!(line.selection(), Some((0, 5)));
        line.key("x", Some("x"), PLAIN, "");
        assert_eq!((line.text(), line.caret()), ("x", 1));
        assert_eq!(line.selection(), None);
    }

    #[test]
    fn shift_arrows_select_and_backspace_removes() {
        let mut line = typed("abcd");
        line.key("left", None, SHIFT, "");
        line.key("left", None, SHIFT, "");
        assert_eq!(line.selection(), Some((2, 4)));
        line.key("backspace", None, PLAIN, "");
        assert_eq!((line.text(), line.caret()), ("ab", 2));
        line.key("home", None, SHIFT, "");
        assert_eq!(line.selection(), Some((0, 2)));
        line.key("delete", None, PLAIN, "");
        assert_eq!((line.text(), line.caret()), ("", 0));
    }

    #[test]
    fn left_and_right_collapse_a_selection() {
        let mut line = typed("abcd");
        line.key("home", None, PLAIN, "");
        line.key("right", None, SHIFT, "");
        line.key("right", None, SHIFT, "");
        line.key("right", None, PLAIN, "");
        assert_eq!((line.selection(), line.caret()), (None, 2));
        line.key("end", None, SHIFT, "");
        line.key("left", None, PLAIN, "");
        assert_eq!((line.selection(), line.caret()), (None, 2));
    }

    #[test]
    fn copy_returns_the_selection_and_cut_removes_it() {
        let mut line = typed("abcd");
        line.key("left", None, SHIFT, "");
        line.key("left", None, SHIFT, "");
        assert_eq!(line.key("c", Some("c"), CMD, ""), Reply::Clip("cd".into()));
        assert_eq!(line.text(), "abcd");
        assert_eq!(line.key("x", Some("x"), CMD, ""), Reply::Clip("cd".into()));
        assert_eq!(line.text(), "ab");
        assert_eq!(line.key("c", Some("c"), CMD, ""), Reply::Ignored);
    }

    #[test]
    fn a_masked_field_does_not_copy_or_cut() {
        let held = Held {
            mask: true,
            ..CMD
        };
        let mut line = typed("secret");
        line.select_all();
        assert_eq!(line.key("c", Some("c"), held, ""), Reply::Ignored);
        assert_eq!(line.key("x", Some("x"), held, ""), Reply::Ignored);
        assert_eq!(line.text(), "secret");
    }

    #[test]
    fn a_press_places_the_caret_and_a_drag_selects() {
        let mut line = typed("abcd");
        line.press(1, false);
        assert_eq!((line.selection(), line.caret()), (None, 1));
        line.drag(3);
        assert_eq!(line.selection(), Some((1, 3)));
        line.press(0, true);
        assert_eq!(line.selection(), Some((0, 1)));
    }
}
