//! Custom themes, stored as one JSON file each in a folder the user can open.
//!
//! A folder of small files rather than rows in the cache, because the point of
//! a custom theme is that it is yours: you can read one, copy it to make the
//! next, mail it to someone, or drop one in from outside the app and have it
//! appear. None of that is true of a blob in SQLite, and a theme is not data
//! the sync has any opinion about.
//!
//! # What a theme actually declares
//!
//! Seven colours and a name. Everything else the stylesheet needs — the two
//! hairlines, the selection tint, the elevation, the duotone icons' back layer
//! — is derived from those seven on the frontend, in `lib/customTheme.ts`.
//!
//! That split is deliberate and it is the reason this is writable by a language
//! model at all. The derived tokens are the ones with a correct answer: a
//! hairline is the foreground at 12%, and a theme where it isn't is a theme
//! that looks broken. Asking an author — human or model — to supply
//! `rgba(237, 232, 222, 0.13)` invites exactly one kind of mistake and offers
//! nothing in return. What's left is the part that is genuinely a choice.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where themes live: alongside the cache, not inside it.
pub fn themes_dir() -> Result<PathBuf> {
    let dir = crate::paths::base_dir()?.join("themes");
    std::fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;
    Ok(dir)
}

/// Light or dark. A theme is one or the other by construction — see the note in
/// `lib/theme.ts` on why a palette settles that question rather than answering
/// to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
}

/// The seven authored colours.
///
/// Named for the job each does rather than for what it looks like, matching the
/// CSS variables one for one — `muted` is `--mut`, which can't be spelled that
/// way in Rust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Colors {
    /// The page.
    pub bg: String,
    /// The nav's ground: a step off the page, in whichever direction that
    /// palette's page leaves room for.
    pub bg2: String,
    /// Body text.
    pub fg: String,
    /// Secondary text — labels, notes, anything supporting.
    pub muted: String,
    /// The quietest readable step. Captions and asides.
    pub faint: String,
    /// The one colour that isn't a grey. Links, the selected marker, the tint
    /// behind duotone icons.
    pub acc: String,
    /// Reserved for warnings, and nothing else.
    pub warn: String,
}

impl Colors {
    fn each(&self) -> [(&'static str, &str); 7] {
        [
            ("bg", &self.bg),
            ("bg2", &self.bg2),
            ("fg", &self.fg),
            ("muted", &self.muted),
            ("faint", &self.faint),
            ("acc", &self.acc),
            ("warn", &self.warn),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    /// The file's stem. Assigned on save from the name, never authored — a
    /// theme whose file says one thing and whose slug says another is a theme
    /// you can't find again.
    #[serde(default)]
    pub slug: String,
    pub name: String,
    pub appearance: Appearance,
    /// One short line, shown beside the name in the palette list.
    #[serde(default)]
    pub note: String,
    pub colors: Colors,
    /// How strongly the accent tints the back layer of a duotone icon. Optional
    /// because there is a sensible answer per appearance; present because the
    /// right value depends on how light the accent is, which only the author
    /// knows. See the `--icon2-a` note in `styles.css`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_tint_alpha: Option<f64>,
}

/// `#rgb` or `#rrggbb`, and nothing else.
///
/// Not a matter of taste: these strings are interpolated into CSS custom
/// properties and into `color-mix()` calls that derive the rest of the palette,
/// so a value that isn't a hex colour is either a broken window or, if it
/// contained a brace, a way to write arbitrary CSS from a tool call.
fn check_hex(field: &str, value: &str) -> Result<()> {
    let body = value.strip_prefix('#').unwrap_or("");
    let ok = matches!(body.len(), 3 | 6) && body.chars().all(|c| c.is_ascii_hexdigit());
    if !ok {
        bail!("`{field}` is {value:?}, which is not a hex colour — write it as #rrggbb");
    }
    Ok(())
}

/// Lowercase letters and digits, joined by single hyphens.
///
/// Built from scratch rather than by sanitising what it's given, which is what
/// makes it safe to use as a filename: only characters that pass the filter can
/// appear, so no amount of `../` in a name can produce a path that leaves the
/// themes folder. Everything else collapses to a separator.
///
/// Letters, not ASCII letters. Filenames are UTF-8 and a slug never leaves this
/// machine, so there's no reason "Čaj" should file itself as `aj`.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

impl Theme {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("a theme needs a name");
        }
        if slugify(&self.name).is_empty() {
            bail!(
                "`{}` has no letters or digits in it to make a filename from",
                self.name
            );
        }
        for (field, value) in self.colors.each() {
            check_hex(field, value)?;
        }
        if let Some(a) = self.icon_tint_alpha {
            if !(0.0..=1.0).contains(&a) {
                bail!("`iconTintAlpha` is {a}, which is outside 0–1");
            }
        }
        Ok(())
    }
}

/// Every theme in the folder, by name.
///
/// A file that doesn't parse is skipped rather than failing the read: the
/// folder is meant to be edited by hand, and one bad file part-way through an
/// edit shouldn't take the theme picker down with it. The rest still load, and
/// the broken one simply isn't offered.
pub fn list() -> Result<Vec<Theme>> {
    list_in(&themes_dir()?)
}

fn list_in(dir: &Path) -> Result<Vec<Theme>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("could not read {}", dir.display()))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut theme) = serde_json::from_str::<Theme>(&raw) else {
            continue;
        };
        if theme.validate().is_err() {
            continue;
        }
        // The filename wins. It's what the selection is stored as, so a `slug`
        // typed into the file can't be allowed to disagree with it.
        theme.slug = stem.to_string();
        out.push(theme);
    }
    out.sort_by_key(|t| t.name.to_lowercase());
    Ok(out)
}

/// Write a theme, returning it with the slug it was actually filed under.
///
/// Saving under an existing slug overwrites, which is what makes "make it a bit
/// warmer" work as a second tool call rather than as a pile of near-duplicates.
/// Renaming therefore forks: the new name is a new file, and the old one stays
/// until it's deleted.
pub fn save(theme: Theme) -> Result<Theme> {
    save_in(&themes_dir()?, theme)
}

fn save_in(dir: &Path, mut theme: Theme) -> Result<Theme> {
    theme.validate()?;
    theme.name = theme.name.trim().to_string();
    theme.note = theme.note.trim().to_string();
    theme.slug = slugify(&theme.name);

    let path = dir.join(format!("{}.json", theme.slug));
    let json = serde_json::to_string_pretty(&theme)? + "\n";
    std::fs::write(&path, json).with_context(|| format!("could not write {}", path.display()))?;
    Ok(theme)
}

pub fn delete(slug: &str) -> Result<()> {
    delete_in(&themes_dir()?, slug)
}

fn delete_in(dir: &Path, slug: &str) -> Result<()> {
    // Rebuilt rather than trusted: `slug` arrives from the frontend and, via a
    // tool call, from a model. `../` in it must not reach outside the folder.
    let clean = slugify(slug);
    if clean.is_empty() {
        bail!("no such theme");
    }
    let path = dir.join(format!("{clean}.json"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        // Already gone is the state that was asked for.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("could not delete {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors() -> Colors {
        Colors {
            bg: "#ffffff".into(),
            bg2: "#eeeeee".into(),
            fg: "#111111".into(),
            muted: "#777777".into(),
            faint: "#aaaaaa".into(),
            acc: "#b0563a".into(),
            warn: "#8a6a1f".into(),
        }
    }

    fn theme(name: &str) -> Theme {
        Theme {
            slug: String::new(),
            name: name.into(),
            appearance: Appearance::Light,
            note: String::new(),
            colors: colors(),
            icon_tint_alpha: None,
        }
    }

    #[test]
    fn slugs_are_filenames_and_nothing_else() {
        assert_eq!(slugify("Rose Quartz"), "rose-quartz");
        assert_eq!(slugify("  Deep   Sea  "), "deep-sea");
        // Diacritics survive, folded to lowercase, rather than being punched
        // out into separators.
        assert_eq!(slugify("Čaj Ünïcode 2"), "čaj-ünïcode-2");
        // The one that matters: a slug can never climb out of the folder.
        assert_eq!(slugify("../../etc/passwd"), "etc-passwd");
        assert_eq!(slugify("..\\..\\windows"), "windows");
        assert_eq!(slugify("///"), "");
    }

    #[test]
    fn colours_must_be_hex() {
        let mut t = theme("Bad");
        t.colors.acc = "red".into();
        assert!(t.validate().is_err());

        // The case the hex check is really there for: anything that could carry
        // its own declarations into the stylesheet.
        t.colors.acc = "#fff; } :root { display:none".into();
        assert!(t.validate().is_err());

        t.colors.acc = "#fff".into();
        assert!(t.validate().is_ok());
    }

    #[test]
    fn a_theme_needs_a_usable_name() {
        assert!(theme("  ").validate().is_err());
        assert!(theme("!!!").validate().is_err());
        assert!(theme("Ok").validate().is_ok());
    }

    /// A scratch folder, so none of this touches the real themes directory.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("garmin-theme-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_theme_round_trips_through_the_folder() {
        let dir = scratch("roundtrip");

        let mut t = theme("Rose Quartz");
        t.note = "  Pale pink, deep rose  ".into();
        let saved = save_in(&dir, t).unwrap();
        // The slug is derived on save, which is what the caller needs back:
        // it's what the selection gets stored as.
        assert_eq!(saved.slug, "rose-quartz");
        assert_eq!(saved.note, "Pale pink, deep rose");
        assert!(dir.join("rose-quartz.json").exists());

        let listed = list_in(&dir).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Rose Quartz");
        assert_eq!(listed[0].slug, "rose-quartz");
        assert_eq!(listed[0].colors.acc, "#b0563a");

        // Saving under the same name revises rather than accumulating — this is
        // what "make it warmer" does on a second tool call.
        let mut again = theme("Rose Quartz");
        again.colors.acc = "#c97354".into();
        save_in(&dir, again).unwrap();
        let listed = list_in(&dir).unwrap();
        assert_eq!(listed.len(), 1, "a revision must not fork the theme");
        assert_eq!(listed[0].colors.acc, "#c97354");

        delete_in(&dir, "rose-quartz").unwrap();
        assert!(list_in(&dir).unwrap().is_empty());
        // Deleting what is already gone is the state that was asked for.
        assert!(delete_in(&dir, "rose-quartz").is_ok());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_filename_is_the_slug_whatever_the_file_claims() {
        let dir = scratch("slug");
        // Hand-written files are expected here, so the two can disagree.
        std::fs::write(
            dir.join("from-disk.json"),
            r##"{"slug":"lies","name":"From Disk","appearance":"dark",
                "colors":{"bg":"#000","bg2":"#111","fg":"#fff","muted":"#888",
                          "faint":"#555","acc":"#0af","warn":"#fa0"}}"##,
        )
        .unwrap();

        let listed = list_in(&dir).unwrap();
        assert_eq!(listed[0].slug, "from-disk");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn one_broken_file_does_not_hide_the_rest() {
        let dir = scratch("broken");
        save_in(&dir, theme("Good")).unwrap();
        std::fs::write(dir.join("half-written.json"), "{ \"name\": ").unwrap();
        // Parses, but `acc` isn't a colour — must be skipped, not offered.
        std::fs::write(
            dir.join("invalid.json"),
            r##"{"name":"Bad","appearance":"light",
                "colors":{"bg":"#fff","bg2":"#eee","fg":"#000","muted":"#888",
                          "faint":"#555","acc":"javascript:alert(1)","warn":"#fa0"}}"##,
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "not a theme").unwrap();

        let listed = list_in(&dir).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Good");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn alpha_stays_in_range() {
        let mut t = theme("Tint");
        t.icon_tint_alpha = Some(1.4);
        assert!(t.validate().is_err());
        t.icon_tint_alpha = Some(0.4);
        assert!(t.validate().is_ok());
    }
}
