//! The docked message box of the Ask screen. From `Composer.tsx` and the
//! `.composer` rules in `styles.css`.
//!
//! Enter sends. Shift+Enter makes a new line. Ctrl+V pastes, Ctrl+C and Ctrl+X
//! use the clipboard. The text model is in `editor.rs`. This file draws it and
//! reads the keys.
//!
//! Not here yet: placing the caret with the mouse, selecting with the mouse,
//! a blinking caret, and input methods for composing keyboards.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    div, font, prelude::*, px, relative, App, BoxShadow, ClickEvent, ClipboardItem, ColorExt as _,
    Context, EventEmitter, FocusHandle, Focusable, Font, FontId, Hsla, IntoElement, KeyDownEvent,
    MouseDownEvent, Render, SharedString, TextRun, TextSystem, Window,
};

use crate::editor::{Advance, Editor, Mods, Reply};
use crate::icons::{icon, Icon};
use crate::theme::{Palette, Theme, FS_MD};
use crate::ui::SANS;

/// Rows of text that show before the box scrolls.
const MAX_ROWS: usize = 7;
const LINE_H: f32 = FS_MD * 1.55;
/// Padding and the button, as in `.composer-box`: 14 left, 8 right, 34 button, 8 gap, 2 border.
const CHROME_W: f32 = 14.0 + 8.0 + 34.0 + 8.0 + 2.0;

pub enum ComposerEvent {
    Send(String),
    Stop,
}

pub struct Composer {
    editor: Editor,
    focus: FocusHandle,
    /// A turn runs. The button becomes Stop and Enter does not send.
    busy: bool,
    /// The first row on screen, when the text is taller than the box.
    top: usize,
    want_focus: bool,
    palette: Palette,
    /// The width of the reading column, from the last render of the screen.
    column_w: f32,
}

impl EventEmitter<ComposerEvent> for Composer {}

impl Focusable for Composer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Character widths from the text system. The cache keeps the lookups cheap.
struct Measure {
    system: Arc<TextSystem>,
    font_id: FontId,
    cache: RefCell<HashMap<char, f32>>,
}

impl Advance for Measure {
    fn of(&self, c: char, size: f32) -> f32 {
        // The cache is for one size: the composer has one.
        *self.cache.borrow_mut().entry(c).or_insert_with(|| {
            self.system
                .advance(self.font_id, px(size), c)
                .map_or(size * 0.5, |s| f32::from(s.width))
        })
    }
}

/// The x of a byte offset inside a row, with the kerning that the text has.
fn x_of(window: &Window, row: &str, upto: usize, font: &Font, colour: Hsla) -> f32 {
    if upto == 0 {
        return 0.0;
    }
    let prefix = &row[..upto];
    let run = TextRun {
        len: prefix.len(),
        font: font.clone(),
        color: colour,
        background_color: None,
        underline: None,
        strikethrough: None,
        letter_spacing: None,
    };
    let line = window.text_system().shape_line(
        SharedString::from(prefix.to_string()),
        px(FS_MD),
        &[run],
        None,
    );
    f32::from(line.width)
}

impl Composer {
    pub fn new(palette: Palette, cx: &mut Context<Self>) -> Self {
        Self {
            editor: Editor::default(),
            focus: cx.focus_handle(),
            busy: false,
            top: 0,
            want_focus: true,
            palette,
            column_w: 720.0,
        }
    }

    pub fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        cx.notify();
    }

    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }

    pub fn set_column_width(&mut self, width: f32) {
        self.column_w = width;
    }

    /// Put text in the box without sending it, for a suggested question.
    pub fn fill(&mut self, text: &str, cx: &mut Context<Self>) {
        self.editor.select_all();
        self.editor.insert(text);
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            cx.emit(ComposerEvent::Stop);
        } else if !self.editor.is_blank() {
            let text = self.editor.take();
            self.top = 0;
            cx.emit(ComposerEvent::Send(text.trim().to_string()));
        }
        cx.notify();
    }

    fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let stroke = &event.keystroke;
        let mods = Mods::from(stroke.modifiers);
        if stroke.key == "escape" && self.busy {
            cx.emit(ComposerEvent::Stop);
            return;
        }
        match self
            .editor
            .key(&stroke.key, mods, stroke.key_char.as_deref())
        {
            Reply::Send => self.submit(cx),
            Reply::Held => cx.notify(),
            Reply::Ignored if mods.secondary => self.clipboard(&stroke.key, cx),
            Reply::Ignored => {}
        }
    }

    fn clipboard(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            "v" => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    self.editor.insert(&text);
                }
            }
            "c" | "x" => {
                if let Some(text) = self.editor.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                    if key == "x" {
                        self.editor.insert("");
                    }
                }
            }
            _ => return,
        }
        cx.notify();
    }
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.want_focus) {
            window.focus(&self.focus, cx);
        }
        let t = self.palette.theme();
        let focused = self.focus.is_focused(window);
        let face = font(SANS);
        let measure = Measure {
            system: cx.text_system().clone(),
            font_id: cx.text_system().resolve_font(&face),
            cache: RefCell::default(),
        };
        let rows = self
            .editor
            .wrapped(self.column_w - CHROME_W, FS_MD, &measure);
        let (caret_row, caret_at) = self.editor.caret_in(&rows);
        let shown = rows.len().min(MAX_ROWS);
        // Keep the caret inside the window of rows that shows.
        self.top = self.top.min(rows.len() - shown);
        if caret_row < self.top {
            self.top = caret_row;
        } else if caret_row >= self.top + shown {
            self.top = caret_row + 1 - shown;
        }

        let text = self.editor.text();
        let highlight = self.editor.highlight_in(&rows);
        let mut area = div()
            .relative()
            .flex_1()
            .min_w_0()
            .py(px(5.0))
            .h(px(shown as f32 * LINE_H + 10.0))
            .overflow_hidden();
        if text.is_empty() {
            area = area.child(
                div()
                    .absolute()
                    .top(px(5.0))
                    .left_0()
                    .h(px(LINE_H))
                    .text_color(t.faint)
                    .child("Ask about your sleep, training or recovery"),
            );
        }
        for (i, &(from, to)) in rows.iter().enumerate().skip(self.top).take(shown) {
            let row = &text[from..to];
            let mut line = div().relative().h(px(LINE_H)).whitespace_nowrap();
            for &(r, a, b) in highlight.iter().filter(|h| h.0 == i) {
                debug_assert_eq!(r, i);
                let (x0, x1) = (
                    x_of(window, row, a, &face, t.fg),
                    x_of(window, row, b.min(row.len()), &face, t.fg),
                );
                line = line.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(x0))
                        .w(px(x1 - x0))
                        .h(px(LINE_H))
                        .bg(t.sel),
                );
            }
            line = line.child(row.to_string());
            if focused && i == caret_row {
                let x = x_of(window, row, caret_at, &face, t.fg);
                line = line.child(
                    div()
                        .absolute()
                        .top(px(2.0))
                        .left(px(x))
                        .w(px(1.5))
                        .h(px(LINE_H - 4.0))
                        .bg(t.acc),
                );
            }
            area = area.child(line);
        }

        let sendable = self.busy || !self.editor.is_blank();
        let button = send_button(self.busy, sendable, &t).on_click(cx.listener(
            |this, _: &ClickEvent, window, cx| {
                window.focus(&this.focus, cx);
                this.submit(cx);
            },
        ));

        let ring = t.acc;
        div()
            .id("composer-box")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.on_key(event, cx)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| window.focus(&this.focus, cx)),
            )
            .flex()
            .items_end()
            .gap(px(8.0))
            .pl(px(14.0))
            .pr(px(8.0))
            .py(px(8.0))
            .bg(t.bg)
            .border_1()
            .border_color(if focused { ring } else { t.line })
            .rounded(px(6.0))
            .when(focused, |d| {
                d.shadow(vec![BoxShadow {
                    color: ring.opacity(0.16),
                    offset: gpui::point(px(0.0), px(0.0)),
                    blur_radius: px(0.0),
                    spread_radius: px(3.0),
                    inset: false,
                }])
            })
            .font_family(SANS)
            .text_size(px(FS_MD))
            .line_height(relative(1.55))
            .text_color(t.fg)
            .child(area)
            .child(button)
    }
}

/// The one filled button of the app. It becomes an outlined Stop while a turn runs.
fn send_button(busy: bool, enabled: bool, t: &Theme) -> gpui::Stateful<gpui::Div> {
    let base = div()
        .id("composer-send")
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(34.0))
        .rounded(px(5.0));
    if busy {
        base.cursor_pointer()
            .border_1()
            .border_color(t.line)
            .child(icon(Icon::Stop, 14.0, t, t.fg))
    } else if enabled {
        base.cursor_pointer()
            .bg(t.acc)
            .child(icon(Icon::Send, 16.0, t, t.bg))
    } else {
        base.child(icon(Icon::Send, 16.0, t, t.faint))
    }
}
