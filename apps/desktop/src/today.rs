//! The Today screen, from `app/src/screens/Today.tsx`.
//!
//! The layout, the spacing and the order of the blocks are the same as the
//! current app. This version has no Coach panel and no share actions yet.

use gpui::{div, prelude::*, px, relative, ColorExt as _, Div, Hsla};

use crate::data::{Drift, Session, Today};
use crate::icons::{icon_hover, Icon};
use crate::theme::{Theme, FS_BASE, FS_CAPTION, FS_LG, FS_MD, FS_MICRO, FS_SMALL};
use crate::ui::{
    axis_labels, block_in, body, bullet, ch, eyebrow, eyebrow_lg, h1, line_chart, metric,
    small_stat, underlined, LH_NORMAL, MONO, SANS,
};

/// The fill of one zone in the stacked bar. Z3 and up step through the accent.
fn zone_fill(t: &Theme, zone: usize) -> Hsla {
    match zone {
        0 => t.line,
        1 => t.mutd,
        2 => t.acc.opacity(0.55),
        3 => t.acc.opacity(0.78),
        _ => t.acc,
    }
}

/// The section marker with the air above and below it that Today uses.
fn section_eyebrow(text: &str, below: f32, t: &Theme) -> Div {
    div()
        .mt(px(72.0))
        .mb(px(below))
        .child(eyebrow(text.to_string(), t))
}

/// The refresh control at the right of the page header.
fn refresh(t: &Theme) -> impl IntoElement {
    let (mutd, fg) = (t.mutd, t.fg);
    div()
        .id("refresh")
        .group("refresh")
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(8.0))
        .font_family(SANS)
        .text_size(px(FS_CAPTION))
        .text_color(mutd)
        .hover(move |s| s.text_color(fg))
        .child(icon_hover(Icon::Sync, 14.0, t, mutd, Some(("refresh", fg))))
        .child("Refresh")
}

fn header(view: &Today, key: usize, t: &Theme) -> impl IntoElement {
    div()
        .mb(px(28.0))
        .child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(16.0))
                .min_h(px(14.0))
                .child(block_in(key, 0, eyebrow_lg(view.eyebrow.clone(), t)))
                .child(refresh(t)),
        )
        .child(block_in(key, 0, h1(view.title.clone()).mt(px(18.0))))
}

fn zone_bar(session: &Session, t: &Theme) -> Div {
    const LABELS: [&str; 5] = ["Z1", "Z2", "Z3", "Z4", "Z5"];
    let bar = session
        .zones
        .iter()
        .enumerate()
        .filter(|(_, p)| **p > 0.0)
        .fold(
            div().flex().h(px(10.0)).overflow_hidden().rounded(px(2.0)),
            |bar, (i, p)| bar.child(div().h_full().w(relative(*p / 100.0)).bg(zone_fill(t, i))),
        );
    // Below 1% a label is noise, and five of them crowd out the ones that matter.
    let legend = session
        .zones
        .iter()
        .enumerate()
        .filter(|(_, p)| **p >= 1.0)
        .fold(
            div()
                .flex()
                .flex_wrap()
                .gap(px(16.0))
                .mt(px(9.0))
                .font_family(SANS)
                .text_size(px(FS_CAPTION))
                .line_height(relative(LH_NORMAL))
                .text_color(t.mutd),
            |row, (i, p)| {
                row.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().size(px(6.0)).rounded(px(1.0)).bg(zone_fill(t, i)))
                        .child(format!("{} {:.0}%", LABELS[i], p)),
                )
            },
        );
    div().child(bar).child(legend)
}

fn last_session(session: &Session, t: &Theme) -> impl IntoElement {
    let stats = session.stats.iter().fold(
        div()
            .flex()
            .flex_wrap()
            .gap_x(px(38.0))
            .gap_y(px(20.0))
            .mt(px(24.0)),
        |row, s| row.child(small_stat(&s.label, &s.value, t)),
    );
    div()
        .child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(16.0))
                .mt(px(72.0))
                .mb(px(16.0))
                .child(eyebrow("Last session", t))
                .child(underlined("full-breakdown", "Full breakdown", FS_SMALL, t)),
        )
        .child(body(session.name.clone(), FS_LG, t.fg).mb(px(4.0)))
        .child(body(session.detail.clone(), FS_SMALL, t.faint).mb(px(20.0)))
        .child(zone_bar(session, t))
        .child(stats)
}

/// The strip's height, and the room left for the bars once the two labels and
/// the two gaps of each column are taken out. A percent height would need a
/// definite parent, and the flex column does not give one.
const DRIFT_H: f32 = 96.0;
const DRIFT_GAP: f32 = 7.0;
const DRIFT_TRACK: f32 = DRIFT_H - 2.0 * DRIFT_GAP - (FS_CAPTION + FS_MICRO) * LH_NORMAL;

fn drift_bar(date: &str, share: f32, target: f32, t: &Theme) -> impl IntoElement {
    let fill = if share > target { t.acc } else { t.mutd };
    let track = div()
        .flex_none()
        .relative()
        .h(px(DRIFT_TRACK))
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(DRIFT_TRACK * target / 100.0))
                .h(px(1.0))
                .bg(t.line),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(DRIFT_TRACK * share.max(1.5) / 100.0))
                .bg(fill)
                .rounded_t(px(2.0)),
        );
    div()
        .flex_1()
        .flex()
        .flex_col()
        .justify_end()
        .h(px(DRIFT_H))
        .gap(px(DRIFT_GAP))
        .child(
            div()
                .font_family(MONO)
                .text_size(px(FS_CAPTION))
                .line_height(relative(LH_NORMAL))
                .text_color(t.mutd)
                .child(format!("{share:.0}%")),
        )
        .child(track)
        .child(
            div()
                .text_size(px(FS_MICRO))
                .line_height(relative(LH_NORMAL))
                .text_color(t.faint)
                .whitespace_nowrap()
                .text_center()
                .child(date.to_string()),
        )
}

fn zone_drift(drift: &Drift, t: &Theme) -> impl IntoElement {
    let bars = drift.runs.iter().fold(
        div().flex().gap(px(10.0)).items_end().h(px(DRIFT_H)),
        |row, run| row.child(drift_bar(&run.date, run.above_z2, drift.target, t)),
    );
    div()
        .child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(16.0))
                .mt(px(72.0))
                .mb(px(18.0))
                .child(eyebrow(
                    format!("Zone drift · last {} runs", drift.runs.len()),
                    t,
                ))
                .child(body(drift.summary.clone(), FS_SMALL, t.mutd)),
        )
        .child(bars)
}

fn last_seven_days(view: &Today, t: &Theme) -> impl IntoElement {
    let week = &view.week;
    let chart = div()
        .flex_1()
        .min_w(px(320.0))
        .child(line_chart(&week.km, 84.0, 6.0, t))
        .child(axis_labels(&week.axis, t));
    div()
        .child(section_eyebrow("Last seven days", 18.0, t))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_end()
                .gap(px(34.0))
                .child(chart)
                .child(
                    body(week.summary.clone(), FS_BASE, t.mutd)
                        .line_height(relative(1.6))
                        .max_w(ch(26.0, FS_BASE)),
                ),
        )
}

/// The screen. `key` changes on each visit, so the blocks rise in again.
pub fn render(view: &Today, key: usize, t: &Theme) -> impl IntoElement {
    let metrics = view
        .metrics
        .iter()
        .fold(div().flex().flex_wrap().gap(px(54.0)), |row, m| {
            row.child(metric(m, t))
        });

    let mut screen = div()
        .child(header(view, key, t))
        .child(block_in(
            key,
            1,
            div().mb(px(40.0)).child(
                div()
                    .max_w(ch(62.0, FS_MD))
                    .font_family(SANS)
                    .text_size(px(FS_MD))
                    .line_height(relative(1.65))
                    .text_color(t.mutd)
                    .child(view.lede.clone()),
            ),
        ))
        .child(block_in(key, 2, metrics))
        .child(block_in(
            key,
            3,
            body(view.fuel_note.clone(), FS_BASE, t.mutd)
                .mt(px(26.0))
                .max_w(ch(58.0, FS_BASE)),
        ));

    if let Some(note) = &view.weight_note {
        let weight = div()
            .mt(px(14.0))
            .max_w(ch(58.0, FS_BASE))
            .flex()
            .flex_wrap()
            .items_baseline()
            .gap_x(px(5.0))
            .font_family(SANS)
            .text_size(px(FS_BASE))
            .line_height(relative(1.6))
            .text_color(t.mutd)
            .child(note.clone())
            .child(underlined("weight-link", "Weight", FS_BASE, t));
        screen = screen.child(block_in(key, 4, weight));
    }

    if !view.attention.is_empty() {
        let list = view
            .attention
            .iter()
            .enumerate()
            .fold(div().flex().flex_col().gap(px(13.0)), |col, (i, a)| {
                col.child(bullet(i, a, t))
            });
        screen = screen.child(block_in(
            key,
            5,
            div()
                .child(section_eyebrow("Attention", 16.0, t))
                .child(list),
        ));
    }
    if let Some(session) = &view.last_session {
        screen = screen.child(block_in(key, 6, last_session(session, t)));
    }
    if let Some(drift) = view.zone_drift.as_ref().filter(|d| d.runs.len() >= 2) {
        screen = screen.child(block_in(key, 7, zone_drift(drift, t)));
    }
    screen.child(block_in(key, 8, last_seven_days(view, t)))
}
