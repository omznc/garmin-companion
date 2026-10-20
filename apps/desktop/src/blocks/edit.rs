//! The edit mode of the Today screen: the edit bar, the cards with their
//! controls, the Add blocks panel and the keys. The list itself is in
//! `layout.rs`. Everything here reads the registry, so no block needs code
//! for the edit mode.

use gpui::{
    div, point, prelude::*, AnyElement, App, BorderStyle, BoxShadow, ClickEvent, Context, Div,
    FocusHandle, FontWeight, Hsla, KeyDownEvent, MouseButton, SharedString, Stateful, Window,
};

use super::layout::{width_label, Layout};
use super::{def, lay_out, registry, BlockDef, Ctx};
use crate::field::Field;
use crate::icons::{icon, Icon};
use crate::kit::{self, Look};
use crate::motion;
use crate::scale::px;
use crate::screen::Screen;
use crate::scroll::SmoothScroll;
use crate::shell::Root;
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

/// The width of the Add blocks panel.
pub const PANEL_W: f32 = 320.0;

/// The state of the edit mode. `on` is false outside the mode.
pub struct Edit {
    pub on: bool,
    /// The card that the keys move, as a place in the layout.
    pub selected: Option<usize>,
    pub search: Field,
    /// The page holds this focus, so the keys reach it.
    pub focus: FocusHandle,
    scroll: SmoothScroll,
}

impl Edit {
    pub fn new(cx: &mut App) -> Self {
        let mut search = Field::new("Search blocks", false, cx);
        search.sans = true;
        Self {
            on: false,
            selected: None,
            search,
            focus: cx.focus_handle(),
            scroll: SmoothScroll::new(motion::reduced()),
        }
    }

    pub fn tick(&self, window: &mut Window) {
        self.scroll.tick(window);
    }
}

/// What a drag of a card carries: the place the card has now.
#[derive(Clone, Copy)]
struct CardDrag {
    from: usize,
    title: &'static str,
    t: Theme,
}

/// The card that follows the pointer during a drag.
struct Ghost(CardDrag);

impl gpui::Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.0.t;
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(32.0))
            .px(px(10.0))
            .rounded(px(7.0))
            .border_1()
            .border_color(t.acc)
            .bg(t.bg2)
            .shadow(vec![shadow(t.shadow, 0.0, 6.0, 18.0)])
            .font_family(SANS)
            .text_size(px(12.5))
            .text_color(t.fg)
            .child(icon(Icon::Grip, 12.0, &t, t.acc))
            .child(self.0.title)
    }
}

fn shadow(color: Hsla, y: f32, blur: f32, spread: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    }
}

/* ------------------------------------------------------------------ root --- */

impl Root {
    /// True while the user arranges Today.
    pub(crate) fn editing(&self) -> bool {
        self.active == Screen::Today && self.edit.as_ref().is_some_and(|e| e.on)
    }

    /// Enter the edit mode, with a card selected or none.
    pub(crate) fn start_edit(
        &mut self,
        selected: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.edit.as_mut().filter(|_| self.active == Screen::Today) else {
            return;
        };
        edit.on = true;
        edit.selected = selected;
        edit.search.line.clear();
        edit.focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn stop_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.edit.as_mut() {
            edit.on = false;
            edit.selected = None;
        }
        cx.notify();
    }

    /// A change of the layout is saved at once.
    fn layout_changed(&mut self, selected: Option<usize>, cx: &mut Context<Self>) {
        if let Some(edit) = self.edit.as_mut() {
            edit.selected = selected;
        }
        self.save_window_state();
        cx.notify();
    }

    fn select_block(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(edit) = self.edit.as_mut() {
            edit.selected = Some(at);
            edit.focus.focus(window, cx);
        }
        cx.notify();
    }

    fn move_block(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if let Some(at) = self.today_layout.move_to(from, to) {
            self.layout_changed(Some(at), cx);
        }
    }

    fn step_block(&mut self, at: usize, step: isize, cx: &mut Context<Self>) {
        if let Some(to) = self.today_layout.move_by(at, step) {
            self.layout_changed(Some(to), cx);
        }
    }

    fn cycle_block(&mut self, at: usize, cx: &mut Context<Self>) {
        if self.today_layout.cycle_width(at).is_some() {
            self.layout_changed(Some(at), cx);
        }
    }

    fn remove_block(&mut self, at: usize, cx: &mut Context<Self>) {
        if self.today_layout.remove(at) {
            self.layout_changed(None, cx);
        }
    }

    fn add_block(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.today_layout.add(id) {
            let last = self.today_layout.len() - 1;
            self.layout_changed(Some(last), cx);
        }
    }

    fn reset_layout(&mut self, cx: &mut Context<Self>) {
        self.today_layout = Layout::standard();
        self.layout_changed(None, cx);
    }

    /// A key on the page: the arrows move the card, W changes its width,
    /// Delete removes it and Escape leaves the mode.
    fn edit_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if key.modifiers.modified() {
            return;
        }
        let selected = self.edit.as_ref().and_then(|e| e.selected);
        match (key.key.as_str(), selected) {
            ("escape", _) => self.stop_edit(cx),
            ("left" | "up", Some(at)) => self.step_block(at, -1, cx),
            ("right" | "down", Some(at)) => self.step_block(at, 1, cx),
            ("w", Some(at)) => self.cycle_block(at, cx),
            ("delete" | "backspace", Some(at)) => self.remove_block(at, cx),
            ("left" | "right" | "up" | "down", None) if !self.today_layout.is_empty() => {
                self.layout_changed(Some(0), cx)
            }
            _ => {}
        }
    }
}

/* ------------------------------------------------------------------- bar --- */

/// A button with an icon before the label. The row runs in reverse, so the
/// icon that comes second in the code stands first on the screen.
pub fn icon_label_button(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    label: &'static str,
    look: Look,
    t: &Theme,
) -> Stateful<Div> {
    let colour = if look == Look::Primary { t.bg } else { t.mutd };
    kit::button(id, label, look, true, t)
        .flex_row_reverse()
        .child(icon(glyph, 12.0, t, colour))
}

/// The button of the bar that starts the edit mode.
pub fn start_button(t: &Theme, cx: &mut Context<Root>) -> Stateful<Div> {
    icon_label_button(
        "edit-layout",
        Icon::GridPlus,
        "Edit layout",
        Look::Default,
        t,
    )
    .on_click(cx.listener(|root, _: &ClickEvent, window, cx| root.start_edit(None, window, cx)))
}

/// The bar of the edit mode. It stands in the place of the breadcrumb bar.
pub fn bar(root: &Root, narrow: bool, t: &Theme, cx: &mut Context<Root>) -> Div {
    let count = root.today_layout.len();
    let count = format!("{count} block{}", if count == 1 { "" } else { "s" });
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(38.0))
        .pl(px(16.0))
        .pr(px(12.0))
        .bg(t.acctint)
        .border_b_1()
        .border_color(t.accline)
        .font_family(SANS)
        .text_size(px(12.5))
        .text_color(t.mutd)
        .child(icon(Icon::GridPlus, 14.0, t, t.acc))
        .child(
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_color(t.fg)
                .font_weight(FontWeight::MEDIUM)
                .child("Editing Today"),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .when(narrow, |d| d.invisible())
                .child("Drag a block to move it. Use the arrows with the keyboard."),
        )
        .child(
            div()
                .flex_none()
                .font_family(MONO)
                .text_size(px(11.5))
                .child(count),
        )
        .child(
            kit::button("edit-reset", "Reset", Look::Default, true, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.reset_layout(cx))),
        )
        .child(
            kit::button("edit-done", "Done", Look::Primary, true, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.stop_edit(cx))),
        )
}

/* ----------------------------------------------------------------- cards --- */

fn control(id: (&'static str, usize), glyph: Icon, hover: Hsla, t: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.0))
        .rounded(px(4.0))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(glyph, 12.0, t, t.mutd))
}

/// The size button: the width icon and the width, in mono.
fn size_button(n: usize, width: super::Width, t: &Theme) -> Stateful<Div> {
    let hover = t.hover;
    div()
        .id(("edit-size", n))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.0))
        .h(px(22.0))
        .px(px(6.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(t.line)
        .cursor_pointer()
        .font_family(MONO)
        .text_size(px(11.0))
        .text_color(t.mutd)
        .hover(move |s| s.bg(hover))
        .child(icon(Icon::Width, 12.0, t, t.mutd))
        .child(width_label(width))
}

/// The title and the controls of a card in the edit mode.
fn header(n: usize, d: &BlockDef, spec: &CardSpec, t: &Theme, cx: &mut Context<Root>) -> Div {
    let drag = CardDrag {
        from: n,
        title: d.title,
        t: *t,
    };
    let wide = spec.width.columns() >= 6;
    let title = if spec.body.is_some() {
        d.title.to_string()
    } else {
        format!("{} \u{b7} no data yet", d.title)
    };
    let title_colour = if spec.body.is_some() { t.mutd } else { t.faint };
    let grip = div()
        .id(("edit-grip", n))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.0))
        .cursor_pointer()
        .on_drag(drag, move |drag, _, _, cx| cx.new(|_| Ghost(*drag)))
        .child(icon(Icon::Grip, 12.0, t, t.faint));
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.0))
        .h(px(34.0))
        .pl(px(6.0))
        .pr(px(6.0))
        .bg(t.bg2)
        .border_b_1()
        .border_color(t.line)
        .child(grip)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(SANS)
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(title_colour)
                .child(title),
        )
        .when(wide, |h| {
            h.child(kit::chip(d.group.label(), false, false, t))
        })
        .children(spec.meta.clone().filter(|_| wide).map(|text| {
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(MONO)
                .text_size(px(11.0))
                .text_color(t.faint)
                .child(text)
        }))
        .child(
            control(("edit-prev", n), Icon::ChevronLeft, t.hover, t).on_click(
                cx.listener(move |root, _: &ClickEvent, _, cx| root.step_block(n, -1, cx)),
            ),
        )
        .child(
            control(("edit-next", n), Icon::ChevronRight, t.hover, t).on_click(
                cx.listener(move |root, _: &ClickEvent, _, cx| root.step_block(n, 1, cx)),
            ),
        )
        .child(
            size_button(n, spec.width, t)
                .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.cycle_block(n, cx))),
        )
        .child(
            control(("edit-remove", n), Icon::X, t.warntint, t)
                .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.remove_block(n, cx))),
        )
}

/// What a card shows: the width, the header text and the body. A block with
/// no data has no body, so the card is a muted placeholder.
struct CardSpec {
    width: super::Width,
    meta: Option<SharedString>,
    body: Option<AnyElement>,
    selected: bool,
}

/// The body with a cover. The cover takes the clicks, so a row of a block
/// does not open a screen in the edit mode.
fn covered(body: AnyElement, n: usize, cx: &mut Context<Root>) -> Div {
    div()
        .relative()
        .child(body)
        .child(div().absolute().top_0().left_0().size_full().on_mouse_down(
            MouseButton::Left,
            cx.listener(move |root, _, window, cx| {
                cx.stop_propagation();
                root.select_block(n, window, cx)
            }),
        ))
}

fn edit_card(n: usize, d: &BlockDef, mut spec: CardSpec, t: &Theme, cx: &mut Context<Root>) -> Div {
    let head = header(n, d, &spec, t, cx);
    let acc = t.acc;
    let selected = spec.selected;
    let body = spec.body.take().map(|body| covered(body, n, cx));
    div()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(8.0))
        .border_1()
        .border_color(if selected { t.acc } else { t.accline })
        .when(!selected, |c| c.border_dashed())
        .bg(t.bg)
        .shadow(vec![shadow(t.acctint, 0.0, 0.0, 3.0)])
        .drag_over::<CardDrag>(move |style, _, _, _| {
            let mut style = style.border_color(acc);
            style.border_style = Some(BorderStyle::Solid);
            style
        })
        .on_drop(cx.listener(move |root, drag: &CardDrag, _, cx| root.move_block(drag.from, n, cx)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |root, _, window, cx| root.select_block(n, window, cx)),
        )
        .child(head)
        .children(body)
}

/// The grid of the edit mode. Every block of the layout has a card, also a
/// block with no data, so the user can move or remove it.
pub fn grid(
    layout: &Layout,
    selected: Option<usize>,
    ctx: &Ctx,
    available: f32,
    narrow: bool,
    cx: &mut Context<Root>,
) -> Div {
    let mut cards: Vec<(u8, Div)> = Vec::new();
    for (n, (id, width)) in layout.pairs().into_iter().enumerate() {
        let Some(d) = def(id) else {
            continue;
        };
        let block = (d.render)(ctx);
        let spec = CardSpec {
            width,
            meta: block.as_ref().and_then(|b| b.meta.clone()),
            body: block.map(|b| b.body),
            selected: selected == Some(n),
        };
        cards.push((width.columns(), edit_card(n, d, spec, ctx.t, cx)));
    }
    lay_out(cards, available, narrow)
}

/// The page of the edit mode. It holds the focus, so the keys reach it.
pub fn page(root: &Root, content: Div, cx: &mut Context<Root>) -> Stateful<Div> {
    let page = div().id("edit-page");
    let Some(edit) = root.edit.as_ref() else {
        return page.child(content);
    };
    page.track_focus(&edit.focus)
        .on_key_down(cx.listener(|root, event: &KeyDownEvent, _, cx| root.edit_key(event, cx)))
        .child(content)
}

/// The box that stands for a Today with no blocks.
pub fn empty_box(editing: bool, t: &Theme) -> Div {
    let text = if editing {
        "Today has no blocks. Add blocks from the panel on the right."
    } else {
        "Today has no blocks. Choose Edit layout to add some."
    };
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(120.0))
        .rounded(px(8.0))
        .border_1()
        .border_dashed()
        .border_color(t.line)
        .text_size(px(13.0))
        .text_color(t.mutd)
        .child(text)
}

/* ----------------------------------------------------------------- panel --- */

/// A block matches when the query is in its title or in the name of its screen.
fn matches(d: &BlockDef, query: &str) -> bool {
    query.is_empty()
        || d.title.to_lowercase().contains(query)
        || d.group.label().to_lowercase().contains(query)
}

fn group_head(screen: Screen, t: &Theme) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(7.0))
        .h(px(28.0))
        .mt(px(6.0))
        .px(px(12.0))
        .font_family(SANS)
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.faint)
        .child(icon(screen.icon(), 12.0, t, t.faint))
        .child(screen.label())
}

fn row(
    index: usize,
    d: &'static BlockDef,
    layout: &Layout,
    t: &Theme,
    cx: &mut Context<Root>,
) -> Div {
    let action: AnyElement = if layout.contains(d.id) {
        kit::chip("On Today", true, false, t).into_any_element()
    } else {
        let id = d.id;
        icon_label_button(("add-block", index), Icon::Plus, "Add", Look::Default, t)
            .on_click(cx.listener(move |root, _: &ClickEvent, _, cx| root.add_block(id, cx)))
            .into_any_element()
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(32.0))
        .px(px(12.0))
        .font_family(SANS)
        .text_size(px(12.5))
        .text_color(t.fg)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(d.title),
        )
        .child(
            div()
                .flex_none()
                .font_family(MONO)
                .text_size(px(11.0))
                .text_color(t.faint)
                .child(width_label(d.default_width)),
        )
        .child(action)
}

/// The catalog: the blocks that match the query, grouped by screen.
fn catalog(layout: &Layout, query: &str, t: &Theme, cx: &mut Context<Root>) -> Div {
    let mut list = div().flex().flex_col().pb(px(12.0));
    let mut index = 0;
    for screen in Screen::ALL {
        let rows: Vec<&'static BlockDef> = registry()
            .filter(|d| d.group == screen && matches(d, query))
            .collect();
        if rows.is_empty() {
            continue;
        }
        list = list.child(group_head(screen, t));
        for d in rows {
            list = list.child(row(index, d, layout, t, cx));
            index += 1;
        }
    }
    if index == 0 {
        list = list.child(
            div()
                .p(px(12.0))
                .text_size(px(12.5))
                .text_color(t.mutd)
                .child("No block matches that search."),
        );
    }
    list
}

fn search_field(edit: &Edit, t: &Theme, window: &Window, cx: &mut Context<Root>) -> AnyElement {
    let field = edit.search.render(
        "block-search",
        t,
        cx,
        window,
        |root: &mut Root| {
            &mut root
                .edit
                .as_mut()
                .expect("start makes the edit state")
                .search
        },
        |_, _, _| {},
    );
    div()
        .flex_none()
        .px(px(12.0))
        .py(px(10.0))
        .child(field)
        .into_any_element()
}

/// The Add blocks panel. It stands in the place of the coach panel.
pub fn panel(
    root: &Root,
    width: f32,
    t: &Theme,
    window: &Window,
    cx: &mut Context<Root>,
) -> Option<AnyElement> {
    let edit = root.edit.as_ref()?;
    let query = edit.search.line.text().trim().to_lowercase();
    let head = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .h(px(34.0))
        .px(px(12.0))
        .border_b_1()
        .border_color(t.line)
        .font_family(SANS)
        .text_size(px(12.5))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.fg)
                .child("Add blocks"),
        )
        .child(
            div()
                .font_family(MONO)
                .text_size(px(11.0))
                .text_color(t.faint)
                .child(format!("{} blocks", registry().count())),
        );
    let list = catalog(&root.today_layout, &query, t, cx);
    Some(
        div()
            .flex_none()
            .w(px(width))
            .h_full()
            .flex()
            .flex_col()
            .bg(t.bg2)
            .border_l_1()
            .border_color(t.line)
            .child(head)
            .child(search_field(edit, t, window, cx))
            .child(
                edit.scroll
                    .area("add-blocks")
                    .flex_1()
                    .min_h_0()
                    .child(list),
            )
            .into_any_element(),
    )
}
