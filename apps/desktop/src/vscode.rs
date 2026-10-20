//! A VS Code colour theme, read as our seven colours.
//!
//! A theme on open-vsx can name six hundred colours or eleven. This app needs
//! seven: the page, the nav ground, the text, two quiet text steps, the accent
//! and the warning. Each one takes the first key that the theme sets, from a
//! short chain. A key that is missing gets a value computed from the others.
//! The hairlines and tints come later, from `theme::derive`.
//!
//! `tokenColors` and `semanticTokenColors` colour code inside an editor. This
//! app has no code view, so the reader drops them.
//!
//! The text colours must be readable. `readable` moves a colour away from its
//! background until it clears the contrast floor. It moves the text, never the
//! background. The port comes from `mbrd-rs/crates/app/src/vscode.rs`.

use std::collections::HashMap;

use garmin_core::theme::{Appearance, Colors};
use serde::Deserialize;
use serde_json::{Map, Value};

/// An include chain longer than this stops. It also stops a file that includes itself.
const MAX_INCLUDE_DEPTH: usize = 6;
/// The contrast floor for text that the user reads as a sentence.
const TEXT_FLOOR: f32 = 4.5;
/// The least spread between the top and the bottom channel for a colour to count as a hue.
const MIN_CHROMA: f32 = 0.15;
/// The most HSL saturation that a muted or faint text colour may have.
const NEUTRAL_SATURATION: f32 = 0.35;
/// The floor for the quietest text step. It repeats words that are near it.
const FAINT_FLOOR: f32 = 3.0;

/// A theme file, as far as this app needs it.
#[derive(Debug, Deserialize)]
pub struct File {
    #[serde(default)]
    pub name: Option<String>,
    /// `dark`, `light`, `hc` or `hcLight`. Many files leave it out.
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// Another file to start from, relative to this one.
    #[serde(default)]
    pub include: Option<String>,
    #[serde(default)]
    pub colors: Map<String, Value>,
}

/// Read one file. The format is JSONC: comments and trailing commas are legal.
pub fn parse(text: &str) -> Result<File, String> {
    serde_json::from_str::<File>(&strip(text)).map_err(|e| format!("is not a colour theme: {e}"))
}

/// Lay an `include` chain flat. The file that includes has the last word.
///
/// `files` holds every JSON file of the extension, keyed by its path. A
/// missing include or a cycle ends the walk. The colours found so far stay.
pub fn resolve(path: &str, files: &HashMap<String, String>) -> Result<File, String> {
    let mut chain = Vec::new();
    let mut at = path.to_string();
    for _ in 0..MAX_INCLUDE_DEPTH {
        let Some(text) = files.get(&at) else { break };
        let file = parse(text)?;
        let next = file.include.clone();
        chain.push(file);
        let Some(next) = next else { break };
        at = join(&at, &next);
    }
    let Some(mut out) = chain.pop() else {
        return Err("is not in this extension".into());
    };
    for outer in chain.into_iter().rev() {
        for (key, value) in outer.colors {
            out.colors.insert(key, value);
        }
        out.name = out.name.or(outer.name);
        out.kind = out.kind.or(outer.kind);
    }
    Ok(out)
}

/// A relative path against the file that names it: `./a.json`, `../b/a.json`, `a.json`.
fn join(from: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for step in rel.split('/') {
        match step {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            name => parts.push(name),
        }
    }
    parts.join("/")
}

/// JSONC to JSON: remove the comments and the trailing commas.
///
/// One pass with one state, "inside a string". Only a string can hide a `//`.
/// Only a backslash can hide a quote.
pub fn strip(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('/', Some('/')) => skip_line(&mut chars, &mut out),
            ('/', Some('*')) => skip_block(&mut chars, &mut out),
            _ => {
                in_string = c == '"';
                out.push(c);
            }
        }
    }
    trailing_commas(&out)
}

/// Drop a `//` comment. The line break stays, so line numbers hold.
fn skip_line(chars: &mut impl Iterator<Item = char>, out: &mut String) {
    if chars.any(|c| c == '\n') {
        out.push('\n');
    }
}

/// Drop a `/* */` comment. It leaves a space: `1/**/2` is two tokens.
fn skip_block(chars: &mut impl Iterator<Item = char>, out: &mut String) {
    chars.next();
    let mut last = '\0';
    for c in chars {
        if last == '*' && c == '/' {
            break;
        }
        last = c;
    }
    out.push(' ');
}

/// A comma whose next non-space character closes its array or object is dropped.
fn trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        at += 1;
        if in_string {
            out.push(c);
            match c {
                '\\' => {
                    out.extend(chars.get(at));
                    at += 1;
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        let closes = chars[at..]
            .iter()
            .find(|c| !c.is_whitespace())
            .is_some_and(|&c| c == '}' || c == ']');
        if c == ',' && closes {
            continue;
        }
        in_string = c == '"';
        out.push(c);
    }
    out
}

// -- Colour arithmetic, in sRGB ---------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rgba {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl Rgba {
    fn hex(value: u32) -> Self {
        let byte = |shift: u32| ((value >> shift) & 0xff) as f32 / 255.0;
        Rgba {
            r: byte(16),
            g: byte(8),
            b: byte(0),
            a: 1.0,
        }
    }

    /// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
    fn read(text: &str) -> Option<Self> {
        let body = text.trim().strip_prefix('#')?;
        if !body.is_ascii() {
            return None;
        }
        let digits: Vec<u32> = match body.len() {
            3 | 4 => body
                .chars()
                .map(|c| c.to_digit(16).map(|d| d * 17))
                .collect::<Option<_>>()?,
            6 | 8 => body
                .as_bytes()
                .chunks(2)
                .map(|p| u32::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
                .collect::<Option<_>>()?,
            _ => return None,
        };
        let at = |i: usize| digits.get(i).map_or(1.0, |&d| d as f32 / 255.0);
        Some(Rgba {
            r: at(0),
            g: at(1),
            b: at(2),
            a: at(3),
        })
    }

    fn to_hex(self) -> String {
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!(
            "#{:02x}{:02x}{:02x}",
            byte(self.r),
            byte(self.g),
            byte(self.b)
        )
    }
}

/// One colour on an opaque one. Most editor colours are washes with an alpha.
fn over(top: Rgba, bottom: Rgba) -> Rgba {
    let a = top.a.clamp(0.0, 1.0);
    let mix = |t: f32, b: f32| t * a + b * (1.0 - a);
    Rgba {
        r: mix(top.r, bottom.r),
        g: mix(top.g, bottom.g),
        b: mix(top.b, bottom.b),
        a: 1.0,
    }
}

/// A step from one colour towards another.
fn toward(from: Rgba, to: Rgba, amount: f32) -> Rgba {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    Rgba {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: 1.0,
    }
}

/// WCAG relative luminance.
fn luminance(c: Rgba) -> f32 {
    let channel = |v: f32| {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

/// WCAG contrast ratio.
fn contrast(a: Rgba, b: Rgba) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// A colour moved until it reads on a background.
///
/// It moves away from the background: to white on a dark one, to black on a
/// light one. It does not jump to white. A dim blue becomes a bright blue.
/// Twenty steps of five percent. A colour that is already close moves a little.
fn readable(color: Rgba, on: Rgba, floor: f32) -> Rgba {
    if contrast(color, on) >= floor {
        return color;
    }
    let target = if luminance(on) < 0.18 {
        Rgba::hex(0xffffff)
    } else {
        Rgba::hex(0x000000)
    };
    let mut out = color;
    for step in 1..=20 {
        out = toward(color, target, step as f32 * 0.05);
        if contrast(out, on) >= floor {
            break;
        }
    }
    out
}

// -- The translation ---------------------------------------------------------

/// The `colors` of a theme, without the values that are not colours.
/// VS Code allows `null` to unset a key. That value is dropped here.
struct Keys(HashMap<String, Rgba>);

impl Keys {
    fn of(colors: &Map<String, Value>) -> Self {
        Keys(
            colors
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), Rgba::read(value.as_str()?)?)))
                .collect(),
        )
    }

    /// The first key in the chain that the theme sets.
    fn pick(&self, keys: &[&str]) -> Option<Rgba> {
        keys.iter().find_map(|key| self.0.get(*key).copied())
    }

    /// The same, flattened onto the surface behind it.
    fn solid(&self, keys: &[&str], on: Rgba) -> Option<Rgba> {
        self.pick(keys).map(|c| over(c, on))
    }

    /// The first key in the chain with a real hue, else the first key set. The
    /// accent must not be a grey: many themes set a dull button or focus colour.
    fn vivid(&self, keys: &[&str], on: Rgba) -> Option<Rgba> {
        let chroma = |c: &Rgba| c.r.max(c.g).max(c.b) - c.r.min(c.g).min(c.b);
        keys.iter()
            .filter_map(|key| self.0.get(*key).map(|c| over(*c, on)))
            .find(|c| chroma(c) >= MIN_CHROMA)
            .or_else(|| self.solid(keys, on))
    }

    /// The first key in the chain with a grey colour that is not `not`. Some
    /// themes put the text colour or an accent under a quiet key.
    fn neutral(&self, keys: &[&str], on: Rgba, not: Rgba) -> Option<Rgba> {
        keys.iter()
            .filter_map(|key| self.0.get(*key).map(|c| over(*c, on)))
            .find(|c| saturation(*c) < NEUTRAL_SATURATION && c.to_hex() != not.to_hex())
    }
}

/// The HSL saturation: 0 for a grey, 1 for a pure hue.
fn saturation(c: Rgba) -> f32 {
    let (hi, lo) = (c.r.max(c.g).max(c.b), c.r.min(c.g).min(c.b));
    let spread = 1.0 - (hi + lo - 1.0).abs();
    if spread <= f32::EPSILON {
        0.0
    } else {
        (hi - lo) / spread
    }
}

const GROUND: [&str; 3] = [
    "editor.background",
    "editorPane.background",
    "tab.activeBackground",
];
const TEXT: [&str; 3] = [
    "editor.foreground",
    "foreground",
    "editorLineNumber.activeForeground",
];
const CHROME: [&str; 5] = [
    "sideBar.background",
    "activityBar.background",
    "panel.background",
    "editorWidget.background",
    "menu.background",
];
const MUTED: [&str; 3] = [
    "descriptionForeground",
    "editorLineNumber.activeForeground",
    "input.placeholderForeground",
];
const FAINT: [&str; 3] = [
    "icon.foreground",
    "editorLineNumber.foreground",
    "tab.inactiveForeground",
];
const ACCENT: [&str; 6] = [
    // Our accent is a link colour, so the link key goes first. A focus border
    // is often a dull grey-blue.
    "textLink.foreground",
    "button.background",
    "activityBarBadge.background",
    "progressBar.background",
    "focusBorder",
    "editorCursor.foreground",
];
const WARN: [&str; 3] = [
    "editorWarning.foreground",
    "list.warningForeground",
    "terminal.ansiYellow",
];

/// What stands in for a colour that the theme does not set. The values are those
/// of Paper and Ink.
struct Base {
    bg: Rgba,
    fg: Rgba,
    acc: Rgba,
    warn: Rgba,
}

fn base(appearance: Appearance) -> Base {
    let [bg, fg, acc, warn] = match appearance {
        Appearance::Light => [0xfaf9f6, 0x1b1a17, 0xb0563a, 0x8a6a1f],
        Appearance::Dark => [0x15140f, 0xede8de, 0xc97354, 0xc0a15a],
    };
    Base {
        bg: Rgba::hex(bg),
        fg: Rgba::hex(fg),
        acc: Rgba::hex(acc),
        warn: Rgba::hex(warn),
    }
}

/// Light or dark. The `type` of the file, then the `uiTheme` of the manifest,
/// then the luminance of the page. The high-contrast pair map to the plain two.
pub fn appearance_of(file: &File, ui_theme: Option<&str>) -> Appearance {
    let said = file
        .kind
        .as_deref()
        .or(ui_theme)
        .map(str::to_ascii_lowercase);
    match said.as_deref() {
        Some("light" | "hclight" | "vs" | "hc-light") => Appearance::Light,
        Some("dark" | "hc" | "hcblack" | "vs-dark" | "hc-black") => Appearance::Dark,
        _ => match Keys::of(&file.colors).pick(&GROUND) {
            Some(ground) if luminance(ground) > 0.18 => Appearance::Light,
            _ => Appearance::Dark,
        },
    }
}

/// Whether the file sets any key that this app reads. A file with only
/// `tokenColors` has no opinion about the app, and no theme comes from it.
pub fn says_anything(file: &File) -> bool {
    let keys = Keys::of(&file.colors);
    [&GROUND[..], &TEXT[..], &ACCENT[..]]
        .iter()
        .any(|chain| keys.pick(chain).is_some())
}

/// The seven colours of a theme. The order matters. The page and the text come
/// first. A surface comes before the text drawn on it.
pub fn to_colors(file: &File, appearance: Appearance) -> Colors {
    let keys = Keys::of(&file.colors);
    let base = base(appearance);
    let light = appearance == Appearance::Light;

    let bg = keys.solid(&GROUND, base.bg).unwrap_or(base.bg);
    let fg = keys.solid(&TEXT, bg).unwrap_or(base.fg);
    // A nav ground that equals the page has no edge. Take a step off the page.
    let bg2 = keys
        .solid(&CHROME, bg)
        .filter(|c| contrast(*c, bg) >= 1.02)
        .unwrap_or_else(|| toward(bg, fg, if light { 0.05 } else { 0.07 }));

    let fg = readable(readable(fg, bg, TEXT_FLOOR), bg2, TEXT_FLOOR);
    let muted = keys
        .neutral(&MUTED, bg2, fg)
        .unwrap_or_else(|| toward(fg, bg, 0.32));
    let muted = readable(readable(muted, bg, TEXT_FLOOR), bg2, TEXT_FLOOR);
    let faint = keys
        .neutral(&FAINT, bg2, fg)
        .unwrap_or_else(|| toward(fg, bg, 0.48));
    let faint = readable(faint, bg, FAINT_FLOOR);
    let acc = keys.vivid(&ACCENT, bg).unwrap_or(base.acc);
    let acc = readable(acc, bg, TEXT_FLOOR);
    let warn = readable(keys.solid(&WARN, bg).unwrap_or(base.warn), bg, FAINT_FLOOR);

    Colors {
        bg: bg.to_hex(),
        bg2: bg2.to_hex(),
        fg: fg.to_hex(),
        muted: muted.to_hex(),
        faint: faint.to_hex(),
        acc: acc.to_hex(),
        warn: warn.to_hex(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> HashMap<String, String> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn colors(json: &str, appearance: Appearance) -> Colors {
        to_colors(&parse(json).unwrap(), appearance)
    }

    fn rgba(hex: &str) -> Rgba {
        Rgba::read(hex).unwrap()
    }

    #[test]
    fn jsonc_loses_comments_and_trailing_commas_but_not_strings() {
        let text = r##"{
            // a licence header
            "colors": { "a": "#fff", /* inline */ "b": "http://x//y", },
            "list": [1, 2,],
        }"##;
        let value: Value = serde_json::from_str(&strip(text)).unwrap();
        assert_eq!(value["colors"]["b"], "http://x//y");
        assert_eq!(value["list"], serde_json::json!([1, 2]));
        // An escaped quote does not end the string.
        let value: Value =
            serde_json::from_str(&strip(r#"{"a": "q\" // not a comment",}"#)).unwrap();
        assert_eq!(value["a"], "q\" // not a comment");
    }

    #[test]
    fn an_include_chain_is_laid_flat_with_the_nearest_file_last() {
        let all = files(&[
            (
                "t/light.json",
                r##"{"include":"./dark.json","colors":{"editor.background":"#ffffff"}}"##,
            ),
            (
                "t/dark.json",
                r##"{"type":"dark","colors":{"editor.background":"#101010","editor.foreground":"#eeeeee"}}"##,
            ),
        ]);
        let file = resolve("t/light.json", &all).unwrap();
        assert_eq!(file.colors["editor.background"], "#ffffff");
        assert_eq!(file.colors["editor.foreground"], "#eeeeee");
        assert_eq!(file.kind.as_deref(), Some("dark"));
    }

    #[test]
    fn a_cycle_and_a_missing_include_end_the_walk() {
        let all = files(&[
            (
                "a.json",
                r##"{"include":"b.json","colors":{"foreground":"#111111"}}"##,
            ),
            (
                "b.json",
                r##"{"include":"a.json","colors":{"editor.background":"#222222"}}"##,
            ),
            (
                "c.json",
                r##"{"include":"gone.json","colors":{"foreground":"#333333"}}"##,
            ),
        ]);
        assert_eq!(resolve("a.json", &all).unwrap().colors.len(), 2);
        assert_eq!(resolve("c.json", &all).unwrap().colors.len(), 1);
        assert!(resolve("none.json", &all).is_err());
    }

    #[test]
    fn an_include_resolves_against_the_folder_of_the_file_that_names_it() {
        assert_eq!(join("themes/a.json", "../base/b.json"), "base/b.json");
        assert_eq!(join("a.json", "./b.json"), "b.json");
    }

    #[test]
    fn the_include_depth_is_capped() {
        let mut list = Vec::new();
        for i in 0..20 {
            list.push((
                format!("{i}.json"),
                format!(
                    r##"{{"include":"{}.json","colors":{{"k{i}":"#000000"}}}}"##,
                    i + 1
                ),
            ));
        }
        let all: HashMap<_, _> = list.into_iter().collect();
        assert_eq!(
            resolve("0.json", &all).unwrap().colors.len(),
            MAX_INCLUDE_DEPTH
        );
    }

    #[test]
    fn colours_read_in_all_four_hex_forms_and_nothing_else() {
        assert_eq!(rgba("#fff"), Rgba::hex(0xffffff));
        assert_eq!(rgba("#ffffff"), Rgba::hex(0xffffff));
        assert!((rgba("#00000080").a - 128.0 / 255.0).abs() < 1e-6);
        assert!((rgba("#0008").a - 136.0 / 255.0).abs() < 1e-6);
        assert!(Rgba::read("red").is_none() && Rgba::read("#12345").is_none());
        assert!(Rgba::read("#ggg").is_none());
        assert_eq!(rgba("#B0563A").to_hex(), "#b0563a");
    }

    #[test]
    fn a_muted_key_with_the_text_colour_or_a_hue_is_not_taken() {
        // Catppuccin Frappe: the quiet keys hold the text colour and mauve.
        let c = colors(
            r##"{"colors":{
                "editor.background":"#303446","editor.foreground":"#c6d0f5",
                "sideBar.background":"#292c3c","descriptionForeground":"#c6d0f5",
                "editorLineNumber.activeForeground":"#babbf1",
                "icon.foreground":"#ca9ee6","editorLineNumber.foreground":"#838ba7"}}"##,
            Appearance::Dark,
        );
        assert_eq!(c.fg, "#c6d0f5");
        assert_ne!(c.muted, c.fg);
        assert!(saturation(rgba(&c.muted)) < NEUTRAL_SATURATION);
        assert_eq!(c.faint, "#838ba7");
    }

    #[test]
    fn theme_keys_land_on_the_seven_colours() {
        let c = colors(
            r##"{"colors":{
                "editor.background":"#1e1e2e","editor.foreground":"#cdd6f4",
                "sideBar.background":"#181825","descriptionForeground":"#a6adc8",
                "icon.foreground":"#7f849c","focusBorder":"#89b4fa",
                "editorWarning.foreground":"#f9e2af"}}"##,
            Appearance::Dark,
        );
        assert_eq!(c.bg, "#1e1e2e");
        assert_eq!(c.bg2, "#181825");
        assert_eq!(c.fg, "#cdd6f4");
        assert_eq!(c.muted, "#a6adc8");
        assert_eq!(c.faint, "#7f849c");
        assert_eq!(c.acc, "#89b4fa");
        assert_eq!(c.warn, "#f9e2af");
    }

    #[test]
    fn a_translucent_key_is_flattened_onto_the_page() {
        let c = colors(
            r##"{"colors":{"editor.background":"#000000","sideBar.background":"#ffffff80"}}"##,
            Appearance::Dark,
        );
        assert_eq!(c.bg2, "#808080");
    }

    #[test]
    fn an_empty_theme_still_gets_seven_valid_colours() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let c = colors(r#"{"colors":{}}"#, appearance);
            for hex in [&c.bg, &c.bg2, &c.fg, &c.muted, &c.faint, &c.acc, &c.warn] {
                assert!(Rgba::read(hex).is_some(), "{hex}");
            }
            assert!(contrast(rgba(&c.fg), rgba(&c.bg)) >= TEXT_FLOOR);
        }
    }

    #[test]
    fn text_that_cannot_be_read_moves_away_from_its_page() {
        // Dim grey text on a dark page, and a pale accent on a white page.
        let c = colors(
            r##"{"colors":{"editor.background":"#101010","editor.foreground":"#202020",
                "descriptionForeground":"#262626","icon.foreground":"#141414"}}"##,
            Appearance::Dark,
        );
        let (bg, bg2) = (rgba(&c.bg), rgba(&c.bg2));
        assert!(contrast(rgba(&c.fg), bg) >= TEXT_FLOOR);
        assert!(contrast(rgba(&c.fg), bg2) >= TEXT_FLOOR);
        assert!(contrast(rgba(&c.muted), bg) >= TEXT_FLOOR);
        assert!(contrast(rgba(&c.faint), bg) >= FAINT_FLOOR);
        let c = colors(
            r##"{"colors":{"editor.background":"#ffffff","focusBorder":"#ffe0e0"}}"##,
            Appearance::Light,
        );
        assert!(contrast(rgba(&c.acc), rgba(&c.bg)) >= TEXT_FLOOR);
    }

    #[test]
    fn the_accent_skips_a_grey_key_for_a_key_with_a_hue() {
        let c = colors(
            r##"{"colors":{"editor.background":"#101010","textLink.foreground":"#808080",
                "progressBar.background":"#ff79c6"}}"##,
            Appearance::Dark,
        );
        assert_eq!(c.acc, "#ff79c6");
        // All grey: the first key set wins.
        let c = colors(
            r##"{"colors":{"editor.background":"#101010","textLink.foreground":"#c0c0c0",
                "focusBorder":"#a0a0a0"}}"##,
            Appearance::Dark,
        );
        assert_eq!(c.acc, "#c0c0c0");
    }

    #[test]
    fn a_colour_that_already_reads_is_left_alone() {
        let (white, black) = (rgba("#ffffff"), rgba("#000000"));
        assert_eq!(readable(white, black, TEXT_FLOOR), white);
        assert!((contrast(white, black) - 21.0).abs() < 1e-3);
    }

    #[test]
    fn appearance_comes_from_type_then_manifest_then_the_page() {
        let dark_page = parse(r##"{"colors":{"editor.background":"#101010"}}"##).unwrap();
        assert_eq!(appearance_of(&dark_page, None), Appearance::Dark);
        assert_eq!(appearance_of(&dark_page, Some("vs")), Appearance::Light);
        let typed =
            parse(r##"{"type":"hcLight","colors":{"editor.background":"#101010"}}"##).unwrap();
        assert_eq!(appearance_of(&typed, Some("vs-dark")), Appearance::Light);
        let light_page = parse(r##"{"colors":{"editor.background":"#fafafa"}}"##).unwrap();
        assert_eq!(appearance_of(&light_page, None), Appearance::Light);
    }

    #[test]
    fn a_file_with_only_token_colours_says_nothing() {
        let tokens =
            parse(r##"{"tokenColors":[{"scope":"x","settings":{"foreground":"#fff"}}]}"##).unwrap();
        assert!(!says_anything(&tokens));
        let theme = parse(r##"{"colors":{"foreground":"#fff"}}"##).unwrap();
        assert!(says_anything(&theme));
    }
}
