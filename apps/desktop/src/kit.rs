//! The parts that every screen is built from: the buttons, the chips, the key
//! hints and the charts. The sizes come from the CSS of the design (`.ib`,
//! `.btn`, `.chip`, `.kbd`).

use crate::scale::px;
use gpui::{div, prelude::*, relative, Div, ElementId, FontWeight, Hsla, SharedString, Stateful};

use crate::charts::{spark, Tone};
use crate::icons::{icon, icon_hover, Icon};
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

/// The key that the shortcuts use: Cmd on macOS, Ctrl elsewhere.
pub fn modifier() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

/// `.kbd`: a key hint in a hairline box.
pub fn kbd(text: impl Into<SharedString>, t: &Theme) -> Div {
    div()
        .flex_none()
        .px(px(4.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(t.line)
        .font_family(MONO)
        .text_size(px(11.0))
        .line_height(px(16.0))
        .text_color(t.faint)
        .whitespace_nowrap()
        .child(text.into())
}

/// `.ib`: an icon button, 28 px or 22 px. `on` marks a toggle that is set.
pub fn icon_button(
    id: &'static str,
    glyph: Icon,
    glyph_size: f32,
    small: bool,
    on: bool,
    t: &Theme,
) -> Stateful<Div> {
    let colour = if on { t.fg } else { t.mutd };
    icon_button_shell(id, small, on, t).child(icon_hover(
        glyph,
        glyph_size,
        t,
        colour,
        Some((id, t.fg)),
    ))
}

/// The box of an icon button, for a button whose icon is not a plain one.
pub fn icon_button_shell(id: &'static str, small: bool, on: bool, t: &Theme) -> Stateful<Div> {
    let side = if small { 22.0 } else { 28.0 };
    let hover = t.hover;
    div()
        .id(id)
        .group(id)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(side))
        .rounded(px(if small { 4.0 } else { 6.0 }))
        .cursor_pointer()
        .when(on, |d| d.bg(t.active))
        .when(!on, |d| d.hover(move |s| s.bg(hover)))
}

/// `.chip`: a small label in a hairline box. A dot marks a live state.
pub fn chip(label: impl Into<SharedString>, accent: bool, dot: bool, t: &Theme) -> Div {
    let (fg, border, bg) = if accent {
        (t.acc, t.accline, t.acctint)
    } else {
        (t.mutd, t.line, t.bg2)
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.0))
        .h(px(20.0))
        .px(px(7.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(border)
        .bg(bg)
        .font_family(SANS)
        .text_size(px(11.5))
        .text_color(fg)
        .whitespace_nowrap()
        .when(dot, |d| {
            d.child(div().size(px(6.0)).rounded_full().bg(t.acc))
        })
        .child(label.into())
}

/// How a button looks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Default,
    /// The filled accent button.
    Primary,
    /// The outlined warning button. The screens of Track 3 use it.
    #[allow(dead_code)]
    Danger,
    /// No ground: for a button inside a box.
    Ghost,
}

/// `.btn`: a 24 px button with a label. A disabled button has no pointer.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    look: Look,
    enabled: bool,
    t: &Theme,
) -> Stateful<Div> {
    let (fg, border, bg): (Hsla, Hsla, Hsla) = match look {
        Look::Default => (t.fg, t.line, t.bg2),
        Look::Primary => (t.bg, t.acc, t.acc),
        Look::Danger => (t.warn, t.warnline, t.warntint),
        Look::Ghost => (t.fg, t.line, gpui::transparent_black()),
    };
    let hover = t.hover;
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.0))
        .h(px(24.0))
        .px(px(9.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(border)
        .bg(bg)
        .font_family(SANS)
        .text_size(px(12.0))
        .line_height(relative(1.0))
        .text_color(fg)
        .whitespace_nowrap()
        .when(enabled, |d| {
            d.cursor_pointer().hover(move |s| {
                if look == Look::Primary {
                    s.opacity(0.9)
                } else {
                    s.bg(hover)
                }
            })
        })
        .when(!enabled, |d| d.opacity(0.45))
        .child(label.into())
}

/* ------------------------------------------------------------------- card --- */

/// `.lnk`: the accent link in a card header.
pub fn link(id: impl Into<ElementId>, label: impl Into<SharedString>, t: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .pr(px(6.0))
        .cursor_pointer()
        .font_family(SANS)
        .text_size(px(12.0))
        .text_color(t.acc)
        .whitespace_nowrap()
        .hover(|s| s.underline())
        .child(label.into())
}

/// `.card`: a hairline box with a 34 px header. The header holds the title,
/// the meta text in mono, and a link. `body` is the content.
pub fn card(
    title: impl Into<SharedString>,
    meta: Option<SharedString>,
    link: Option<Stateful<Div>>,
    body: impl IntoElement,
    t: &Theme,
) -> Div {
    let header = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(34.0))
        .pl(px(12.0))
        .pr(px(6.0))
        .bg(t.bg2)
        .border_b_1()
        .border_color(t.line)
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
                .letter_spacing(px(12.0 * 0.02))
                .text_color(t.mutd)
                .child(title.into()),
        )
        .children(meta.map(|text| {
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
        .children(link);
    // The body fills a card that a lane stretches, so a stat grid can reach
    // the bottom edge.
    div()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(8.0))
        .border_1()
        .border_color(t.line)
        .bg(t.bg)
        .flex()
        .flex_col()
        .child(header)
        .child(div().flex_1().flex().flex_col().child(body))
}

/* -------------------------------------------------------------- stat grid --- */

/// One cell of a stat grid.
pub struct Cell {
    pub label: SharedString,
    pub value: SharedString,
    pub unit: Option<SharedString>,
    /// A line under the value, in mono, and the tone of its colour.
    pub sub: Option<(SharedString, Tone)>,
    /// A sparkline under the value. A cell with no series has none.
    pub spark: Option<Vec<f32>>,
    /// The value is 21 px (a metric) and not 15 px (a key figure).
    pub big: bool,
}

/// The box of one grid cell. An empty filler cell uses it too, so the dividers
/// of every row line up.
fn cell_box(big: bool, t: &Theme) -> Div {
    let (x, y) = if big { (14.0, 12.0) } else { (12.0, 10.0) };
    div()
        .flex_1()
        .min_w_0()
        .px(px(x))
        .py(px(y))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .border_r_1()
        .border_b_1()
        .border_color(t.line2)
}

/// `.metrics` and `.kv`: cells in `columns` columns, with a hairline between
/// them. The hairlines of the last column and the last row fall outside the card.
pub fn stat_grid(cells: Vec<Cell>, columns: usize, t: &Theme) -> Div {
    let columns = columns.max(1);
    // Fixed rows, not flex_wrap: at a fractional display scale the 1/n widths
    // can round above the row width, and the last cell wraps on every other frame.
    let mut grid = div().flex().flex_col().mr(px(-1.0)).mb(px(-1.0));
    let mut row = div().flex_1().flex();
    let mut in_row = 0;
    let mut last_big = false;
    for cell in cells {
        let size = if cell.big { 21.0 } else { 15.0 };
        let value = div()
            .flex()
            .items_baseline()
            .font_family(MONO)
            .text_size(px(size))
            .text_color(t.fg)
            .whitespace_nowrap()
            .child(cell.value)
            .children(cell.unit.map(|u| {
                div()
                    .ml(px(3.0))
                    .text_size(px(11.5))
                    .text_color(t.mutd)
                    .child(u)
            }));
        last_big = cell.big;
        row = row.child(
            cell_box(cell.big, t)
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(11.5))
                        .text_color(t.faint)
                        .child(cell.label),
                )
                .child(value)
                .children(
                    cell.spark
                        .map(|values| div().mt(px(6.0)).mb(px(4.0)).child(spark(&values, t))),
                )
                .children(cell.sub.map(|(text, tone)| {
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_family(MONO)
                        .text_size(px(11.5))
                        .text_color(tone.colour(t))
                        .child(text)
                })),
        );
        in_row += 1;
        if in_row == columns {
            grid = grid.child(row);
            row = div().flex_1().flex();
            in_row = 0;
        }
    }
    if in_row > 0 {
        // Empty cells keep the 1/n width of the cells in the last row.
        grid = grid.child(row.children((in_row..columns).map(|_| cell_box(last_big, t))));
    }
    // In a stretched card the rows grow, so the dividers reach the bottom edge.
    div()
        .flex_1()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(grid.flex_1())
}

/* ------------------------------------------------------------------- list --- */

/// One row of a list: an icon, a title with a line under it, and a figure at the right.
pub struct ListRow {
    pub glyph: Icon,
    pub tone: Tone,
    pub title: SharedString,
    pub sub: Option<SharedString>,
    pub right: Option<SharedString>,
    pub right_sub: Option<SharedString>,
}

/// `.row`: the rows of a list, each under a hairline but the first.
pub fn list_rows(rows: Vec<ListRow>, t: &Theme) -> Div {
    list_rows_click(rows.into_iter().map(|row| (row, None)).collect(), t)
}

/// As `list_rows`, and a row with a `RowClick` opens something when clicked.
pub fn list_rows_click(rows: Vec<(ListRow, Option<RowClick>)>, t: &Theme) -> Div {
    let hover = t.hover;
    let mut col = div().flex().flex_col();
    for (i, (row, click)) in rows.into_iter().enumerate() {
        let colour = match row.tone {
            Tone::Muted => t.mutd,
            tone => tone.colour(t),
        };
        let mut line = div()
            .flex()
            .items_start()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(9.0))
            .text_size(px(13.0))
            .when(i > 0, |d| d.border_t_1().border_color(t.line2))
            .hover(move |s| s.bg(hover))
            .child(div().mt(px(2.0)).child(icon(row.glyph, 14.0, t, colour)))
            .child(
                div().flex_1().min_w_0().child(row.title).children(
                    row.sub
                        .map(|sub| div().text_size(px(12.0)).text_color(t.faint).child(sub)),
                ),
            );
        if row.right.is_some() || row.right_sub.is_some() {
            line = line.child(
                div()
                    .flex_none()
                    .text_right()
                    .font_family(MONO)
                    .text_size(px(12.0))
                    .whitespace_nowrap()
                    .children(row.right)
                    .children(
                        row.right_sub
                            .map(|sub| div().text_size(px(11.0)).text_color(t.faint).child(sub)),
                    ),
            );
        }
        col = col.child(match click {
            Some(click) => line
                .id(("list-row", i))
                .cursor_pointer()
                .on_click(move |_, window, cx| click(window, cx))
                .into_any_element(),
            None => line.into_any_element(),
        });
    }
    col
}

/* ------------------------------------------------------------------ table --- */

/// The width of one character of a numeric cell, in design pixels.
const NUMBER_CHAR: f32 = 7.5;
/// The same for a text cell. A text column starts at this width and grows.
const TEXT_CHAR: f32 = 7.0;
/// A text column with this many characters or fewer, such as a date, never shrinks.
const SHORT_TEXT: f32 = 12.0;

/// A column of a table: its title, and whether it holds numbers.
#[allow(dead_code)] // The table, the segmented control and the switch wait for later screens.
pub struct Column {
    pub title: SharedString,
    pub numeric: bool,
}

/// `.tbl`: a header row, then rows. A numeric column is mono and right
/// aligned. The first column is dim.
#[allow(dead_code)]
pub fn table(columns: &[Column], rows: Vec<Vec<SharedString>>, t: &Theme) -> Div {
    let rows = rows.into_iter().map(|cells| (cells, None)).collect();
    table_rows(columns, rows, t)
}

/// What a click on a row does. It runs with the window and the app.
pub type RowClick = Box<dyn Fn(&mut gpui::Window, &mut gpui::App)>;

/// As `table`, and a row with a `RowClick` opens something when clicked.
pub fn table_rows(
    columns: &[Column],
    rows: Vec<(Vec<SharedString>, Option<RowClick>)>,
    t: &Theme,
) -> Div {
    let hover = t.hover;
    // A numeric column is as wide as its longest text. A text column takes
    // the rest of the row, in shares that follow its longest text.
    let weights: Vec<f32> = (0..columns.len())
        .map(|n| {
            let longest = rows
                .iter()
                .filter_map(|(cells, _)| cells.get(n))
                .map(|c| c.chars().count())
                .chain([columns[n].title.chars().count()])
                .max()
                .unwrap_or(4);
            longest.clamp(3, 36) as f32
        })
        .collect();
    let cell = |text: SharedString, column: &Column, head: bool, first: bool, weight: f32| {
        let base = div().min_w_0();
        let base = if column.numeric {
            base.flex_none().w(px(weight * NUMBER_CHAR + 24.0))
        } else {
            base.flex_basis(px(weight * TEXT_CHAR + 24.0))
                .flex_grow(weight)
                .flex_shrink(if weight <= SHORT_TEXT { 0.0 } else { 1.0 })
        };
        base.px(px(12.0))
            .py(px(if head { 7.0 } else { 8.0 }))
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .when(column.numeric, |d| d.text_right())
            .when(column.numeric && !head, |d| d.font_family(MONO))
            .when(head, |d| {
                d.text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.faint)
            })
            .when(!head, |d| {
                d.text_size(px(12.5))
                    .text_color(if first { t.faint } else { t.fg })
            })
            .child(text)
    };
    let mut out = div()
        .flex()
        .flex_col()
        .child(columns.iter().zip(&weights).fold(
            div().flex().border_b_1().border_color(t.line),
            |row, (c, w)| row.child(cell(c.title.clone(), c, true, false, *w)),
        ));
    let last = rows.len().saturating_sub(1);
    for (i, (cells, click)) in rows.into_iter().enumerate() {
        let line = cells.into_iter().zip(columns).enumerate().fold(
            div()
                .flex()
                .when(i < last, |d| d.border_b_1().border_color(t.line2))
                .hover(move |s| s.bg(hover)),
            |line, (n, (text, c))| line.child(cell(text, c, false, n == 0, weights[n])),
        );
        out = out.child(match click {
            Some(click) => line
                .id(("table-row", i))
                .cursor_pointer()
                .on_click(move |_, window, cx| click(window, cx))
                .into_any_element(),
            None => line.into_any_element(),
        });
    }
    out
}

/* --------------------------------------------------------- segmented, switch --- */

/// `.seg`: a choice of one among a few. The chosen one has the `active` ground.
#[allow(dead_code)]
pub fn segmented(id: &'static str, options: &[&'static str], chosen: usize, t: &Theme) -> Div {
    let mut row = div()
        .flex()
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(t.line)
        .bg(t.bg);
    for (i, label) in options.iter().enumerate() {
        let on = i == chosen;
        let (hover, fg) = (t.hover, t.fg);
        row = row.child(
            div()
                .id((id, i))
                .flex()
                .items_center()
                .h(px(22.0))
                .px(px(9.0))
                .rounded(px(4.0))
                .cursor_pointer()
                .font_family(SANS)
                .text_size(px(12.0))
                .whitespace_nowrap()
                .text_color(if on { t.fg } else { t.mutd })
                .when(on, |d| d.bg(t.active))
                .when(!on, |d| d.hover(move |s| s.bg(hover).text_color(fg)))
                .child(*label),
        );
    }
    row
}

/// `.switch`: 30 by 17, with a round thumb.
#[allow(dead_code)]
pub fn switch(on: bool, t: &Theme) -> Div {
    let (ground, thumb) = if on { (t.acc, t.bg) } else { (t.line, t.faint) };
    let thumb = div().size(px(13.0)).rounded_full().bg(thumb);
    div()
        .flex_none()
        .flex()
        .items_center()
        .w(px(30.0))
        .h(px(17.0))
        .px(px(2.0))
        .rounded_full()
        .bg(ground)
        .when(on, |d| d.justify_end())
        .child(thumb)
}
