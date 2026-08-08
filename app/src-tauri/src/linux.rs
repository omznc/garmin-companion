//! Everything about the Linux session that has to be settled before GTK and
//! WebKit start, plus the one answer the frontend needs out of it.
//!
//! All of this runs from `main()` before `run()`, because both the WebKit
//! workarounds and the GDK backend are read once when the toolkit initialises
//! and spawns its subprocesses — setting them later changes nothing.

use std::sync::OnceLock;

/// Whether a transparent pixel in the window will actually show the desktop
/// behind it.
///
/// The app draws its own rounded window corners by cutting them out of an
/// otherwise opaque surface (`#root` in `styles.css`), which only reads as a
/// corner if something composites what's behind the cut. Decided once by
/// [`prepare`]; see [`pick_backend`] for why the answer is "on Wayland".
static COMPOSITES_ALPHA: OnceLock<bool> = OnceLock::new();

/// Reads the decision [`prepare`] made. False until then, which is the safe
/// way round: a square window is only plainer than intended, where a cut
/// corner on a surface nothing composites is a hole with rubbish in it.
pub fn composites_alpha() -> bool {
    *COMPOSITES_ALPHA.get().unwrap_or(&false)
}

/// The escape hatch. `AppRun` exports `GDK_BACKEND` unconditionally, so a user
/// who sets it before launching an AppImage is simply overwritten and has no
/// way to ask for a different backend — including no way back to X11 if the
/// Wayland one turns out to be broken on their machine. This one survives,
/// because nothing else in the AppImage knows about it.
const BACKEND_OVERRIDE: &str = "GARMIN_COMPANION_GDK_BACKEND";

/// Two WebKitGTK 2.52 bugs make the window come up blank, both of which have to
/// be handled before webkit spawns its subprocesses.
fn work_around_webkit_bugs() {
    // 1. The DMA-BUF renderer fails to allocate GBM buffers on NVIDIA + Wayland
    //    and takes the web process down with it.
    unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };

    // 2. NetworkCache::mapFile() overruns a stack buffer reading its own cache
    //    blobs, aborting the network process — after which every load fails with
    //    "WebKit encountered an internal error" and the window stays blank. It
    //    hits mid-session too, as soon as anything re-reads a blob written
    //    earlier in the same run, so clearing the cache at startup is not
    //    enough: the cache has to stay off.
    //
    //    WebKitGTK exposes no switch for this (there is no WEBKIT_* env var for
    //    the disk cache), so the directory is held open by a file. WebKit finds
    //    the path occupied, gives up on the disk cache, and runs from the memory
    //    cache alone — which for a localhost dev server costs nothing.
    //    localstorage/ and storage/ are untouched, so app state survives.
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".local/share"))
        });
    if let Some(dir) = data {
        let cache = dir.join("com.omznc.garmincompanion/WebKitCache");
        if !cache.is_file() {
            let _ = std::fs::remove_dir_all(&cache);
            if let Some(parent) = cache.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&cache, b"");
        }
    }
}

/// Which GDK backend to run on, and whether that backend composites.
///
/// The only reason this is a decision at all is the AppImage. linuxdeploy's GTK
/// plugin writes an `AppRun` hook that hard-codes `GDK_BACKEND=x11` for every
/// Tauri AppImage ever built — a blanket workaround for tauri#8541, applied
/// whether or not the machine it lands on has the problem. The cost on a
/// Wayland session is real and visible: the window goes through XWayland, so
/// the compositor scales it as a bitmap on any fractional-scaled display, and
/// the hand-cut corners come out soft and wrong along with everything else.
///
/// So: honour whatever the user asked for, undo the hook when it's the hook
/// talking, and otherwise leave `GDK_BACKEND` unset and let GTK choose — which
/// is Wayland when there's a Wayland session and X11 when there isn't.
///
/// Rounding the corners is then gated on landing on Wayland rather than on
/// "is anything compositing", because those are different questions on X11:
/// an ARGB visual there needs a compositing manager running, which is not a
/// given, and asking costs an X connection opened before GTK's. A Wayland
/// compositor composites by definition.
fn pick_backend() -> bool {
    let wayland_session = std::env::var_os("WAYLAND_DISPLAY").is_some();

    if let Some(forced) = std::env::var_os(BACKEND_OVERRIDE) {
        let wayland = forced.to_string_lossy().starts_with("wayland");
        unsafe { std::env::set_var("GDK_BACKEND", &forced) };
        return wayland;
    }

    match std::env::var("GDK_BACKEND") {
        // `AppRun` sourced the hook and clobbered whatever was here. Inside an
        // AppImage this value is never the user's, so it's ours to overrule —
        // but only towards a backend we know exists, since GTK aborts rather
        // than falls back when `GDK_BACKEND` names one it can't open.
        Ok(b) if b == "x11" && std::env::var_os("APPDIR").is_some() && wayland_session => {
            unsafe { std::env::set_var("GDK_BACKEND", "wayland") };
            true
        }
        // Set, and ours to leave alone: either the user's own, or the hook's on
        // a machine with no Wayland session to move to.
        Ok(b) => b.starts_with("wayland"),
        // Unset — a .deb, an .rpm, or `cargo tauri dev`. GTK picks Wayland
        // whenever `WAYLAND_DISPLAY` is set, which is the same answer we'd
        // give, so there is nothing to write.
        Err(_) => wayland_session,
    }
}

/// Runs the lot. Call once, from `main`, before anything touches GTK.
pub fn prepare() {
    work_around_webkit_bugs();
    let _ = COMPOSITES_ALPHA.set(pick_backend());
}
