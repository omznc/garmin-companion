//! The blocks of the Reports screen: the figures of one week, and the files
//! that the user can save.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::scale::px;
use chrono::{Datelike, Months, NaiveDate};
use garmin_api::reports::ReportView;
use gpui::{div, prelude::*, App, ClickEvent, Div, SharedString};

use super::{Block, BlockDef, Ctx, Width};
use crate::activities::Store;
use crate::charts::Tone;
use crate::icons::{icon, Icon};
use crate::kit::{self, Cell};
use crate::screen::Screen;
use crate::theme::Theme;
use crate::ui::{MONO, SANS};

pub const BLOCKS: &[BlockDef] = &[
    BlockDef {
        id: "reports_week",
        title: "Weekly report",
        group: Screen::Reports,
        default_width: Width::TwoThirds,
        render: week,
    },
    BlockDef {
        id: "reports_files",
        title: "Reports",
        group: Screen::Reports,
        default_width: Width::Third,
        render: files,
    },
];

pub const LAYOUT: &[(&str, Width)] = &[
    ("reports_week", Width::TwoThirds),
    ("reports_files", Width::Third),
];

pub fn title(_ctx: &Ctx) -> Option<String> {
    Some("Reports".into())
}

pub fn lede(_ctx: &Ctx) -> Option<String> {
    Some("Weekly and monthly summaries, ready to export.".into())
}

/// Reports keep the text of the screen.
pub fn crumb(_ctx: &Ctx) -> Option<String> {
    None
}

/// The Plex Sans file that the PDF reports embed.
const FONT: &[u8] = include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf");

/* ------------------------------------------------------------------- week --- */

/// The value of the figure with this label.
fn figure(v: &ReportView, label: &str) -> Option<String> {
    v.figures
        .iter()
        .find(|f| f.label == label)
        .map(|f| f.value.clone())
}

/// A figure as a cell. A value with a unit splits at the first space.
fn cell(label: &str, value: &str, sub: Option<(String, Tone)>) -> Cell {
    let (number, unit) = match value.split_once(' ') {
        Some((n, u)) => (n.to_string(), Some(u.to_string().into())),
        None => (value.to_string(), None),
    };
    Cell {
        label: label.to_string().into(),
        value: number.into(),
        unit,
        sub: sub.map(|(s, t)| (s.into(), t)),
        spark: None,
        big: false,
    }
}

/// The change against the week before, as text with a tone.
fn change(now: f64, before: Option<f64>, unit: &str) -> Option<(String, Tone)> {
    let diff = now - before?;
    if diff.abs() < 0.5 {
        return None;
    }
    let text = format!("{diff:+.0} {unit} vs last week").replace('-', "\u{2212}");
    Some((text, Tone::Muted))
}

fn week_cells(v: &ReportView) -> Vec<Cell> {
    let x = &v.extras;
    let mut out = Vec::new();
    for label in ["Sessions", "Distance", "Time"] {
        if let Some(value) = figure(v, label) {
            out.push(cell(label, &value, None));
        }
    }
    if let Some(load) = x.load {
        out.push(cell(
            "Load",
            &format!("{load:.0}"),
            change(load, x.prior_load, ""),
        ));
    }
    if let Some(value) = figure(v, "Avg sleep") {
        out.push(cell("Avg sleep", &value, None));
    }
    if let Some(hrv) = x.avg_hrv {
        out.push(cell(
            "HRV",
            &format!("{hrv:.0} ms"),
            change(hrv, x.prior_avg_hrv, "ms"),
        ));
    }
    if let Some(kg) = x.weight_kg {
        let sub = x
            .weight_change_kg
            .filter(|d| d.abs() >= 0.05)
            .map(|d| (format!("{d:+.1} kg").replace('-', "\u{2212}"), Tone::Muted));
        out.push(cell("Weight", &format!("{kg:.1} kg"), sub));
    }
    out
}

/// The text of the report: the headline and the paragraphs.
fn prose(v: &ReportView, t: &Theme) -> Div {
    let mut col = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .px(px(12.0))
        .py(px(12.0))
        .border_t_1()
        .border_color(t.line2)
        .child(
            div()
                .text_size(px(13.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(v.headline.clone()),
        );
    for p in &v.paragraphs {
        col = col.child(
            div()
                .text_size(px(12.5))
                .line_height(gpui::relative(1.6))
                .text_color(t.mutd)
                .child(p.clone()),
        );
    }
    col
}

fn week(ctx: &Ctx) -> Option<Block> {
    let v = &ctx.training?.report;
    if v.empty.is_some() {
        return None;
    }
    let cells = week_cells(v);
    if cells.is_empty() {
        return None;
    }
    let range = v
        .eyebrow
        .split_once(" · ")
        .map_or(v.eyebrow.as_str(), |p| p.1);
    let body = div()
        .child(kit::stat_grid(cells, 3, ctx.t))
        .child(prose(v, ctx.t));
    Some(Block {
        meta: Some(range.to_string().into()),
        link: None,
        body: body.into_any_element(),
    })
}

/* ------------------------------------------------------------------ files --- */

/// What one row of the list saves.
#[derive(Clone, Copy)]
enum Export {
    /// The week at this offset, as in `report_view`.
    Week(usize),
    /// The month this many months back from the day on show.
    Month(u32),
    Csv,
}

/// The line under the list: the result of the last save.
static STATUS: Mutex<String> = Mutex::new(String::new());

fn set_status(text: String) {
    if let Ok(mut s) = STATUS.lock() {
        *s = text;
    }
}

fn status() -> String {
    STATUS.lock().map(|s| s.clone()).unwrap_or_default()
}

/// The bytes of the file, and its suggested name.
fn build(kind: Export, store: &Store, font: &[u8]) -> Result<(Vec<u8>, String), String> {
    let today = store.today();
    let db = store.open().map_err(|e| e.to_string())?;
    let bad = |e: garmin_api::ApiError| e.to_string();
    Ok(match kind {
        Export::Week(offset) => (
            garmin_api::export::weekly_report_pdf(&db, today, offset, font).map_err(bad)?,
            format!("garmin-week-{}.pdf", today.format("%Y-%m-%d")),
        ),
        Export::Month(back) => {
            let month = today.with_day(1).unwrap_or(today) - Months::new(back);
            (
                garmin_api::export::monthly_report_pdf(&db, today, back, font).map_err(bad)?,
                format!("garmin-{}.pdf", month.format("%Y-%m")),
            )
        }
        Export::Csv => (
            garmin_api::export::activities_csv(&db)
                .map_err(bad)?
                .into_bytes(),
            "garmin-activities.csv".to_string(),
        ),
    })
}

/// Ask for a place with the dialog of the system, then write the file.
fn save(kind: Export, store: Store, cx: &mut App) {
    let (bytes, name) = match build(kind, &store, FONT) {
        Ok(made) => made,
        Err(e) => {
            set_status(format!("Could not make the file: {e}"));
            cx.refresh_windows();
            return;
        }
    };
    let dir = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    let asked = cx.prompt_for_new_path(&dir, Some(&name));
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(path))) = asked.await else {
            return;
        };
        let text = match std::fs::write(&path, bytes) {
            Ok(()) => format!("Saved {}", path.display()),
            Err(e) => format!("Could not save the file: {e}"),
        };
        set_status(text);
        cx.update(|cx| cx.refresh_windows());
    })
    .detach();
}

struct File {
    title: String,
    sub: String,
    tag: &'static str,
    kind: Export,
}

fn file_row(i: usize, file: File, store: &Store, t: &Theme) -> impl IntoElement {
    let hover = t.hover;
    let store = store.clone();
    let kind = file.kind;
    div()
        .id(("report-file", i))
        .flex()
        .items_start()
        .gap(px(10.0))
        .px(px(12.0))
        .py(px(9.0))
        .text_size(px(13.0))
        .cursor_pointer()
        .when(i > 0, |d| d.border_t_1().border_color(t.line2))
        .hover(move |s| s.bg(hover))
        .on_click(move |_: &ClickEvent, _, cx| save(kind, store.clone(), cx))
        .child(
            div()
                .mt(px(2.0))
                .child(icon(Icon::NavReports, 14.0, t, t.acc)),
        )
        .child(
            div().flex_1().min_w_0().child(file.title).child(
                div()
                    .text_size(px(12.0))
                    .text_color(t.faint)
                    .child(file.sub),
            ),
        )
        .child(
            div()
                .flex_none()
                .font_family(MONO)
                .text_size(px(11.0))
                .text_color(t.faint)
                .child(file.tag),
        )
}

fn files_list(ctx: &Ctx) -> Vec<File> {
    let mut out = Vec::new();
    if let Some(v) = ctx
        .training
        .map(|t| &t.report)
        .filter(|v| v.empty.is_none())
    {
        let range = v
            .eyebrow
            .split_once(" · ")
            .map_or(v.eyebrow.as_str(), |p| p.1);
        out.push(File {
            title: "Weekly report".into(),
            sub: range.to_string(),
            tag: "PDF",
            kind: Export::Week(v.offset),
        });
    }
    let day: NaiveDate = ctx.store.today();
    for back in 0..2 {
        let month = day.with_day(1).unwrap_or(day) - Months::new(back);
        out.push(File {
            title: month.format("%B %Y").to_string(),
            sub: "monthly".into(),
            tag: "PDF",
            kind: Export::Month(back),
        });
    }
    if let Some(n) = ctx.store.count().filter(|n| *n > 0) {
        out.push(File {
            title: "All activities".into(),
            sub: format!("{n} rows"),
            tag: "CSV",
            kind: Export::Csv,
        });
    }
    out
}

fn files(ctx: &Ctx) -> Option<Block> {
    let list = files_list(ctx);
    if list.iter().all(|f| !matches!(f.kind, Export::Csv)) {
        return None;
    }
    let mut col = div().flex().flex_col();
    for (i, file) in list.into_iter().enumerate() {
        col = col.child(file_row(i, file, ctx.store, ctx.t));
    }
    let line: SharedString = status().into();
    if !line.is_empty() {
        col = col.child(
            div()
                .px(px(12.0))
                .py(px(8.0))
                .border_t_1()
                .border_color(ctx.t.line2)
                .font_family(SANS)
                .text_size(px(12.0))
                .text_color(ctx.t.mutd)
                .child(line),
        );
    }
    Some(Block {
        meta: Some("PDF and CSV".into()),
        link: None,
        body: col.into_any_element(),
    })
}
