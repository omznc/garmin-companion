//! Export of the cache as a CSV file and as PDF reports.
//!
//! The CSV holds every activity. The PDF reports use the same figures as the
//! report view, on one A4 page. The caller passes the font file (a TTF), so
//! this crate does not embed a font.

use crate::derive::{date_key, mean};
use crate::reports::{report_view, week_start};
use crate::text::hours_minutes;
use crate::{ApiError, Source};
use chrono::{Datelike, Months, NaiveDate};
use garmin_core::sport::{is_exercise, sport_label};
use garmin_core::Db;
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};
use std::collections::BTreeMap;
use std::path::Path;

const HEADER: [&str; 9] = [
    "date",
    "name",
    "sport",
    "distance_km",
    "duration_s",
    "avg_hr",
    "max_hr",
    "training_load",
    "elevation_gain_m",
];

/// One CSV field. A field with a comma, a quote or a line break gets quotes,
/// and a quote inside the field is doubled (RFC 4180).
fn field(s: &str) -> String {
    if s.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// One text cell. A spreadsheet reads a cell that starts with `=`, `+`, `-`,
/// `@`, a tab or a line break as a formula, so such a cell gets a leading
/// quote (OWASP). Do not use this for numbers: a negative number stays a number.
fn text_field(s: &str) -> String {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r', '\n']) {
        field(&format!("'{s}"))
    } else {
        field(s)
    }
}

fn number(v: Option<f64>, decimals: usize) -> String {
    v.map(|v| format!("{v:.decimals$}")).unwrap_or_default()
}

/// Every activity in the cache, newest first, as CSV with a header row.
pub fn activities_csv(db: &Db) -> Result<String, ApiError> {
    let mut out = HEADER.join(",");
    out.push_str("\r\n");
    for a in db.activities_since("0000-01-01")? {
        let row = [
            a.local_date.clone().unwrap_or_default(),
            text_field(a.name.as_deref().unwrap_or_default()),
            text_field(&sport_label(a.type_key.as_deref())),
            number(a.distance_m.map(|m| m / 1000.0), 2),
            number(a.duration_s, 0),
            number(a.avg_hr, 0),
            number(a.max_hr, 0),
            number(a.training_load, 0),
            number(a.elevation_gain, 0),
        ];
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }
    Ok(out)
}

/// The text of one report page.
struct Doc {
    title: String,
    range: String,
    rows: Vec<(String, String)>,
    prose: Vec<String>,
}

fn or_dash(v: Option<String>) -> String {
    v.unwrap_or_else(|| "-".into())
}

/// The figures of the days from `from` to `to`, both included.
fn period_rows(db: &Db, from: NaiveDate, to: NaiveDate) -> Result<Vec<(String, String)>, ApiError> {
    let (f, t) = (date_key(from), date_key(to));
    let acts: Vec<_> = db
        .activities_since(&f)?
        .into_iter()
        .filter(|a| is_exercise(a.type_key.as_deref()))
        .filter(|a| a.local_date.as_deref().is_some_and(|d| d <= t.as_str()))
        .collect();
    let daily: Vec<_> = db
        .daily_since(&f)?
        .into_iter()
        .filter(|d| d.date <= t)
        .collect();
    let weight = db
        .weigh_ins_since(&f)?
        .into_iter()
        .filter(|w| w.calendar_date <= t)
        .max_by(|a, b| (&a.calendar_date, a.sample_pk).cmp(&(&b.calendar_date, b.sample_pk)));
    let loads: Vec<f64> = acts.iter().filter_map(|a| a.training_load).collect();
    // An empty f64 sum is -0.0, which prints as "-0.0".
    let km = acts
        .iter()
        .filter_map(|a| a.distance_m)
        .fold(0.0, |a, b| a + b)
        / 1000.0;
    let secs: f64 = acts.iter().filter_map(|a| a.duration_s).sum();
    let sleep = mean(daily.iter().map(|d| d.sleep_secs));
    let rows = [
        ("Sessions", Some(acts.len().to_string())),
        ("Distance", Some(format!("{km:.1} km"))),
        ("Time", Some(hours_minutes(secs))),
        (
            "Training load",
            (!loads.is_empty()).then(|| format!("{:.0}", loads.iter().sum::<f64>())),
        ),
        ("Average sleep", sleep.map(hours_minutes)),
        (
            "Resting HR",
            mean(daily.iter().map(|d| d.resting_hr)).map(|v| format!("{v:.0} bpm")),
        ),
        (
            "HRV",
            mean(daily.iter().map(|d| d.hrv_last_night)).map(|v| format!("{v:.0} ms")),
        ),
        (
            "Weight",
            weight.map(|w| format!("{:.1} kg", w.weight_g / 1000.0)),
        ),
    ];
    Ok(rows
        .into_iter()
        .map(|(k, v)| (k.to_string(), or_dash(v)))
        .collect())
}

fn range_text(from: NaiveDate, to: NaiveDate) -> String {
    format!("{} - {}", from.format("%d %b"), to.format("%d %b %Y"))
}

/// The report of one week as a PDF. `week_offset` counts as in `report_view`.
pub fn weekly_report_pdf(
    db: &Db,
    today: NaiveDate,
    week_offset: usize,
    font: &[u8],
) -> Result<Vec<u8>, ApiError> {
    let view = report_view(db, today, week_offset)?;
    let doc = match week_start(db, today, week_offset)? {
        None => Doc {
            title: "Weekly report".into(),
            range: view.eyebrow,
            rows: Vec::new(),
            prose: view.empty.map(|e| vec![e.body]).unwrap_or_default(),
        },
        Some(start) => {
            let end = start + chrono::Duration::days(6);
            let mut prose = vec![view.headline];
            prose.extend(view.paragraphs);
            Doc {
                title: "Weekly report".into(),
                range: range_text(start, end),
                rows: period_rows(db, start, end)?,
                prose,
            }
        }
    };
    render(&doc, font)
}

/// The report of one calendar month as a PDF. 0 is the month of `today`.
pub fn monthly_report_pdf(
    db: &Db,
    today: NaiveDate,
    month_offset: u32,
    font: &[u8],
) -> Result<Vec<u8>, ApiError> {
    let first = today
        .with_day(1)
        .and_then(|d| d.checked_sub_months(Months::new(month_offset)))
        .ok_or_else(|| ApiError::BadArguments("month offset is out of range".into()))?;
    let last = (first + Months::new(1)).pred_opt().unwrap_or(first);
    let rows = period_rows(db, first, last)?;
    let sessions = rows.first().map(|r| r.1.as_str()).unwrap_or("0");
    let prose = if sessions == "0" {
        "No sessions in this month.".to_string()
    } else {
        format!(
            "{} sessions in {}: {}, in {}.",
            sessions,
            first.format("%B %Y"),
            rows[1].1,
            rows[2].1
        )
    };
    let doc = Doc {
        title: "Monthly report".into(),
        range: range_text(first, last),
        rows,
        prose: vec![prose],
    };
    render(&doc, font)
}

// ---- Files for the mobile bridge ----

/// The font of the PDF reports. The desktop app embeds the same file.
const FONT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../apps/desktop/assets/fonts/IBMPlexSans-Regular.ttf"
));

/// What a file export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Csv,
    Week(usize),
    Month(u32),
}

fn io_error(e: std::io::Error) -> ApiError {
    ApiError::Internal(e.to_string())
}

/// The file name and the bytes of one export.
fn build(db: &Db, today: NaiveDate, kind: Kind) -> Result<(String, Vec<u8>), ApiError> {
    Ok(match kind {
        Kind::Csv => (
            "garmin-activities.csv".into(),
            activities_csv(db)?.into_bytes(),
        ),
        Kind::Week(offset) => {
            let start = week_start(db, today, offset)?.unwrap_or(today);
            (
                format!("garmin-week-{}.pdf", date_key(start)),
                weekly_report_pdf(db, today, offset, FONT)?,
            )
        }
        Kind::Month(back) => {
            let month = today
                .with_day(1)
                .and_then(|d| d.checked_sub_months(Months::new(back)));
            (
                format!("garmin-{}.pdf", month.unwrap_or(today).format("%Y-%m")),
                monthly_report_pdf(db, today, back, FONT)?,
            )
        }
    })
}

/// Write one export into `dir` and return the path of the file. A sample
/// source seeds a scratch cache inside `dir`, so it never opens the real cache.
pub(crate) fn write_export(source: Source, dir: &str, kind: Kind) -> Result<String, ApiError> {
    if dir.trim().is_empty() {
        return Err(ApiError::BadArguments("dir must not be empty".into()));
    }
    let dir = Path::new(dir);
    std::fs::create_dir_all(dir).map_err(io_error)?;
    let (name, bytes) = match source {
        Source::Live => build(&crate::open_default_db()?, garmin_core::today(), kind)?,
        Source::Sample => {
            let scratch = dir.join("sample-export");
            let _ = std::fs::remove_dir_all(&scratch);
            let today = crate::personas::sample_day();
            let made = (|| {
                let db = Db::open(&scratch.join("cache.sqlite3"))?;
                crate::personas::seed(&db, crate::personas::Persona::Sample, today)
                    .map_err(|e| ApiError::Internal(e.to_string()))?;
                garmin_core::with_as_of(today, || build(&db, today, kind))
            })();
            let _ = std::fs::remove_dir_all(&scratch);
            made?
        }
    };
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(io_error)?;
    Ok(path.to_string_lossy().into_owned())
}

// ---- PDF drawing ----

const PAGE_W: f32 = 595.0;
const PAGE_H: f32 = 842.0;
const MARGIN: f32 = 56.0;
const VALUE_X: f32 = 300.0;

/// A TTF file, and the glyphs that the page has used from it.
struct Font<'a> {
    face: ttf_parser::Face<'a>,
    used: BTreeMap<u16, char>,
}

impl Font<'_> {
    fn scale(&self) -> f32 {
        1000.0 / f32::from(self.face.units_per_em())
    }

    /// The glyph for `c`, and the char it shows. A char that the font lacks
    /// shows as "?" (never the .notdef box, glyph 0).
    fn resolve(&self, c: char) -> (u16, char) {
        let find = |c: char| self.face.glyph_index(c).map(|g| (g.0, c));
        find(c).or_else(|| find('?')).unwrap_or((0, '?'))
    }

    fn glyph(&self, c: char) -> u16 {
        self.resolve(c).0
    }

    /// The advance of a glyph, in thousandths of the font size.
    fn advance(&self, gid: u16) -> f32 {
        let units = self.face.glyph_hor_advance(ttf_parser::GlyphId(gid));
        f32::from(units.unwrap_or(0)) * self.scale()
    }

    fn width(&self, s: &str, size: f32) -> f32 {
        s.chars().map(|c| self.advance(self.glyph(c))).sum::<f32>() * size / 1000.0
    }

    /// The glyph ids of `s` as big-endian bytes (the Identity-H encoding).
    fn encode(&mut self, s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for c in s.chars() {
            let (gid, shown) = self.resolve(c);
            self.used.insert(gid, shown);
            out.extend(gid.to_be_bytes());
        }
        out
    }
}

/// The page content, from the top down.
struct Sheet<'a, 'b> {
    content: Content,
    font: &'b mut Font<'a>,
    y: f32,
}

impl Sheet<'_, '_> {
    fn text(&mut self, s: &str, x: f32, size: f32, gray: f32) {
        let bytes = self.font.encode(s);
        self.content
            .set_fill_gray(gray)
            .begin_text()
            .set_font(Name(b"F1"), size)
            .next_line(x, self.y)
            .show(Str(&bytes))
            .end_text();
    }

    fn rule(&mut self) {
        self.content
            .set_stroke_gray(0.85)
            .set_line_width(0.5)
            .move_to(MARGIN, self.y)
            .line_to(PAGE_W - MARGIN, self.y)
            .stroke();
    }

    /// Break `s` into lines that fit the page width.
    fn wrap(&self, s: &str, size: f32) -> Vec<String> {
        let max = PAGE_W - 2.0 * MARGIN;
        let mut lines: Vec<String> = Vec::new();
        for word in s.split_whitespace() {
            match lines.last_mut() {
                Some(line) if self.font.width(&format!("{line} {word}"), size) <= max => {
                    line.push(' ');
                    line.push_str(word);
                }
                _ => lines.push(word.to_string()),
            }
        }
        lines
    }
}

/// Draw the page. The page does not break: text below the bottom margin is
/// dropped. A report is short, so add a second page only if that changes.
fn draw(doc: &Doc, font: &mut Font) -> Vec<u8> {
    let mut s = Sheet {
        content: Content::new(),
        font,
        y: PAGE_H - MARGIN,
    };
    s.text(&doc.title, MARGIN, 22.0, 0.0);
    s.y -= 20.0;
    s.text(&doc.range, MARGIN, 11.0, 0.4);
    s.y -= 30.0;
    for (label, value) in &doc.rows {
        s.text(label, MARGIN, 11.0, 0.3);
        s.text(value, VALUE_X, 11.0, 0.0);
        s.y -= 6.0;
        s.rule();
        s.y -= 16.0;
    }
    s.y -= 10.0;
    for para in &doc.prose {
        for line in s.wrap(para, 11.0) {
            if s.y < MARGIN {
                return s.content.finish().to_vec();
            }
            s.text(&line, MARGIN, 11.0, 0.1);
            s.y -= 15.0;
        }
        s.y -= 8.0;
    }
    s.content.finish().to_vec()
}

fn render(doc: &Doc, font_bytes: &[u8]) -> Result<Vec<u8>, ApiError> {
    let face = ttf_parser::Face::parse(font_bytes, 0)
        .map_err(|e| ApiError::BadArguments(format!("font file is not valid: {e}")))?;
    let mut font = Font {
        face,
        used: BTreeMap::new(),
    };
    let content = draw(doc, &mut font);
    Ok(write_pdf(&content, &font, font_bytes))
}

fn write_pdf(content: &[u8], font: &Font, font_bytes: &[u8]) -> Vec<u8> {
    let (catalog, tree, page, body) = (Ref::new(1), Ref::new(2), Ref::new(3), Ref::new(4));
    let (type0, cid, descriptor, file, cmap) = (
        Ref::new(5),
        Ref::new(6),
        Ref::new(7),
        Ref::new(8),
        Ref::new(9),
    );
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids([page]).count(1);
    let mut p = pdf.page(page);
    p.media_box(Rect::new(0.0, 0.0, PAGE_W, PAGE_H))
        .parent(tree)
        .contents(body);
    p.resources().fonts().pair(Name(b"F1"), type0);
    p.finish();
    pdf.stream(body, content);

    let name = Name(b"EmbeddedFont");
    let info = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    };
    pdf.type0_font(type0)
        .base_font(name)
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(cid)
        .to_unicode(cmap);
    let mut c = pdf.cid_font(cid);
    c.subtype(CidFontType::Type2)
        .base_font(name)
        .system_info(info)
        .font_descriptor(descriptor)
        .default_width(0.0)
        .cid_to_gid_map_predefined(Name(b"Identity"));
    let mut widths = c.widths();
    for &gid in font.used.keys() {
        widths.consecutive(gid, [font.advance(gid)]);
    }
    widths.finish();
    c.finish();

    let scale = font.scale();
    let bb = font.face.global_bounding_box();
    pdf.font_descriptor(descriptor)
        .name(name)
        .flags(FontFlags::NON_SYMBOLIC)
        .bbox(Rect::new(
            f32::from(bb.x_min) * scale,
            f32::from(bb.y_min) * scale,
            f32::from(bb.x_max) * scale,
            f32::from(bb.y_max) * scale,
        ))
        .italic_angle(0.0)
        .ascent(f32::from(font.face.ascender()) * scale)
        .descent(f32::from(font.face.descender()) * scale)
        .cap_height(f32::from(font.face.capital_height().unwrap_or(700)) * scale)
        .stem_v(80.0)
        .font_file2(file);
    let mut stream = pdf.stream(file, font_bytes);
    stream.pair(Name(b"Length1"), font_bytes.len() as i32);
    stream.finish();

    let mut map = pdf_writer::types::UnicodeCmap::new(Name(b"Adobe-Identity-UCS"), info);
    for (&gid, &c) in &font.used {
        map.pair(gid, c);
    }
    pdf.cmap(cmap, &map.finish());
    pdf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()
    }

    fn db() -> Db {
        let dir = std::env::temp_dir().join(format!(
            "garmin-api-export-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::open(&dir.join("cache.sqlite3")).unwrap();
        for (id, name, date) in [
            (1, "Morning, easy \"run\"", "2026-03-10"),
            (2, "Trčanje ćevapi", "2026-03-03"),
        ] {
            let a = serde_json::from_value(json!({
                "activityId": id,
                "activityName": name,
                "startTimeLocal": format!("{date} 07:00:00"),
                "distance": 5000.0,
                "duration": 1800.0,
                "averageHR": 150.0,
                "activityTrainingLoad": 60.0,
                "activityType": {"typeKey": "running"},
            }))
            .unwrap();
            db.upsert_activity(&a).unwrap();
        }
        db
    }

    #[test]
    fn csv_quotes_fields_with_commas_and_quotes() {
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("two\nlines"), "\"two\nlines\"");
    }

    #[test]
    fn csv_text_cells_cannot_start_a_formula() {
        assert_eq!(text_field("=1+1"), "'=1+1");
        assert_eq!(text_field("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(text_field("-2"), "'-2");
        assert_eq!(text_field("+1,5"), "\"'+1,5\"");
        assert_eq!(text_field("\tx"), "'\tx");
        assert_eq!(text_field("Run = fun"), "Run = fun");
        assert_eq!(number(Some(-3.0), 0), "-3");
    }

    #[test]
    fn csv_has_a_header_and_one_row_per_activity() {
        let csv = activities_csv(&db()).unwrap();
        let mut lines = csv.lines();
        assert_eq!(lines.next().unwrap(), HEADER.join(","));
        let first = lines.next().unwrap();
        assert!(first.starts_with("2026-03-10,\"Morning, easy \"\"run\"\"\","));
        assert!(first.contains(",5.00,1800,150,"));
        assert_eq!(lines.count(), 1);
    }

    fn pages(pdf: &[u8]) -> usize {
        let text = String::from_utf8_lossy(pdf);
        text.matches("/Type /Page\n").count() + text.matches("/Type /Page ").count()
    }

    #[test]
    fn weekly_and_monthly_pdf_are_valid_pages() {
        let db = db();
        let week = weekly_report_pdf(&db, today(), 0, FONT).unwrap();
        let month = monthly_report_pdf(&db, today(), 0, FONT).unwrap();
        for pdf in [week, month] {
            assert!(pdf.starts_with(b"%PDF"));
            assert!(pages(&pdf) > 0);
        }
    }

    #[test]
    fn an_empty_cache_still_gives_a_pdf() {
        let empty = {
            let dir = std::env::temp_dir()
                .join(format!("garmin-api-export-empty-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Db::open(&dir.join("cache.sqlite3")).unwrap()
        };
        let pdf = weekly_report_pdf(&empty, today(), 0, FONT).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn a_missing_glyph_never_draws_the_notdef_box() {
        let face = ttf_parser::Face::parse(FONT, 0).unwrap();
        let mut font = Font {
            face,
            used: BTreeMap::new(),
        };
        let bytes = font.encode("Čaša 5 \u{2013} 20\u{b0}C 朝のラン \u{1F3C3}");
        let gids: Vec<u16> = bytes
            .chunks(2)
            .map(|p| u16::from_be_bytes([p[0], p[1]]))
            .collect();
        assert!(gids.iter().all(|&g| g != 0));
        assert!(font.used.keys().all(|&g| g != 0));
        // The CJK chars and the emoji all share the "?" glyph.
        assert_eq!(font.used.values().filter(|&&c| c == '?').count(), 1);
        assert_eq!(font.glyph('朝'), font.glyph('?'));
        // A char that the font has keeps its own glyph.
        assert_ne!(font.glyph('\u{2013}'), font.glyph('?'));
        assert_ne!(font.glyph('\u{b0}'), font.glyph('?'));
    }

    #[test]
    fn a_bad_font_is_an_error() {
        assert!(monthly_report_pdf(&db(), today(), 0, b"no font").is_err());
    }
}
