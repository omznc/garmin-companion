//! Getting a card off this machine.
//!
//! The frontend draws the card and rasterises it; this module is only the last
//! step, and that step is genuinely different on the two platforms rather than
//! being one thing with a switch in it.
//!
//! Android has a sharesheet, so the PNG goes to a private cache directory and
//! the path is handed to the `__GARMIN_SHARE__` bridge, which wraps it in a
//! `FileProvider` URI and lets the system ask where it's going. Nothing is
//! saved to the gallery on the way — a card the user cancelled out of should
//! not leave a file in their photos.
//!
//! Desktop has no sharesheet worth the name, so "share" means the clipboard:
//! the image itself, ready to paste into whatever the conversation is already
//! happening in. It's also written to Pictures, because a clipboard is a
//! terrible place to keep something and the paste may not happen for an hour.
//!
//! The PNG crosses the IPC boundary as base64. Tauri's default argument
//! encoding is JSON, and a megabyte of `Vec<u8>` through that is a million-
//! element array of numbers — several megabytes of text to serialise and parse.
//! Base64 costs a third on top of the bytes and nothing on either side of the
//! wire, and the webview already produces it: the renderer's `toPng` hands back
//! a data URL, so this is the encoding it was in anyway.

use crate::CmdResult;
use base64::Engine;
use serde::Serialize;
use tauri::Manager;

/// Decodes the frontend's payload, which arrives as bare base64 — the
/// `data:image/png;base64,` prefix is stripped on the way out of the renderer.
fn decode(png: &str) -> CmdResult<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(png)
        .map_err(|e| format!("the card didn't survive the trip: {e}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shared {
    /// Where the PNG landed. Shown on the desktop, where it's somewhere the
    /// user can go; on Android it's a cache path they have no business seeing,
    /// and only the bridge reads it.
    pub path: String,
    /// Whether the image made it onto the clipboard. False on Android, and on
    /// a desktop session with no clipboard we can reach — a rare thing, but
    /// the button shouldn't claim a paste that isn't there.
    pub clipboard: bool,
}

/// Filenames come from the screen's own title, so they arrive with spaces,
/// slashes and the occasional non-Latin character in them. Anything that isn't
/// plainly safe becomes a dash, and a run of dashes collapses.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "card".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(desktop)]
#[tauri::command]
pub fn share_image(app: tauri::AppHandle, png: String, name: String) -> CmdResult<Shared> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    let png = decode(&png)?;

    // `picture_dir` is the XDG/Known Folder location, which is where someone
    // would look for an image this app made. It can be missing on a headless
    // or minimally-configured session, and the fall-back is the app's own data
    // directory rather than an error — a card on the clipboard with nowhere to
    // file it is still a card on the clipboard.
    let dir = app
        .path()
        .picture_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| format!("nowhere to save the card: {e}"))?
        .join("Garmin Companion");
    std::fs::create_dir_all(&dir).map_err(|e| format!("nowhere to save the card: {e}"))?;

    let path = dir.join(format!("{}.png", slug(&name)));
    std::fs::write(&path, &png).map_err(|e| format!("couldn't write the card: {e}"))?;

    // The clipboard wants raw RGBA, not a PNG, so this decodes rather than
    // hands the bytes over. Failing here is not failing the share: the file is
    // already on disk, and `clipboard: false` is what the button reads to
    // decide which of the two it's allowed to claim.
    let clipboard = match tauri::image::Image::from_bytes(&png) {
        Ok(image) => app.clipboard().write_image(&image).is_ok(),
        Err(_) => false,
    };

    Ok(Shared {
        path: path.to_string_lossy().into_owned(),
        clipboard,
    })
}

#[cfg(target_os = "android")]
#[tauri::command]
pub fn share_image(app: tauri::AppHandle, png: String, name: String) -> CmdResult<Shared> {
    let png = decode(&png)?;

    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("nowhere to put the card: {e}"))?
        .join("share");
    std::fs::create_dir_all(&dir).map_err(|e| format!("nowhere to put the card: {e}"))?;

    let path = dir.join(format!("{}.png", slug(&name)));

    // Every card before this one has either been shared or abandoned, and
    // either way it is dead weight in a cache the system may never sweep.
    // Cleared here rather than after the sharesheet returns, because the
    // sharesheet doesn't reliably tell us it did.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if e.path() != path {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }

    std::fs::write(&path, &png).map_err(|e| format!("couldn't write the card: {e}"))?;

    Ok(Shared {
        path: path.to_string_lossy().into_owned(),
        clipboard: false,
    })
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn slug_is_a_filename() {
        assert_eq!(slug("Last night"), "last-night");
        assert_eq!(slug("Morning Run · 10 Aug"), "morning-run-10-aug");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(slug("···"), "card");
    }
}
