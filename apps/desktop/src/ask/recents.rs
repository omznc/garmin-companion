//! Recents: the crumbs and the buttons above a thread, and the list of saved
//! threads with open, rename and delete.

use chrono::{DateTime, Utc};
use garmin_api::settings::since;

use super::*;

/// The line under a title: how many questions, and how long ago.
fn meta_line(meta: &chat::ThreadMeta, now: DateTime<Utc>) -> String {
    let n = meta.turn_count;
    let count = format!("{n} {}", if n == 1 { "question" } else { "questions" });
    match DateTime::parse_from_rfc3339(&meta.updated_at) {
        Ok(then) => format!("{count} · {}", since(then.with_timezone(&Utc), now)),
        Err(_) => count,
    }
}

impl Ask {
    /// For screenshots: `GC_OPEN_RECENTS=rename` or `=delete` shows that state
    /// on the sample recovery thread.
    #[cfg(debug_assertions)]
    pub fn preview_row(&mut self, state: &str) {
        let id = Some("sample-recovery".to_owned());
        match state {
            "rename" => {
                self.rename
                    .line
                    .insert("Am I recovered enough for a hard day?");
                self.renaming = id;
            }
            "delete" => self.deleting = id,
            _ => {}
        }
    }

    /// The page under `Ask` in the breadcrumb bar: `Recents`, or the title of
    /// the open thread. `None` for an empty chat.
    pub fn crumb_tail(&self) -> Option<String> {
        match self.pane {
            Pane::Recents => Some("Recents".to_owned()),
            Pane::Chat => self.title.clone(),
        }
    }

    /// The crumbs of the panel: `Coach` and then `Recents`, or the title of the
    /// open thread. In Recents the first crumb goes back to the chat.
    pub(super) fn crumbs(&self, cx: &mut Context<Self>, t: &Theme) -> Div {
        let in_list = self.pane == Pane::Recents;
        let tail = self.crumb_tail();
        let fg = t.fg;
        let head = div()
            .id("crumb-coach")
            .flex_none()
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if tail.is_some() { t.mutd } else { t.fg })
            .child("Coach")
            .when(in_list, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.text_color(fg))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.show_chat(cx)))
            });
        let mut row = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.0))
            .font_family(SANS)
            .text_size(px(13.0))
            .child(head);
        if let Some(tail) = tail {
            row = row
                .child(div().flex_none().text_color(t.faint).child("›"))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(t.fg)
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(SharedString::from(tail)),
                );
        }
        row
    }

    /// The two buttons at the right of the crumbs: Recents and a new chat.
    pub(super) fn thread_buttons(&self, cx: &mut Context<Self>, t: &Theme) -> Div {
        let (recents_id, new_id) = if self.panel {
            ("coach-recents", "coach-new")
        } else {
            ("ask-recents", "ask-new")
        };
        let in_list = self.pane == Pane::Recents;
        let recents = icon_button(recents_id, Icon::History, 14.0, false, in_list, t).on_click(
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                if in_list {
                    this.show_chat(cx)
                } else {
                    this.show_recents(cx)
                }
            }),
        );
        let new_thread = icon_button(new_id, Icon::Plus, 14.0, false, false, t)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_chat(cx)));
        div()
            .flex_none()
            .flex()
            .items_center()
            .child(recents)
            .child(new_thread)
    }

    /// The list of saved threads, newest first.
    pub(super) fn recents_list(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
        t: &Theme,
    ) -> gpui::AnyElement {
        let Some(threads) = &self.threads else {
            return body("Loading", FS_SMALL, t.faint).into_any_element();
        };
        if threads.is_empty() {
            return body(
                "No saved chats yet. A question that you ask the coach appears here.",
                FS_BASE,
                t.mutd,
            )
            .line_height(relative(1.6))
            .into_any_element();
        }
        let now = Utc::now();
        let mut col = div().flex().flex_col().gap(px(2.0));
        for (i, meta) in threads.iter().enumerate() {
            col = col.child(self.thread_row(i, meta, now, window, cx, t));
        }
        col.into_any_element()
    }

    fn thread_row(
        &self,
        i: usize,
        meta: &chat::ThreadMeta,
        now: DateTime<Utc>,
        window: &Window,
        cx: &mut Context<Self>,
        t: &Theme,
    ) -> gpui::AnyElement {
        let id = meta.session_id.clone();
        if self.renaming.as_deref() == Some(id.as_str()) {
            return self.rename_row(i, window, cx, t).into_any_element();
        }
        if self.deleting.as_deref() == Some(id.as_str()) {
            return self.delete_row(i, meta, cx, t).into_any_element();
        }
        let current = self.session.as_deref() == Some(id.as_str());
        let (hover, active) = (t.hover, t.active);
        let sub = if meta.running {
            body("Writing an answer", FS_CAPTION, t.acc)
        } else {
            body(meta_line(meta, now), FS_CAPTION, t.faint)
        };
        let title_for_rename = meta.title.clone();
        let (open_id, rename_id, delete_id) = (id.clone(), id.clone(), id);
        let actions = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                quiet(("thread-rename", i), "Rename", true, false, t).on_click(cx.listener(
                    move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.start_rename(rename_id.clone(), &title_for_rename, window, cx);
                    },
                )),
            )
            .child(
                quiet(("thread-delete", i), "Delete", true, true, t).on_click(cx.listener(
                    move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.deleting = Some(delete_id.clone());
                        cx.notify();
                    },
                )),
            );
        div()
            .id(("thread-row", i))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .when(current, |d| d.bg(active))
            .hover(move |s| s.bg(hover))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                // The open thread stays as it is: a running answer goes on.
                if this.session.as_deref() == Some(open_id.as_str()) {
                    this.show_chat(cx);
                } else {
                    this.open_thread(open_id.clone(), cx);
                }
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        body(meta.title.clone(), FS_BASE, t.fg)
                            .truncate()
                            .font_weight(gpui::FontWeight::MEDIUM),
                    )
                    .child(sub),
            )
            .child(actions)
            .into_any_element()
    }

    /// A row that waits for the new title: the field, and Save and Cancel.
    fn rename_row(&self, i: usize, window: &Window, cx: &mut Context<Self>, t: &Theme) -> Div {
        let field = self.rename.render(
            "thread-rename-field",
            t,
            cx,
            window,
            |ask: &mut Ask| &mut ask.rename,
            |ask, _, cx| ask.commit_rename(cx),
        );
        let save = button(("thread-rename-save", i), "Save", Look::Primary, true, t)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.commit_rename(cx)));
        let cancel = button(
            ("thread-rename-cancel", i),
            "Cancel",
            Look::Default,
            true,
            t,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.renaming = None;
            cx.notify();
        }));
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(8.0))
            .child(field)
            .child(div().flex().gap(px(6.0)).child(save).child(cancel))
    }

    /// A row that asks "Delete?" before it deletes.
    fn delete_row(
        &self,
        i: usize,
        meta: &chat::ThreadMeta,
        cx: &mut Context<Self>,
        t: &Theme,
    ) -> Div {
        let id = meta.session_id.clone();
        let yes = button(("thread-delete-yes", i), "Delete", Look::Danger, true, t).on_click(
            cx.listener(move |this, _: &ClickEvent, _, cx| this.delete_thread(id.clone(), cx)),
        );
        let no = button(("thread-delete-no", i), "Cancel", Look::Default, true, t).on_click(
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.deleting = None;
                cx.notify();
            }),
        );
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(t.warnline)
            .bg(t.warntint)
            .child(body("Delete this chat?", FS_BASE, t.fg))
            .child(body(meta.title.clone(), FS_CAPTION, t.mutd).truncate())
            .child(div().flex().gap(px(6.0)).child(yes).child(no))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(n: i64, updated: &str) -> chat::ThreadMeta {
        chat::ThreadMeta {
            session_id: "s".into(),
            title: "t".into(),
            updated_at: updated.into(),
            turn_count: n,
            running: false,
        }
    }

    #[test]
    fn the_line_under_a_title_counts_questions_and_gives_the_age() {
        let now: DateTime<Utc> = "2026-10-22T12:00:00Z".parse().unwrap();
        assert_eq!(
            meta_line(&meta(1, "2026-10-22T09:00:00Z"), now),
            "1 question · 3 hours ago"
        );
        assert_eq!(
            meta_line(&meta(3, "2026-10-22T11:59:30Z"), now),
            "3 questions · just now"
        );
        assert_eq!(meta_line(&meta(2, "not a date"), now), "2 questions");
    }
}
