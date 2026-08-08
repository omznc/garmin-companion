// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Before `run()`, not inside it: the GDK backend and WebKit's workarounds
    // are both read once when the toolkit initialises. See `linux.rs`.
    #[cfg(target_os = "linux")]
    app_lib::linux::prepare();

    app_lib::run()
}
