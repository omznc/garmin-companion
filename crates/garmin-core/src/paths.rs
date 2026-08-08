//! Where this crate is allowed to put things.
//!
//! On a desktop the answer is `dirs::data_dir()`, and every caller could have
//! asked for it directly — which is what they used to do. Android has no such
//! thing: there are no XDG variables to read, and an app's private directory is
//! handed to it by the system at runtime rather than derived from a convention.
//! `dirs::data_dir()` there returns `None`, so the cache and the themes folder
//! both had nowhere to go.
//!
//! So the base directory became something that can be *told* rather than only
//! worked out. The Tauri app calls [`set_base_dir`] with the path Android gave
//! it before anything opens a database; the MCP server and the desktop build
//! never call it and get the derived answer. That keeps this crate free of a
//! Tauri dependency, which matters — `garmin-mcp` links it too, and that binary
//! has no app handle to ask.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Set once, at startup, by a host that knows better than the conventions do.
static BASE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The folder name under the platform's data directory. Unused when the base
/// directory has been set explicitly — Android's app-private path is already
/// unique to this app, so nesting a second name inside it would only add a
/// level for nobody's benefit.
const APP_DIR: &str = "garmin-coach";

/// Point this crate at a directory of the host's choosing.
///
/// Only the first call counts. A second one is not an error and not a panic:
/// the value is identical in every real path that reaches here twice (Tauri's
/// `setup` running again under a mobile relaunch), and taking the process down
/// over a redundant call would be a worse outcome than ignoring it.
pub fn set_base_dir(dir: impl Into<PathBuf>) {
    let _ = BASE_DIR.set(dir.into());
}

/// Where the cache, the themes and anything else this crate owns live.
///
/// Errors rather than falling back to the working directory: a cache written
/// next to wherever the binary happened to be launched from is a cache that
/// silently isn't the one from last time.
pub fn base_dir() -> Result<PathBuf> {
    if let Some(dir) = BASE_DIR.get() {
        return Ok(dir.clone());
    }
    Ok(dirs::data_dir()
        .context("could not locate a data directory")?
        .join(APP_DIR))
}

/// [`base_dir`], created if it isn't there yet.
pub fn ensure_base_dir() -> Result<PathBuf> {
    let dir = base_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;
    Ok(dir)
}
