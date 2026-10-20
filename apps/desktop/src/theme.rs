//! Theme tokens, ported from `app/src/styles.css`.
//!
//! A palette authors seven colours and a few numbers. The hairlines, the
//! selection tint and the icon back layer come from those. This is the same
//! split as `crates/garmin-core/src/theme.rs` and `app/src/lib/customTheme.ts`.
//! No colour literal lives outside this file.

use std::sync::Mutex;

use gpui::{rgb, rgb_to_hsla, ColorExt, Hsla, Rgba};

/// The motion tokens. Durations in milliseconds. The slow step is the page entrance.
pub const DUR_SLOW: u64 = 180;

/// The type scale. Every size below display comes from these six steps.
pub const FS_MICRO: f32 = 12.0;
pub const FS_CAPTION: f32 = 13.0;
pub const FS_SMALL: f32 = 14.0;
pub const FS_BASE: f32 = 15.0;
pub const FS_MD: f32 = 16.0;
pub const FS_LG: f32 = 17.5;

/// One palette. Colours are final: derived tokens are already applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub dark: bool,
    /// The page.
    pub bg: Hsla,
    /// The nav ground, one step off the page.
    pub bg2: Hsla,
    pub fg: Hsla,
    pub mutd: Hsla,
    pub faint: Hsla,
    /// The divider between sections and the outline of a control.
    pub line: Hsla,
    /// The divider between rows of one list.
    pub line2: Hsla,
    pub acc: Hsla,
    pub warn: Hsla,
    /// The tint under the pointer.
    pub sel: Hsla,
    /// The back layer of a duotone icon, with its alpha applied.
    pub icon2: Hsla,
    /// The hairline lift under the lit nav row. The stylesheet fixes it for every palette.
    pub shadow: Hsla,
    /// The accent as a wash, and as a hairline.
    pub acctint: Hsla,
    pub accline: Hsla,
    /// The warning colour as a wash, and as a hairline.
    pub warntint: Hsla,
    pub warnline: Hsla,
    /// The dim behind a dialog.
    pub scrim: Hsla,
    /// The drop shadow of a dialog.
    pub dshadow: Hsla,
}

/// The seven authored colours and the one authored alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Authored {
    dark: bool,
    bg: u32,
    bg2: u32,
    fg: u32,
    mutd: u32,
    faint: u32,
    acc: u32,
    warn: u32,
    /// `None` takes the default for the appearance.
    icon_alpha: Option<f32>,
}

/// A theme from the themes folder. It is leaked once per distinct theme, so a
/// `Palette` stays `Copy`. The cost is a few bytes for each theme file.
#[derive(Debug, PartialEq)]
pub struct Custom {
    id: String,
    name: String,
    authored: Authored,
}

/// The built-in palettes, in the order Settings lists them, and a custom one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Palette {
    Paper,
    Ink,
    Nocturne,
    Moss,
    Newsprint,
    Linen,
    Custom(&'static Custom),
}

/// The prefix of a custom id, so a theme named "Ink" never takes the place of Ink.
const CUSTOM_PREFIX: &str = "custom:";

/// A colour as the themes folder writes it: `#rgb` or `#rrggbb`.
fn parse_hex(text: &str) -> Option<u32> {
    let body = text.strip_prefix('#')?;
    let full: String = match body.len() {
        3 => body.chars().flat_map(|c| [c, c]).collect(),
        6 => body.to_string(),
        _ => return None,
    };
    u32::from_str_radix(&full, 16).ok()
}

impl Palette {
    pub const ALL: [Palette; 6] = [
        Palette::Paper,
        Palette::Ink,
        Palette::Nocturne,
        Palette::Moss,
        Palette::Newsprint,
        Palette::Linen,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Palette::Paper => "Paper",
            Palette::Ink => "Ink",
            Palette::Nocturne => "Nocturne",
            Palette::Moss => "Moss",
            Palette::Newsprint => "Newsprint",
            Palette::Linen => "Linen",
            Palette::Custom(c) => &c.name,
        }
    }

    /// The saved form: the lowercase name, or `custom:` and the slug.
    pub fn id(self) -> String {
        match self {
            Palette::Custom(c) => c.id.clone(),
            built_in => built_in.name().to_ascii_lowercase(),
        }
    }

    /// The palette for a saved id. An empty, unknown or removed id gives `None`.
    pub fn from_id(id: &str, custom: &[garmin_core::theme::Theme]) -> Option<Palette> {
        if let Some(slug) = id.strip_prefix(CUSTOM_PREFIX) {
            return custom
                .iter()
                .find(|t| t.slug == slug)
                .and_then(Palette::custom);
        }
        Palette::ALL
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(id))
    }

    /// A palette from a theme file. `None` when a colour does not parse.
    pub fn custom(theme: &garmin_core::theme::Theme) -> Option<Palette> {
        let c = &theme.colors;
        let authored = Authored {
            dark: theme.appearance == garmin_core::theme::Appearance::Dark,
            bg: parse_hex(&c.bg)?,
            bg2: parse_hex(&c.bg2)?,
            fg: parse_hex(&c.fg)?,
            mutd: parse_hex(&c.muted)?,
            faint: parse_hex(&c.faint)?,
            acc: parse_hex(&c.acc)?,
            warn: parse_hex(&c.warn)?,
            icon_alpha: theme.icon_tint_alpha.map(|a| a as f32),
        };
        let made = Custom {
            id: format!("{CUSTOM_PREFIX}{}", theme.slug),
            name: theme.name.clone(),
            authored,
        };
        // Settings asks on each frame. The same theme must not leak twice.
        static SEEN: Mutex<Vec<&'static Custom>> = Mutex::new(Vec::new());
        let mut seen = SEEN.lock().ok()?;
        if let Some(same) = seen.iter().find(|c| ***c == made) {
            return Some(Palette::Custom(same));
        }
        let leaked: &'static Custom = Box::leak(Box::new(made));
        seen.push(leaked);
        Some(Palette::Custom(leaked))
    }

    /// Light or dark, for the picker that lists it.
    pub fn is_dark(self) -> bool {
        self.theme().dark
    }

    fn authored(self) -> Authored {
        let light = |bg, bg2, fg, mutd, faint, acc, warn, icon_alpha| Authored {
            dark: false,
            bg,
            bg2,
            fg,
            mutd,
            faint,
            acc,
            warn,
            icon_alpha,
        };
        let dark = |bg, bg2, fg, mutd, faint, acc, warn, icon_alpha| Authored {
            dark: true,
            bg,
            bg2,
            fg,
            mutd,
            faint,
            acc,
            warn,
            icon_alpha,
        };
        match self {
            Palette::Custom(c) => c.authored,
            Palette::Paper => light(
                0xfaf9f6, 0xf4f2ed, 0x1b1a17, 0x79736a, 0xa9a296, 0xb0563a, 0x8a6a1f, None,
            ),
            Palette::Ink => dark(
                0x15140f, 0x1c1b15, 0xede8de, 0x8f887c, 0x6b6459, 0xc97354, 0xc0a15a, None,
            ),
            Palette::Nocturne => dark(
                0x10141a,
                0x171d25,
                0xdde5ef,
                0x8695a6,
                0x5d6a79,
                0x6aa9d9,
                0xcfa45e,
                Some(0.36),
            ),
            Palette::Moss => dark(
                0x0f1411,
                0x161d18,
                0xe1e8dd,
                0x869184,
                0x606b5e,
                0x93bb7c,
                0xcbab61,
                Some(0.34),
            ),
            Palette::Newsprint => light(
                0xf7f7f8,
                0xefeff2,
                0x17181c,
                0x6d6f76,
                0x9b9da5,
                0x2f5d8c,
                0x7a6420,
                Some(0.24),
            ),
            Palette::Linen => light(
                0xfbf7f0,
                0xf4efe4,
                0x201b1d,
                0x7c7069,
                0xada298,
                0x8a4a63,
                0x8a6a1f,
                Some(0.26),
            ),
        }
    }

    /// Light and dark Paper are two palettes here, as in the stylesheet.
    pub fn theme(self) -> Theme {
        derive(&self.authored())
    }
}

/// Component-wise mix in sRGB, like `color-mix(in srgb, a 74%, b)`.
fn mix(a: u32, b: u32, a_share: f32) -> Hsla {
    let (a, b) = (rgb(a), rgb(b));
    let m = |x: f32, y: f32| x * a_share + y * (1.0 - a_share);
    rgb_to_hsla(Rgba::new(
        m(a.red, b.red),
        m(a.green, b.green),
        m(a.blue, b.blue),
        1.0,
    ))
}

fn hex(value: u32) -> Hsla {
    rgb_to_hsla(rgb(value))
}

fn derive(a: &Authored) -> Theme {
    let fg = hex(a.fg);
    let acc = hex(a.acc);
    // The numbers are lifted from the built-in palettes in `styles.css`.
    let (line, line2, sel, default_alpha) = if a.dark {
        (0.13, 0.06, 0.05, 0.42)
    } else {
        (0.11, 0.055, 0.045, 0.3)
    };
    // On a dark page the accent is lifted toward the foreground first,
    // or the tint reads as a smudge. See the `--icon2` note in `styles.css`.
    let icon2 = if a.dark { mix(a.acc, a.fg, 0.74) } else { acc };
    let warn = hex(a.warn);
    let (acc_tint, warn_tint) = if a.dark { (0.22, 0.12) } else { (0.12, 0.07) };
    Theme {
        dark: a.dark,
        bg: hex(a.bg),
        bg2: hex(a.bg2),
        fg,
        mutd: hex(a.mutd),
        faint: hex(a.faint),
        line: fg.opacity(line),
        line2: fg.opacity(line2),
        acc,
        warn,
        sel: fg.opacity(sel),
        icon2: icon2.opacity(a.icon_alpha.unwrap_or(default_alpha)),
        shadow: hex(0x1b1a17).opacity(0.04),
        acctint: acc.opacity(acc_tint),
        accline: acc.opacity(0.4),
        warntint: warn.opacity(warn_tint),
        warnline: warn.opacity(0.4),
        scrim: fg.opacity(0.32),
        dshadow: fg.opacity(0.18),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_derives_hairlines_weaker_than_the_text() {
        for p in Palette::ALL {
            let t = p.theme();
            assert!(
                t.line.alpha < 0.2 && t.line.alpha > t.line2.alpha,
                "{}",
                p.name()
            );
            assert!(t.sel.alpha < t.line.alpha, "{}", p.name());
        }
    }

    #[test]
    fn the_tints_and_lines_follow_the_design_alphas() {
        let (paper, ink) = (Palette::Paper.theme(), Palette::Ink.theme());
        assert!(
            (paper.acctint.alpha - 0.12).abs() < 1e-6 && (ink.acctint.alpha - 0.22).abs() < 1e-6
        );
        assert!(
            (paper.warntint.alpha - 0.07).abs() < 1e-6 && (ink.warntint.alpha - 0.12).abs() < 1e-6
        );
        assert!((ink.accline.alpha - 0.4).abs() < 1e-6 && (ink.warnline.alpha - 0.4).abs() < 1e-6);
        assert!((ink.scrim.alpha - 0.32).abs() < 1e-6 && (ink.dshadow.alpha - 0.18).abs() < 1e-6);
    }

    #[test]
    fn dark_palettes_tune_the_icon_tint_the_way_the_stylesheet_does() {
        assert!((Palette::Paper.theme().icon2.alpha - 0.3).abs() < 1e-6);
        assert!((Palette::Ink.theme().icon2.alpha - 0.42).abs() < 1e-6);
        assert!((Palette::Moss.theme().icon2.alpha - 0.34).abs() < 1e-6);
        assert!(Palette::Ink.theme().dark && !Palette::Linen.theme().dark);
    }

    #[test]
    fn a_built_in_id_round_trips_and_a_custom_id_never_takes_a_built_in_name() {
        for p in Palette::ALL {
            assert_eq!(Palette::from_id(&p.id(), &[]), Some(p));
        }
        assert_eq!(Palette::from_id("", &[]), None);
        assert_eq!(Palette::from_id("custom:ink", &[]), None);
    }

    #[test]
    fn a_theme_file_becomes_a_palette_and_the_same_file_leaks_once() {
        use garmin_core::theme as folder;
        let theme = folder::Theme {
            slug: "nord".into(),
            name: "Nord".into(),
            appearance: folder::Appearance::Dark,
            note: String::new(),
            colors: folder::Colors {
                bg: "#2e3440".into(),
                bg2: "#3b4252".into(),
                fg: "#d8dee9".into(),
                muted: "#a3adbd".into(),
                faint: "#7b88a1".into(),
                acc: "#88c0d0".into(),
                warn: "#ebcb8b".into(),
            },
            icon_tint_alpha: None,
        };
        let (a, b) = (
            Palette::custom(&theme).unwrap(),
            Palette::custom(&theme).unwrap(),
        );
        let (Palette::Custom(a_ref), Palette::Custom(b_ref)) = (a, b) else {
            panic!("not custom");
        };
        assert!(std::ptr::eq(a_ref, b_ref));
        assert_eq!(a.id(), "custom:nord");
        assert!(a.theme().dark);
        assert_eq!(a.theme().bg, hex(0x2e3440));
        let mut broken = theme;
        broken.colors.acc = "blue".into();
        assert_eq!(Palette::custom(&broken), None);
        assert_eq!(parse_hex("#abc"), Some(0xaabbcc));
    }
}
