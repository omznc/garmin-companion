//! A single line text field. gpui ships no text input.
//!
//! This one is small on purpose. It has a caret, paste, a password mask and
//! the usual keys. It has no selection, no undo and no IME composition. Use it
//! for a path, a name or a key. A chat box needs a bigger editor.
//!
//! `Line` is the model. It knows no pixels, so the unit tests need no window.

use gpui::{
    div, prelude::*, px, relative, App, ClickEvent, Context, FocusHandle, KeyDownEvent, Window,
};

use crate::theme::{Theme, FS_SMALL};
use crate::ui::{MONO, SANS};

/// The text and the caret. The caret is a count of characters, so it never
/// falls inside a multi-byte character.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Line {
    text: String,
    caret: usize,
}

/// What a key meant for the owner of the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Edited,
    Submit,
    Ignored,
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

    /// Put text at the caret. Control characters and line breaks are dropped,
    /// so a pasted block stays on one line.
    pub fn insert(&mut self, text: &str) {
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        let at = self.byte(self.caret);
        self.text.insert_str(at, &clean);
        self.caret += clean.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.caret > 0 {
            self.caret -= 1;
            let at = self.byte(self.caret);
            self.text.remove(at);
        }
    }

    pub fn delete(&mut self) {
        if self.caret < self.len() {
            let at = self.byte(self.caret);
            self.text.remove(at);
        }
    }

    /// Apply one key. `paste` is the clipboard text, read by the caller.
    pub fn key(&mut self, key: &str, key_char: Option<&str>, command: bool, paste: &str) -> Reply {
        if key == "enter" {
            return Reply::Submit;
        }
        if self.step(key) {
            return Reply::Edited;
        }
        match (command, key, key_char) {
            (true, "v", _) => self.insert(paste),
            (true, _, _) => return Reply::Ignored,
            (false, _, Some(text)) if !text.is_empty() => self.insert(text),
            _ => return Reply::Ignored,
        }
        Reply::Edited
    }

    /// The keys that edit or move without text. False when `key` is not one.
    fn step(&mut self, key: &str) -> bool {
        match key {
            "backspace" => self.backspace(),
            "delete" => self.delete(),
            "left" => self.caret = self.caret.saturating_sub(1),
            "right" => self.caret = (self.caret + 1).min(self.len()),
            "home" => self.caret = 0,
            "end" => self.caret = self.len(),
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
        }
    }

    fn shown(&self) -> String {
        if self.mask {
            "\u{2022}".repeat(self.line.len())
        } else {
            self.line.text.clone()
        }
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
        let shown = self.shown();
        let caret_at = self
            .shown()
            .char_indices()
            .nth(self.line.caret)
            .map_or(shown.len(), |(i, _)| i);
        let (before, after) = shown.split_at(caret_at);
        let (before, after) = (before.to_string(), after.to_string());
        let acc = t.acc;
        let empty = shown.is_empty();

        let mut row = div()
            .flex()
            .items_center()
            .font_family(if self.mask || self.sans { SANS } else { MONO })
            .text_size(px(FS_SMALL))
            .line_height(relative(1.3))
            .text_color(t.fg)
            .child(before);
        if focused {
            row = row.child(div().w(px(1.5)).h(px(17.0)).bg(acc));
        }
        row = row.child(after);
        if empty && !focused {
            row = row.child(div().text_color(t.faint).child(self.placeholder));
        } else if empty {
            row = row.child(
                div()
                    .ml(px(6.0))
                    .text_color(t.faint)
                    .child(self.placeholder),
            );
        }

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
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                get(view).focus.focus(window, cx);
                cx.notify();
            }))
            .on_key_down(cx.listener(move |view, ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                let paste = cx
                    .read_from_clipboard()
                    .and_then(|c| c.text())
                    .unwrap_or_default();
                let field = get(view);
                match field.line.key(
                    &k.key,
                    k.key_char.as_deref(),
                    k.modifiers.secondary(),
                    &paste,
                ) {
                    Reply::Submit => submit(view, window, cx),
                    Reply::Edited => cx.notify(),
                    Reply::Ignored => {}
                }
            }))
            .child(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> Line {
        let mut line = Line::default();
        line.insert(text);
        line
    }

    #[test]
    fn typing_goes_in_at_the_caret() {
        let mut line = typed("ac");
        line.key("left", None, false, "");
        line.key("b", Some("b"), false, "");
        assert_eq!((line.text(), line.caret()), ("abc", 2));
    }

    #[test]
    fn the_caret_counts_characters_not_bytes() {
        let mut line = typed("šđ");
        assert_eq!(line.caret(), 2);
        line.backspace();
        assert_eq!((line.text(), line.caret()), ("š", 1));
        line.key("home", None, false, "");
        line.delete();
        assert_eq!(line.text(), "");
    }

    #[test]
    fn a_paste_stays_on_one_line() {
        let mut line = Line::default();
        assert_eq!(line.key("v", Some("v"), true, "a\nb\r\tc"), Reply::Edited);
        assert_eq!(line.text(), "abc");
    }

    #[test]
    fn other_shortcuts_and_enter_are_not_text() {
        let mut line = typed("x");
        assert_eq!(line.key("c", Some("c"), true, ""), Reply::Ignored);
        assert_eq!(line.key("enter", None, false, ""), Reply::Submit);
        assert_eq!(line.key("escape", None, false, ""), Reply::Ignored);
        assert_eq!(line.text(), "x");
    }

    #[test]
    fn the_keys_stop_at_the_ends() {
        let mut line = typed("a");
        line.key("right", None, false, "");
        line.key("end", None, false, "");
        assert_eq!(line.caret(), 1);
        line.key("home", None, false, "");
        line.key("left", None, false, "");
        line.backspace();
        assert_eq!((line.text(), line.caret()), ("a", 0));
    }
}
