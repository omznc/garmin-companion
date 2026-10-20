//! Set `GC_APP_VERSION` for the compiler. The release workflow sets
//! `APP_VERSION`. Without it, the value is the `version` of `tauri.conf.json`,
//! the same file that `scripts/check-metainfo.mjs` reads. The crate version
//! is not the release version.

fn conf_version() -> String {
    let text = std::fs::read_to_string("tauri.conf.json").expect("read tauri.conf.json");
    // The first "version" key is the top-level one.
    let rest = &text[text.find("\"version\"").expect("version key") + 9..];
    let start = rest.find('"').expect("version value") + 1;
    let len = rest[start..].find('"').expect("end of version");
    rest[start..start + len].to_string()
}

fn main() {
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-env-changed=APP_VERSION");
    let version = std::env::var("APP_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(conf_version);
    println!("cargo:rustc-env=GC_APP_VERSION={version}");
}
