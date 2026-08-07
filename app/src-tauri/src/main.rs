// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Two WebKitGTK 2.52 bugs make the window come up blank on Linux, both of
/// which have to be handled before webkit spawns its subprocesses.
#[cfg(target_os = "linux")]
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
        let cache = dir.join("no.omznc.garmincoach/WebKitCache");
        if !cache.is_file() {
            let _ = std::fs::remove_dir_all(&cache);
            if let Some(parent) = cache.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&cache, b"");
        }
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    work_around_webkit_bugs();

    app_lib::run()
}
