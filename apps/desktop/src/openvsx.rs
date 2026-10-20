//! Themes from open-vsx.org: search, download, and write to the themes folder.
//!
//! open-vsx is the open registry of the Eclipse Foundation. Its REST API needs
//! no key. A `.vsix` is a zip. `extension/package.json` lists the themes in
//! `contributes.themes`. Each one becomes one file in the themes folder of
//! `garmin_core::theme`, in the seven-colour format. The rest of the archive is
//! read past and dropped.
//!
//! The file name comes from the theme name through `garmin_core::theme::slugify`.
//! No path in the archive reaches the disk. The port comes from
//! `mbrd-rs/crates/app/src/openvsx.rs`.

use std::collections::HashMap;
use std::io::Read as _;
use std::time::Duration;

use anyhow::{bail, ensure, Context as _, Result};
use garmin_core::theme::{self, Theme};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::vscode;

/// A search answer or a lookup answer is a small JSON object.
const SEARCH_CEILING: u64 = 2 * 1024 * 1024;
/// A colour theme is a few kilobytes. The size is for screenshots in the archive.
const VSIX_CEILING: u64 = 24 * 1024 * 1024;
/// One file in the archive. The claimed size and the real read are both bounded.
const ENTRY_CEILING: u64 = 4 * 1024 * 1024;
/// One screen of results, not a catalogue.
const RESULTS: usize = 25;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(120);

/// One extension, as a result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub namespace: String,
    pub name: String,
    /// The `displayName`, or the `name` when there is none.
    pub display: String,
    pub description: String,
    pub downloads: u64,
    /// The `.vsix` URL. Empty means ask again by name at install time.
    pub vsix: String,
}

#[derive(Debug, Deserialize)]
struct Answer {
    #[serde(default)]
    extensions: Vec<Listing>,
}

/// One extension in a search answer. A lookup by name answers with the same object.
#[derive(Debug, Deserialize)]
struct Listing {
    namespace: String,
    name: String,
    #[serde(rename = "displayName", default)]
    display: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(rename = "downloadCount", default)]
    downloads: u64,
    #[serde(default)]
    files: Downloads,
}

#[derive(Debug, Default, Deserialize)]
struct Downloads {
    #[serde(default)]
    download: Option<String>,
}

impl Listing {
    fn into_found(self) -> Found {
        Found {
            display: self.display.unwrap_or_else(|| self.name.clone()),
            description: self.description.unwrap_or_default(),
            downloads: self.downloads,
            vsix: self.files.download.unwrap_or_default(),
            namespace: self.namespace,
            name: self.name,
        }
    }
}

/// An extension's `package.json`, down to the one array that this reads.
#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    contributes: Contributes,
}

#[derive(Debug, Default, Deserialize)]
struct Contributes {
    #[serde(default)]
    themes: Vec<Contributed>,
}

/// One theme that an extension contributes. `path` is relative to the extension root.
#[derive(Debug, Deserialize)]
struct Contributed {
    #[serde(default)]
    label: Option<String>,
    #[serde(rename = "uiTheme", default)]
    ui: Option<String>,
    path: String,
}

/// Search for themes. The registry filters by category. The sort is by download
/// count: the relevance sort puts forks of a theme before the one that was meant.
pub async fn search(query: &str) -> Result<Vec<Found>> {
    let url = format!(
        "https://open-vsx.org/api/-/search?query={}&category=Themes&size={RESULTS}&sortBy=downloadCount&sortOrder=desc",
        escape(query)
    );
    let body = fetch(&url, SEARCH_CEILING, SEARCH_TIMEOUT).await?;
    let answer: Answer = serde_json::from_slice(&body)
        .context("open-vsx answered with something that is not a search result")?;
    Ok(answer
        .extensions
        .into_iter()
        .map(Listing::into_found)
        .collect())
}

/// Download one extension and save each theme in it. Answers with the saved
/// names. One theme that cannot be read does not stop the others.
pub async fn install(found: &Found) -> Result<Vec<String>> {
    let url = download_url(found).await?;
    let archive = fetch(&url, VSIX_CEILING, INSTALL_TIMEOUT).await?;
    let themes = convert(&unpack(&archive)?, found)?;
    let mut names = Vec::new();
    for t in themes {
        names.push(theme::save(t)?.name);
    }
    Ok(names)
}

/// `--install-theme <query>` (debug builds): search, install the top result,
/// print the names and exit. It uses the real network and the themes folder.
#[cfg(debug_assertions)]
pub fn selftest(query: &str) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let found = search(query).await?;
        let top = found.first().context("no theme matches")?;
        println!("found {} ({} results)", top.display, found.len());
        println!("installed {:?}", install(top).await?);
        Ok(())
    })
}

/// The search answer usually has the URL. If not, ask for the extension by name.
async fn download_url(found: &Found) -> Result<String> {
    let url = if found.vsix.is_empty() {
        let at = format!(
            "https://open-vsx.org/api/{}/{}",
            escape(&found.namespace),
            escape(&found.name)
        );
        let body = fetch(&at, SEARCH_CEILING, SEARCH_TIMEOUT).await?;
        let listing: Listing = serde_json::from_slice(&body)
            .with_context(|| format!("{at} answered with something that is not an extension"))?;
        listing.files.download.unwrap_or_default()
    } else {
        found.vsix.clone()
    };
    ensure!(
        !url.is_empty(),
        "open-vsx has no download for {}",
        found.name
    );
    Ok(url)
}

/// Every JSON file in the archive, keyed by its path inside `extension/`.
/// A file that is not JSON is skipped without a read.
fn unpack(bytes: &[u8]) -> Result<HashMap<String, String>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .context("open-vsx sent something that is not an extension")?;
    let mut files = HashMap::new();
    for at in 0..archive.len() {
        let entry = archive
            .by_index(at)
            .context("could not read the extension")?;
        if !entry.is_file() {
            continue;
        }
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let path = path.to_string_lossy().replace('\\', "/");
        // A key is never a disk path here. Still, a `..` has no place in one.
        let Some(path) = path
            .strip_prefix("extension/")
            .filter(|p| !p.split('/').any(|s| s == ".."))
        else {
            continue;
        };
        // The claimed size first. The header is written by whoever made the zip,
        // so the read is bounded too.
        if !path.to_ascii_lowercase().ends_with(".json") || entry.size() > ENTRY_CEILING {
            continue;
        }
        let mut text = String::new();
        if entry.take(ENTRY_CEILING).read_to_string(&mut text).is_ok() {
            files.insert(path.to_string(), text);
        }
    }
    Ok(files)
}

/// Turn an unpacked extension into the themes to save. It touches no disk.
fn convert(files: &HashMap<String, String>, found: &Found) -> Result<Vec<Theme>> {
    let manifest = files
        .get("package.json")
        .context("the extension has no package.json")?;
    let manifest: Manifest = serde_json::from_str(&vscode::strip(manifest))
        .context("the package.json of the extension cannot be read")?;
    if manifest.contributes.themes.is_empty() {
        bail!("this extension contributes no colour theme");
    }
    // Labels in words live here when the extension is translated.
    let nls: Map<String, Value> = files
        .get("package.nls.json")
        .and_then(|text| serde_json::from_str(&vscode::strip(text)).ok())
        .unwrap_or_default();

    let out: Vec<Theme> = manifest
        .contributes
        .themes
        .iter()
        .filter_map(|c| one_theme(c, files, &nls, found))
        .collect();
    ensure!(
        !out.is_empty(),
        "no theme in {} could be read",
        found.display
    );
    Ok(out)
}

/// One contributed theme. `None` when the file is missing, broken or empty.
fn one_theme(
    c: &Contributed,
    files: &HashMap<String, String>,
    nls: &Map<String, Value>,
    found: &Found,
) -> Option<Theme> {
    let file = vscode::resolve(&tidy(&c.path), files).ok()?;
    if !vscode::says_anything(&file) {
        return None;
    }
    let name = c
        .label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .and_then(|label| said(label, nls))
        .or_else(|| file.name.clone())
        .unwrap_or_else(|| found.display.clone());
    let appearance = vscode::appearance_of(&file, c.ui.as_deref());
    Some(Theme {
        slug: String::new(),
        name,
        appearance,
        note: format!("From {} on open-vsx.", found.display),
        colors: vscode::to_colors(&file, appearance),
        icon_tint_alpha: None,
    })
}

/// A label in words. A translated extension writes `%key%` and keeps the words
/// in `package.nls.json`. A key with nothing behind it gives `None`.
fn said(label: &str, nls: &Map<String, Value>) -> Option<String> {
    let Some(key) = label
        .strip_prefix('%')
        .and_then(|rest| rest.strip_suffix('%'))
    else {
        return Some(label.to_string());
    };
    let words = nls.get(key)?.as_str()?.trim();
    (!words.is_empty()).then(|| words.to_string())
}

/// A path inside the extension, in the form that `vscode::resolve` uses.
fn tidy(path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = path.strip_prefix("./").unwrap_or(&path);
    path.strip_prefix("extension/").unwrap_or(path).to_string()
}

/// Percent-encode a query for a URL. The unreserved set of RFC 3986 stays.
fn escape(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

/// One bounded GET. A body over the ceiling is an error, not a cut answer.
async fn fetch(url: &str, ceiling: u64, timeout: Duration) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("garmin-companion/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut response = client
        .get(url)
        .timeout(timeout)
        .send()
        .await
        .with_context(|| format!("could not reach {url}"))?
        .error_for_status()
        .with_context(|| format!("{url} answered with an error"))?;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("could not read the answer")?
    {
        ensure!(
            body.len() as u64 + chunk.len() as u64 <= ceiling,
            "{url} sent more than {ceiling} bytes"
        );
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn found() -> Found {
        Found {
            namespace: "dracula-theme".into(),
            name: "theme-dracula".into(),
            display: "Dracula Official".into(),
            description: String::new(),
            downloads: 0,
            vsix: String::new(),
        }
    }

    fn extension(list: &[(&str, &str)]) -> HashMap<String, String> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn an_extension_gives_one_theme_for_each_entry_with_its_type() {
        let files = extension(&[
            (
                "package.json",
                r##"{"contributes":{"themes":[
                    {"label":"Night","uiTheme":"vs-dark","path":"./n.json"},
                    {"label":"Day","uiTheme":"vs","path":"./d.json"}]}}"##,
            ),
            ("n.json", r##"{"colors":{"editor.background":"#282a36"}}"##),
            ("d.json", r##"{"colors":{"editor.background":"#ffffff"}}"##),
        ]);
        let out = convert(&files, &found()).unwrap();
        let names: Vec<_> = out.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Night", "Day"]);
        assert_eq!(out[0].appearance, theme::Appearance::Dark);
        assert_eq!(out[1].appearance, theme::Appearance::Light);
        assert_eq!(out[0].colors.bg, "#282a36");
        assert!(out[0].note.contains("Dracula Official"));
        assert!(out.iter().all(|t| t.validate().is_ok()));
    }

    #[test]
    fn a_missing_file_does_not_take_its_neighbour_with_it() {
        let files = extension(&[
            (
                "package.json",
                r##"{"contributes":{"themes":[{"label":"Gone","path":"gone.json"},
                    {"label":"Here","path":"here.json"}]}}"##,
            ),
            (
                "here.json",
                r##"{"colors":{"editor.background":"#282a36"}}"##,
            ),
        ]);
        let out = convert(&files, &found()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "Here");
    }

    #[test]
    fn a_theme_that_is_not_a_theme_or_has_no_files_is_refused() {
        let none = extension(&[("package.json", r##"{"contributes":{"commands":[]}}"##)]);
        assert!(convert(&none, &found()).is_err());
        let gone = extension(&[(
            "package.json",
            r##"{"contributes":{"themes":[{"label":"Gone","path":"gone.json"}]}}"##,
        )]);
        assert!(convert(&gone, &found()).is_err());
        assert!(convert(&extension(&[]), &found()).is_err());
    }

    #[test]
    fn a_translated_label_reads_from_the_nls_file() {
        let files = extension(&[
            (
                "package.json",
                r##"{"contributes":{"themes":[{"label":"%themeLabel%","path":"d.json"}]}}"##,
            ),
            ("package.nls.json", r##"{"themeLabel":"Solarized Dark"}"##),
            ("d.json", r##"{"colors":{"editor.background":"#002b36"}}"##),
        ]);
        assert_eq!(convert(&files, &found()).unwrap()[0].name, "Solarized Dark");
        // A key with nothing behind it falls through to the name in the file.
        let files = extension(&[
            (
                "package.json",
                r##"{"contributes":{"themes":[{"label":"%gone%","path":"d.json"}]}}"##,
            ),
            (
                "d.json",
                r##"{"name":"Solar","colors":{"editor.background":"#002b36"}}"##,
            ),
        ]);
        assert_eq!(convert(&files, &found()).unwrap()[0].name, "Solar");
    }

    #[test]
    fn a_manifest_may_be_jsonc_and_an_include_is_laid_flat() {
        let files = extension(&[
            (
                "package.json",
                "{ // from a template\n \"contributes\":{\"themes\":[{\"label\":\"Half\",\"uiTheme\":\"vs\",\"path\":\"./light.json\"},]} }",
            ),
            ("dark.json", r##"{"colors":{"editor.foreground":"#222222"}}"##),
            (
                "light.json",
                r##"{"include":"./dark.json","colors":{"editor.background":"#ffffff"}}"##,
            ),
        ]);
        let out = convert(&files, &found()).unwrap();
        assert_eq!(out[0].colors.bg, "#ffffff");
        assert_eq!(out[0].colors.fg, "#222222");
    }

    #[test]
    fn a_label_that_is_a_path_cannot_leave_the_themes_folder() {
        let slug = theme::slugify("../../../.bashrc");
        assert_eq!(slug, "bashrc");
    }

    #[test]
    fn a_search_answer_becomes_rows_and_a_bare_name_is_still_a_row() {
        let body = r##"{"extensions":[
            {"namespace":"a","name":"b","displayName":"Dracula Official","description":"Dark",
             "downloadCount":4200000,"files":{"download":"https://open-vsx.org/x.vsix"}},
            {"namespace":"someone","name":"quiet"}]}"##;
        let answer: Answer = serde_json::from_str(body).unwrap();
        let rows: Vec<Found> = answer
            .extensions
            .into_iter()
            .map(Listing::into_found)
            .collect();
        assert_eq!(rows[0].display, "Dracula Official");
        assert_eq!(rows[0].downloads, 4_200_000);
        assert!(rows[0].vsix.ends_with(".vsix"));
        assert_eq!(rows[1].display, "quiet");
        assert!(rows[1].vsix.is_empty());
    }

    #[test]
    fn a_query_is_percent_encoded_and_a_path_is_tidied() {
        assert_eq!(escape("solarized dark"), "solarized%20dark");
        assert_eq!(escape("c++"), "c%2B%2B");
        assert_eq!(tidy("./themes/d.json"), "themes/d.json");
        assert_eq!(tidy(".\\themes\\d.json"), "themes/d.json");
        assert_eq!(tidy("extension/themes/d.json"), "themes/d.json");
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut out));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (path, data) in entries {
                zip.start_file(*path, options).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        out
    }

    #[test]
    fn unpack_keeps_json_under_extension_and_skips_the_rest() {
        let big = vec![b' '; ENTRY_CEILING as usize + 1];
        let bytes = zip_of(&[
            ("extension/package.json", b"{}"),
            ("extension/themes/d.json", b"{}"),
            ("extension/readme.md", b"x"),
            ("other/outside.json", b"{}"),
            ("extension/big.json", &big),
            ("extension/../escape.json", b"{}"),
        ]);
        let files = unpack(&bytes).unwrap();
        let mut keys: Vec<_> = files.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["package.json", "themes/d.json"]);
        assert!(unpack(b"not a zip").is_err());
    }
}
