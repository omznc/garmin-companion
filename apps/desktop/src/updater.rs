//! The self-updater: the logic, with no window code.
//!
//! Release 1 reaches the old Tauri app through `latest.json`. This app reads
//! the same file, so the old and the new app share one release channel. The
//! format is the Tauri one: `version`, `notes` and `platforms{key:{url,
//! signature}}`. `scripts/release-rewrite.mjs` writes the keys. The install
//! steps follow `tauri-plugin-updater` 2.x for each platform.
//!
//! The order is fixed: read the manifest, compare the versions, download the
//! file, check the minisign signature, and only then install.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;

/// The URL of the old app (`plugins.updater.endpoints` in its `tauri.conf.json`).
pub const MANIFEST_URL: &str =
    "https://github.com/omznc/garmin-companion/releases/latest/download/latest.json";
/// The release page: where a package install goes to get the new version.
pub const RELEASE_PAGE: &str = "https://github.com/omznc/garmin-companion/releases/latest";
/// `plugins.updater.pubkey` of the old app, as the Tauri config holds it.
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQzRDgyNjA4RjUwNDlBMzcKUldRM21nVDFDQ2JZUSt1eDU2MHBOMEVLTG9zQXFkd0gzTy8xeW1tbnQ2RmFrdmtqc1pObU5JSnAK";

/// The time between two checks.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// The version of this app. The release workflow sets `GC_VERSION` at build
/// time, because the crate version is not the release version.
pub const VERSION: &str = match option_env!("GC_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub platforms: std::collections::HashMap<String, Entry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub url: String,
    pub signature: String,
}

/// How this copy of the app is installed. It decides what an update does.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// The NSIS setup file runs, and the app quits.
    Windows,
    /// The `.app` folder is the path: it is replaced.
    MacApp(PathBuf),
    /// The AppImage file is the path: it is replaced.
    AppImage(PathBuf),
    /// A deb or rpm, or a build run from a folder. The app only points to the release page.
    Manual,
}

/// A newer version, and what the install needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub version: String,
    pub notes: Option<String>,
    pub kind: Kind,
    /// `None` for `Kind::Manual`.
    pub download: Option<(String, String)>,
}

/// Find the install kind from the executable path and the environment.
pub fn detect_kind() -> Kind {
    let exe = std::env::current_exe().unwrap_or_default();
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    kind_of(std::env::consts::OS, &exe, appimage)
}

fn kind_of(os: &str, exe: &Path, appimage: Option<PathBuf>) -> Kind {
    match os {
        "windows" => Kind::Windows,
        "macos" => exe
            .ancestors()
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .map_or(Kind::Manual, |app| Kind::MacApp(app.to_path_buf())),
        "linux" => appimage
            .filter(|p| p.is_file())
            .map_or(Kind::Manual, Kind::AppImage),
        _ => Kind::Manual,
    }
}

/// The manifest keys to try, best first, as `scripts/release-rewrite.mjs`
/// writes them. The Tauri updater tries the long key, then the short one.
pub fn platform_keys(os: &str, arch: &str, kind: &Kind) -> Vec<String> {
    let os = if os == "macos" { "darwin" } else { os };
    let installer = match kind {
        Kind::Windows => "nsis",
        Kind::MacApp(_) => "app",
        Kind::AppImage(_) => "appimage",
        Kind::Manual => return Vec::new(),
    };
    vec![format!("{os}-{arch}-{installer}"), format!("{os}-{arch}")]
}

fn parse_version(text: &str) -> Result<semver::Version> {
    semver::Version::parse(text.trim().trim_start_matches('v'))
        .with_context(|| format!("not a version: {text:?}"))
}

/// Is `latest` newer than `current`?
pub fn is_newer(latest: &str, current: &str) -> Result<bool> {
    Ok(parse_version(latest)? > parse_version(current)?)
}

/// Decide from a manifest. `None` means the app is up to date.
pub fn evaluate(
    manifest: &Manifest,
    current: &str,
    os: &str,
    arch: &str,
    kind: &Kind,
) -> Result<Option<Offer>> {
    if !is_newer(&manifest.version, current)? {
        return Ok(None);
    }
    let download = if *kind == Kind::Manual {
        None
    } else {
        let entry = platform_keys(os, arch, kind)
            .iter()
            .find_map(|key| manifest.platforms.get(key))
            .ok_or_else(|| anyhow!("the release has no file for this system"))?;
        if !entry.url.starts_with("https://") {
            bail!("the download address is not https");
        }
        Some((entry.url.clone(), entry.signature.clone()))
    };
    Ok(Some(Offer {
        version: manifest.version.trim().trim_start_matches('v').to_string(),
        notes: manifest.notes.clone().filter(|n| !n.trim().is_empty()),
        kind: kind.clone(),
        download,
    }))
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(format!("garmin-companion/{VERSION}"))
        .timeout(Duration::from_secs(600))
        .build()?)
}

/// Read the manifest. `None` means the app is up to date.
pub async fn check() -> Result<Option<Offer>> {
    let manifest: Manifest = client()?
        .get(MANIFEST_URL)
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("the update file is not valid")?;
    evaluate(
        &manifest,
        VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
        &detect_kind(),
    )
}

/// Check a signature in the Tauri form: both inputs are base64 of the
/// minisign text files.
pub fn verify(data: &[u8], signature_b64: &str, pubkey_b64: &str) -> Result<()> {
    let text = |b64: &str| -> Result<String> {
        let raw = base64::engine::general_purpose::STANDARD.decode(b64.trim())?;
        Ok(String::from_utf8(raw)?)
    };
    let key = PublicKey::decode(&text(pubkey_b64)?).map_err(|e| anyhow!("bad public key: {e}"))?;
    let sig =
        Signature::decode(&text(signature_b64)?).map_err(|e| anyhow!("bad signature: {e}"))?;
    key.verify(data, &sig, true)
        .map_err(|e| anyhow!("the signature does not match: {e}"))
}

/// Download the file of an offer, check it, install it, and start the new
/// app where that is needed. On `Ok`, the caller must quit the app.
pub async fn install(offer: &Offer) -> Result<()> {
    let (url, signature) = offer
        .download
        .as_ref()
        .context("this install is updated by hand")?;
    let bytes = client()?
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    verify(&bytes, signature, PUBKEY)?;
    install_bytes(&bytes, &offer.kind, &offer.version)
}

fn install_bytes(bytes: &[u8], kind: &Kind, version: &str) -> Result<()> {
    match kind {
        Kind::Windows => run_nsis(bytes, version),
        Kind::MacApp(app) => {
            replace_bundle(bytes, app)?;
            relaunch(&["open", "-n"], app)
        }
        Kind::AppImage(path) => {
            replace_appimage(bytes, path)?;
            relaunch(&[], path)
        }
        Kind::Manual => bail!("this install is updated by hand"),
    }
}

/// The arguments of the NSIS setup, as the Tauri updater passes them in the
/// `passive` mode (the config of the app): `/P` shows progress with no
/// questions, `/R` restarts, `/UPDATE` tells the setup that this is an update.
fn nsis_args() -> [&'static str; 3] {
    ["/P", "/R", "/UPDATE"]
}

fn run_nsis(bytes: &[u8], version: &str) -> Result<()> {
    if !bytes.starts_with(b"MZ") {
        bail!("the download is not a Windows program");
    }
    let dir = fresh_dir(version)?;
    let setup = dir.join("garmin-companion-setup.exe");
    fs::write(&setup, bytes)?;
    Command::new(&setup).args(nsis_args()).spawn()?;
    Ok(())
}

/// A new, empty folder in the temp folder.
fn fresh_dir(tag: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "garmin-companion-update-{tag}-{}",
        std::process::id()
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Start `target` again after one second, from a shell that outlives this
/// process. The wait lets this process quit first, so a single-instance
/// check does not stop the new one.
fn relaunch(prefix: &[&str], target: &Path) -> Result<()> {
    Command::new("sh")
        .arg("-c")
        .arg("sleep 1; exec \"$@\"")
        .arg("sh")
        .args(prefix)
        .arg(target)
        .spawn()
        .context("could not start the new version")?;
    Ok(())
}

/// Linux: replace the AppImage file. The new file is written next to the old
/// one, so the rename is atomic (same file system).
pub fn replace_appimage(bytes: &[u8], target: &Path) -> Result<()> {
    let image = if bytes.starts_with(&[0x1f, 0x8b]) {
        first_with_extension(bytes, "AppImage")?
    } else {
        bytes.to_vec()
    };
    if !image.starts_with(b"\x7fELF") {
        bail!("the download is not an AppImage");
    }
    let name = target
        .file_name()
        .context("no file name")?
        .to_string_lossy();
    let staged = target.with_file_name(format!(".{name}.update"));
    let result = write_executable(&staged, &image, target)
        .and_then(|()| fs::rename(&staged, target).context("could not replace the AppImage"));
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

#[cfg(unix)]
fn write_executable(path: &Path, data: &[u8], like: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, data)?;
    let mode = fs::metadata(like)?.permissions().mode() | 0o100;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_executable(path: &Path, data: &[u8], _like: &Path) -> Result<()> {
    fs::write(path, data)?;
    Ok(())
}

/// The first file in a `.tar.gz` that has this extension.
fn first_with_extension(gz: &[u8], ext: &str) -> Result<Vec<u8>> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(gz));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.extension().is_some_and(|e| e == ext) {
            let mut out = Vec::new();
            entry.read_to_end(&mut out)?;
            return Ok(out);
        }
    }
    bail!("the download holds no .{ext} file")
}

/// macOS: replace the `.app` folder with the one in a `.app.tar.gz`. The
/// archive unpacks next to the old folder, so the swap is a rename. A folder
/// that this user cannot write (for example `/Applications` of an admin)
/// fails with a clear error. The old folder returns if the swap fails.
pub fn replace_bundle(gz: &[u8], target: &Path) -> Result<()> {
    let parent = target.parent().context("the app has no folder")?;
    let stage = parent.join(".garmin-companion-update");
    let backup = parent.join(".garmin-companion-old");
    for dir in [&stage, &backup] {
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
    }
    fs::create_dir(&stage).context("this folder is not writable: install the update by hand")?;
    let swapped = unpack_and_swap(gz, &stage, target, &backup);
    let _ = fs::remove_dir_all(&stage);
    if swapped.is_ok() {
        let _ = fs::remove_dir_all(&backup);
    }
    swapped
}

fn unpack_and_swap(gz: &[u8], stage: &Path, target: &Path, backup: &Path) -> Result<()> {
    tar::Archive::new(flate2::read::GzDecoder::new(gz)).unpack(stage)?;
    let new_app = fs::read_dir(stage)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .context("the download holds no .app folder")?;
    fs::rename(target, backup)?;
    if let Err(e) = fs::rename(&new_app, target) {
        let _ = fs::rename(backup, target);
        return Err(e).context("could not move the new app into place");
    }
    Ok(())
}

/// Is a new check due? `last` is the time of the last good check.
pub fn due(last: Option<SystemTime>, now: SystemTime) -> bool {
    last.is_none_or(|t| now.duration_since(t).map_or(true, |age| age >= CHECK_EVERY))
}

/// The file whose modification time is the time of the last good check.
fn stamp_path() -> Option<PathBuf> {
    garmin_core::paths::base_dir()
        .ok()
        .map(|d| d.join("update-check"))
}

pub fn last_check() -> Option<SystemTime> {
    fs::metadata(stamp_path()?).ok()?.modified().ok()
}

pub fn record_check() {
    if let Some(path) = stamp_path() {
        let _ = fs::write(path, b"");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A throwaway key pair, made for these tests. The private key is gone.
    const TEST_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEIyQTdGRUYwNUE3QTk3RTEKUldUaGwzcGE4UDZuc3RWeDhxWVpDVC9heldsaWVobm9hemFtRit0bUFhWE51aUViMzVSMVQ1VnkK";
    const TEST_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IGZpeHR1cmUKUlVUaGwzcGE4UDZuc3UweXRYZmFXUXBQR09hN2FIdlYzNDNHcmg4RWFSQkYvTXlSR3hOd1Q0RUx3SHIwOTQwbmd3NEJuVW9PWVdlamdoc0VVNFROYnZmSXFaSVYxTXpKbVFnPQp0cnVzdGVkIGNvbW1lbnQ6IGZpeHR1cmUKeVFuVXpCRThhVXUwYWN1M3d6T2pTNzFML1NrSmMxa3QvWGEvYmttRlN6NmtVZThNdHlZQmp6QlQ5OUFmRWE0WHdvTFhtZXBTSVVOc1REcUVtczEvQ0E9PQo=";
    const TEST_DATA: &[u8] = b"garmin companion update fixture";

    const MANIFEST: &str = r#"{
      "version": "1.2.0",
      "notes": "Fixes.",
      "pub_date": "2026-10-19T00:00:00Z",
      "platforms": {
        "windows-x86_64": {"url": "https://x/w.exe", "signature": "s1"},
        "windows-x86_64-nsis": {"url": "https://x/w.exe", "signature": "s1"},
        "darwin-aarch64": {"url": "https://x/m.tar.gz", "signature": "s2"},
        "linux-x86_64": {"url": "https://x/l.AppImage", "signature": "s3"},
        "linux-x86_64-deb": {"url": "https://x/l.deb", "signature": "s4"}
      }
    }"#;

    fn manifest() -> Manifest {
        serde_json::from_str(MANIFEST).unwrap()
    }

    #[test]
    fn the_manifest_parses() {
        let m = manifest();
        assert_eq!(m.version, "1.2.0");
        assert_eq!(m.notes.as_deref(), Some("Fixes."));
        assert_eq!(m.platforms.len(), 5);
        let bare: Manifest = serde_json::from_str(r#"{"version":"1.0.0"}"#).unwrap();
        assert!(bare.platforms.is_empty() && bare.notes.is_none());
    }

    #[test]
    fn keys_match_the_release_script() {
        let app = Kind::MacApp("/Applications/X.app".into());
        let image = Kind::AppImage("/x.AppImage".into());
        assert_eq!(
            platform_keys("windows", "x86_64", &Kind::Windows),
            ["windows-x86_64-nsis", "windows-x86_64"]
        );
        assert_eq!(
            platform_keys("macos", "aarch64", &app),
            ["darwin-aarch64-app", "darwin-aarch64"]
        );
        assert_eq!(
            platform_keys("linux", "x86_64", &image),
            ["linux-x86_64-appimage", "linux-x86_64"]
        );
        assert!(platform_keys("linux", "x86_64", &Kind::Manual).is_empty());
    }

    #[test]
    fn the_kind_follows_the_install() {
        let exe = Path::new("/Applications/Garmin Companion.app/Contents/MacOS/garmin-companion");
        assert_eq!(
            kind_of("macos", exe, None),
            Kind::MacApp("/Applications/Garmin Companion.app".into())
        );
        assert_eq!(
            kind_of("macos", Path::new("/tmp/garmin-companion"), None),
            Kind::Manual
        );
        assert_eq!(kind_of("windows", exe, None), Kind::Windows);
        // A deb or rpm has no APPIMAGE variable. A stale variable is no file.
        assert_eq!(
            kind_of("linux", Path::new("/usr/bin/x"), None),
            Kind::Manual
        );
        let gone = Some(PathBuf::from("/nonexistent/x.AppImage"));
        assert_eq!(
            kind_of("linux", Path::new("/usr/bin/x"), gone),
            Kind::Manual
        );
    }

    #[test]
    fn versions_compare_as_semver() {
        assert!(is_newer("1.0.0", "0.9.1").unwrap());
        assert!(is_newer("v1.10.0", "1.9.0").unwrap());
        assert!(!is_newer("1.0.0", "1.0.0").unwrap());
        assert!(!is_newer("0.9.1", "1.0.0").unwrap());
        assert!(!is_newer("1.0.0-beta.1", "1.0.0").unwrap());
        assert!(is_newer("nonsense", "1.0.0").is_err());
    }

    #[test]
    fn evaluate_picks_the_file_for_the_install() {
        let m = manifest();
        let offer = evaluate(&m, "1.0.0", "windows", "x86_64", &Kind::Windows)
            .unwrap()
            .unwrap();
        assert_eq!(offer.version, "1.2.0");
        assert_eq!(offer.download.unwrap().1, "s1");
        // No "-app" key: the short key serves.
        let mac = Kind::MacApp("/A.app".into());
        let offer = evaluate(&m, "1.0.0", "macos", "aarch64", &mac)
            .unwrap()
            .unwrap();
        assert_eq!(offer.download.unwrap().1, "s2");
        // A system with no entry is an error, not "up to date".
        assert!(evaluate(&m, "1.0.0", "macos", "x86_64", &mac).is_err());
        assert!(evaluate(&m, "1.2.0", "macos", "x86_64", &mac)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_package_install_gets_no_download() {
        let offer = evaluate(&manifest(), "1.0.0", "linux", "x86_64", &Kind::Manual)
            .unwrap()
            .unwrap();
        assert!(offer.download.is_none());
        assert_eq!(offer.kind, Kind::Manual);
    }

    #[test]
    fn an_address_that_is_not_https_is_refused() {
        let mut m = manifest();
        m.platforms.get_mut("windows-x86_64-nsis").unwrap().url = "http://x/w.exe".into();
        assert!(evaluate(&m, "1.0.0", "windows", "x86_64", &Kind::Windows).is_err());
    }

    #[test]
    fn a_good_signature_passes() {
        verify(TEST_DATA, TEST_SIG, TEST_PUBKEY).unwrap();
    }

    #[test]
    fn a_bad_signature_or_key_fails() {
        assert!(verify(b"other data", TEST_SIG, TEST_PUBKEY).is_err());
        assert!(verify(TEST_DATA, "not base64 !", TEST_PUBKEY).is_err());
        assert!(verify(TEST_DATA, TEST_SIG, "bm90IGEga2V5").is_err());
        // The real app key does not accept a signature of the test key.
        assert!(verify(TEST_DATA, TEST_SIG, PUBKEY).is_err());
    }

    #[test]
    fn the_real_public_key_decodes() {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(PUBKEY)
            .unwrap();
        PublicKey::decode(std::str::from_utf8(&raw).unwrap()).unwrap();
    }

    #[test]
    fn a_check_is_due_after_six_hours() {
        let now = SystemTime::now();
        assert!(due(None, now));
        assert!(!due(Some(now - Duration::from_secs(60)), now));
        assert!(due(Some(now - CHECK_EVERY), now));
        // A stamp from the future (clock moved back) does not block for ever.
        assert!(due(Some(now + Duration::from_secs(60)), now));
    }

    #[test]
    fn nsis_gets_the_passive_flags() {
        assert_eq!(nsis_args(), ["/P", "/R", "/UPDATE"]);
    }

    #[cfg(unix)]
    mod unix {
        use super::super::*;

        fn gz(files: &[(&str, &[u8])]) -> Vec<u8> {
            let mut builder = tar::Builder::new(Vec::new());
            for (name, data) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder.append_data(&mut header, name, *data).unwrap();
            }
            let tar = builder.into_inner().unwrap();
            let mut out = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            std::io::Write::write_all(&mut out, &tar).unwrap();
            out.finish().unwrap()
        }

        fn scratch(name: &str) -> PathBuf {
            let dir =
                std::env::temp_dir().join(format!("gc-updater-test-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            dir
        }

        #[test]
        fn the_appimage_is_replaced_and_stays_executable() {
            use std::os::unix::fs::PermissionsExt;
            let dir = scratch("image");
            let target = dir.join("app.AppImage");
            fs::write(&target, b"\x7fELF old").unwrap();
            fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
            replace_appimage(b"\x7fELF new", &target).unwrap();
            assert_eq!(fs::read(&target).unwrap(), b"\x7fELF new");
            assert_ne!(
                fs::metadata(&target).unwrap().permissions().mode() & 0o111,
                0
            );
            assert_eq!(
                fs::read_dir(&dir).unwrap().count(),
                1,
                "no staged file stays"
            );
            // A gzip with the image inside works too.
            replace_appimage(&gz(&[("x/app.AppImage", b"\x7fELF zipped")]), &target).unwrap();
            assert_eq!(fs::read(&target).unwrap(), b"\x7fELF zipped");
            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_download_that_is_no_image_leaves_the_old_file() {
            let dir = scratch("junk");
            let target = dir.join("app.AppImage");
            fs::write(&target, b"\x7fELF old").unwrap();
            assert!(replace_appimage(b"<html>error</html>", &target).is_err());
            assert_eq!(fs::read(&target).unwrap(), b"\x7fELF old");
            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn the_app_folder_is_replaced() {
            let dir = scratch("bundle");
            let target = dir.join("Garmin Companion.app");
            fs::create_dir_all(target.join("Contents")).unwrap();
            fs::write(target.join("Contents/version"), "old").unwrap();
            let archive = gz(&[("Garmin Companion.app/Contents/version", b"new")]);
            replace_bundle(&archive, &target).unwrap();
            assert_eq!(
                fs::read_to_string(target.join("Contents/version")).unwrap(),
                "new"
            );
            assert_eq!(
                fs::read_dir(&dir).unwrap().count(),
                1,
                "no staged folder stays"
            );
            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn an_archive_with_no_app_keeps_the_old_folder() {
            let dir = scratch("noapp");
            let target = dir.join("X.app");
            fs::create_dir_all(&target).unwrap();
            fs::write(target.join("keep"), "old").unwrap();
            assert!(replace_bundle(&gz(&[("readme.txt", b"x")]), &target).is_err());
            assert_eq!(fs::read_to_string(target.join("keep")).unwrap(), "old");
            let _ = fs::remove_dir_all(&dir);
        }
    }
}
