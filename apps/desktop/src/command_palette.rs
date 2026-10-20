//! The command palette: a text field and a filtered list over the window.
//! Ctrl+K and Ctrl+Shift+P open it (Cmd on macOS).

use gpui::{
    actions, div, prelude::*, px, ClickEvent, ColorExt as _, Context, Entity, EventEmitter,
    FocusHandle, IntoElement, KeyBinding, KeyDownEvent, Render, Window,
};

use crate::commands::{self, Action, Command};
use crate::field::Field;
use crate::shell::Root;
use crate::theme::{Palette, FS_SMALL};
use crate::ui::SANS;

actions!(garmin_companion, [OpenPalette]);

/// How many rows show at once. The list scrolls by moving this window.
const ROWS: usize = 8;
const WIDTH: f32 = 560.0;

pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        KeyBinding::new("secondary-k", OpenPalette, None),
        KeyBinding::new("secondary-shift-p", OpenPalette, None),
    ]);
}

pub enum PaletteEvent {
    Run(Action),
    Close,
}

impl EventEmitter<PaletteEvent> for CommandPalette {}

pub struct CommandPalette {
    field: Field,
    commands: Vec<Command>,
    selected: usize,
    /// The query that `selected` belongs to. A new query resets the row.
    seen: String,
    palette: Palette,
    /// What had the focus before the palette opened.
    previous: Option<FocusHandle>,
}

impl CommandPalette {
    fn new(
        query: &str,
        palette: Palette,
        previous: Option<FocusHandle>,
        cx: &mut gpui::App,
    ) -> Self {
        let mut field = Field::new("Type a command", false, cx);
        field.line.insert(query);
        Self {
            field,
            commands: commands::all(),
            selected: 0,
            seen: query.to_string(),
            palette,
            previous,
        }
    }

    /// The matching commands for the text in the field.
    fn hits(&mut self) -> Vec<usize> {
        let text = self.field.line.text();
        if text != self.seen {
            self.seen = text.to_string();
            self.selected = 0;
        }
        commands::filter(&self.commands, text)
    }

    fn step(&mut self, down: bool) {
        let len = self.hits().len();
        if len == 0 {
            return;
        }
        self.selected = if down {
            (self.selected + 1) % len
        } else {
            (self.selected + len - 1) % len
        };
    }

    fn run_selected(&mut self, cx: &mut Context<Self>) {
        let hits = self.hits();
        if let Some(&i) = hits.get(self.selected) {
            cx.emit(PaletteEvent::Run(self.commands[i].action));
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => cx.emit(PaletteEvent::Close),
            "down" => self.step(true),
            "up" => self.step(false),
            _ => return,
        }
        cx.notify();
    }
}

/// The first row to draw, so that the selected row stays in view.
fn first_row(selected: usize) -> usize {
    (selected + 1).saturating_sub(ROWS)
}

impl Render for CommandPalette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.palette.theme();
        let hits = self.hits();
        let first = first_row(self.selected);

        let mut list = div().flex().flex_col().py(px(6.0));
        for (row, &i) in hits.iter().enumerate().skip(first).take(ROWS) {
            let command = &self.commands[i];
            let action = command.action;
            let sel = t.sel;
            let mut line = div()
                .id(("command", i))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .mx(px(6.0))
                .px(px(12.0))
                .py(px(7.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .font_family(SANS)
                .text_size(px(FS_SMALL))
                .text_color(t.fg)
                .hover(move |s| s.bg(sel))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(PaletteEvent::Run(action));
                }))
                .child(command.label.clone());
            if row == self.selected {
                line = line.bg(sel);
            }
            list = list.child(line);
        }
        if hits.is_empty() {
            list = list.child(
                div()
                    .px(px(18.0))
                    .py(px(10.0))
                    .text_size(px(FS_SMALL))
                    .text_color(t.faint)
                    .child("No command matches."),
            );
        }

        let field = self
            .field
            .render(
                "palette-field",
                &t,
                cx,
                window,
                |this| &mut this.field,
                |this, _, cx| this.run_selected(cx),
            )
            .into_any_element();

        // The backdrop takes every click and key that the screen under it would take.
        div()
            .id("palette-backdrop")
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .occlude()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(96.0))
            .bg(t.fg.opacity(0.10))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PaletteEvent::Close)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.on_key(event, cx)))
            .child(
                div()
                    .id("palette-card")
                    .w(px(WIDTH))
                    .max_w_full()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(t.line)
                    .bg(t.bg2)
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    // A click on the card must not reach the backdrop.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(div().p(px(10.0)).child(field))
                    .child(div().border_t_1().border_color(t.line2).child(list)),
            )
    }
}

impl Root {
    /// Open the palette with this text in the field, or close it when it is open.
    pub fn toggle_palette(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_palette.is_some() {
            self.close_palette(window, cx);
            return;
        }
        let previous = window.focused(cx);
        let palette = self.palette;
        let view: Entity<CommandPalette> =
            cx.new(|cx| CommandPalette::new(query, palette, previous, cx));
        let focus = view.read(cx).field.focus.clone();
        focus.focus(window, cx);
        cx.subscribe_in(
            &view,
            window,
            |root, _, event: &PaletteEvent, window, cx| {
                root.close_palette(window, cx);
                if let PaletteEvent::Run(action) = event {
                    root.run_command(*action, window, cx);
                }
            },
        )
        .detach();
        self.command_palette = Some(view);
        cx.notify();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.command_palette.take() {
            if let Some(previous) = view.read(cx).previous.clone() {
                previous.focus(window, cx);
            }
        }
        cx.notify();
    }

    fn run_command(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::Go(index) => self.select_index(index, cx),
            Action::SyncNow => self.start_sync(false, cx),
            Action::CheckUpdates => self.check_for_updates(cx),
            Action::NewChat => {
                self.ask.update(cx, |ask, cx| ask.new_chat(cx));
                self.select_index(crate::shell::ASK, cx);
            }
            Action::ToggleTheme => self.toggle_theme(window, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_follows_the_selected_row() {
        assert_eq!(first_row(0), 0);
        assert_eq!(first_row(ROWS - 1), 0);
        assert_eq!(first_row(ROWS), 1);
        assert_eq!(first_row(ROWS + 4), 5);
    }
}
