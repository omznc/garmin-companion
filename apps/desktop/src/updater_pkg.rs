//! The deb and rpm installs: find the package, and hand a new file to the
//! package manager.
//!
//! The app must not write over a file that dpkg or rpm owns, because the
//! package database would then describe a file that is gone. So the update is
//! the package file itself. The caller has checked the signature already. The
//! install runs through `pkexec` only, and never without it.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

/// The package name, and the path of the binary, in our deb and rpm.
const PACKAGE: &str = "garmin-companion";
pub const BINARY: &str = "/usr/bin/garmin-companion";
/// Folders for the tools. Not `PATH`: the user's `PATH` must not choose what runs as root.
const TOOL_DIRS: [&str; 4] = ["/usr/bin", "/usr/sbin", "/bin", "/sbin"];
/// `pkexec` exits with 126 when the user closes the prompt, and with 127 when
/// it cannot ask (no polkit agent).
const NOT_ALLOWED: [i32; 2] = [126, 127];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    Deb,
    Rpm,
}

/// What an install did.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Installed,
    /// The system did not allow it. The text says why; the user installs by hand.
    Declined(String),
}

/// Which package owns the running binary? The answer comes from files on
/// disk, with no subprocess. `root` is `/` outside the tests.
pub fn package_of(root: &Path, exe: &Path) -> Option<PackageKind> {
    // A copy in another folder is not the file that the package replaces.
    if exe != Path::new(BINARY) {
        return None;
    }
    let dpkg_list = root.join(format!("var/lib/dpkg/info/{PACKAGE}.list"));
    if dpkg_list.exists() {
        return Some(PackageKind::Deb);
    }
    ["var/lib/rpm", "usr/lib/sysimage/rpm"]
        .iter()
        .any(|db| root.join(db).exists())
        .then_some(PackageKind::Rpm)
}

fn find_tool(name: &str) -> Option<PathBuf> {
    TOOL_DIRS
        .iter()
        .map(|dir| Path::new(dir).join(name))
        .find(|p| p.is_file())
}

/// The command that installs `file`: `pkexec`, the first tool that exists, and
/// the file. `find` looks a tool up, so a test can use a fake set.
pub fn command(
    kind: PackageKind,
    file: &Path,
    find: impl Fn(&str) -> Option<PathBuf>,
) -> Option<Vec<OsString>> {
    let tools: &[(&str, &[&str])] = match kind {
        PackageKind::Deb => &[("apt-get", &["install", "-y"]), ("dpkg", &["--install"])],
        PackageKind::Rpm => &[("dnf", &["install", "-y"]), ("rpm", &["--upgrade"])],
    };
    let pkexec = find("pkexec")?;
    let (tool, args) = tools
        .iter()
        .find_map(|(name, args)| Some((find(name)?, *args)))?;
    let mut argv = vec![pkexec.into_os_string(), tool.into_os_string()];
    argv.extend(args.iter().map(OsString::from));
    argv.push(file.into());
    Some(argv)
}

/// A new folder that only this user can enter. It must not exist yet.
#[cfg(unix)]
fn private_dir(tag: &str) -> Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let dir =
        std::env::temp_dir().join(format!("garmin-companion-pkg-{tag}-{}", std::process::id()));
    // A leftover of a crashed run. The folder is ours, or the delete fails.
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .context("could not make a private folder")?;
    Ok(dir)
}

#[cfg(not(unix))]
fn private_dir(_tag: &str) -> Result<PathBuf> {
    anyhow::bail!("packages are for Linux")
}

/// Write the package to a private folder, install it, and remove the folder.
pub fn install(kind: PackageKind, bytes: &[u8], version: &str) -> Result<Outcome> {
    let ext = if kind == PackageKind::Deb {
        "deb"
    } else {
        "rpm"
    };
    let dir = private_dir(version)?;
    let file = dir.join(format!("{PACKAGE}.{ext}"));
    let result = fs::write(&file, bytes)
        .context("could not save the package")
        .and_then(|()| run(kind, &file));
    let _ = fs::remove_dir_all(&dir);
    result
}

fn run(kind: PackageKind, file: &Path) -> Result<Outcome> {
    let Some(argv) = command(kind, file, find_tool) else {
        return Ok(Outcome::Declined(
            "this system has no pkexec or no package tool".into(),
        ));
    };
    let output = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(std::process::Stdio::null())
        .output()
        .context("could not run pkexec")?;
    match output.status.code() {
        Some(0) => Ok(Outcome::Installed),
        Some(code) if NOT_ALLOWED.contains(&code) => Ok(Outcome::Declined(
            "the system did not allow the install".into(),
        )),
        _ => Err(anyhow!(
            "the package tool failed: {}",
            last_line(&output.stderr)
                .or_else(|| last_line(&output.stdout))
                .unwrap_or_else(|| output.status.to_string())
        )),
    }
}

/// The last line a tool wrote: it holds the reason, where the first does not.
fn last_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .map(|l| l.chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gc-pkg-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for f in files {
            fs::create_dir_all(dir.join(f).parent().unwrap()).unwrap();
            fs::write(dir.join(f), "").unwrap();
        }
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn only(names: &'static [&'static str]) -> impl Fn(&str) -> Option<PathBuf> {
        move |n| {
            names
                .contains(&n)
                .then(|| PathBuf::from("/usr/bin").join(n))
        }
    }

    fn argv(kind: PackageKind, has: &'static [&'static str]) -> Vec<String> {
        command(kind, Path::new("/t/x.pkg"), only(has))
            .unwrap()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_deb_is_found_by_the_dpkg_list() {
        let r = root("deb", &["var/lib/dpkg/info/garmin-companion.list"]);
        assert_eq!(package_of(&r, Path::new(BINARY)), Some(PackageKind::Deb));
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn an_rpm_is_found_by_the_database_folder() {
        for db in ["var/lib/rpm/x", "usr/lib/sysimage/rpm/x"] {
            let r = root("rpm", &[db]);
            assert_eq!(package_of(&r, Path::new(BINARY)), Some(PackageKind::Rpm));
            let _ = fs::remove_dir_all(&r);
        }
    }

    #[test]
    fn no_database_or_another_path_is_not_a_package() {
        let r = root("none", &["var/lib/dpkg/info/other.list"]);
        assert_eq!(package_of(&r, Path::new(BINARY)), None);
        let full = root("other", &["var/lib/dpkg/info/garmin-companion.list"]);
        for exe in ["/usr/local/bin/garmin-companion", "/opt/x/garmin-companion"] {
            assert_eq!(package_of(&full, Path::new(exe)), None, "{exe}");
        }
        let _ = fs::remove_dir_all(&r);
        let _ = fs::remove_dir_all(&full);
    }

    #[test]
    fn the_command_starts_with_pkexec_and_ends_with_the_file() {
        assert_eq!(
            argv(PackageKind::Deb, &["pkexec", "apt-get", "dpkg"]),
            [
                "/usr/bin/pkexec",
                "/usr/bin/apt-get",
                "install",
                "-y",
                "/t/x.pkg"
            ]
        );
        assert_eq!(
            argv(PackageKind::Rpm, &["pkexec", "dnf", "rpm"]),
            [
                "/usr/bin/pkexec",
                "/usr/bin/dnf",
                "install",
                "-y",
                "/t/x.pkg"
            ]
        );
        assert_eq!(
            argv(PackageKind::Rpm, &["pkexec", "rpm"]),
            ["/usr/bin/pkexec", "/usr/bin/rpm", "--upgrade", "/t/x.pkg"]
        );
    }

    #[test]
    fn without_pkexec_or_a_tool_there_is_no_command() {
        let file = Path::new("/t/x.pkg");
        assert!(command(PackageKind::Deb, file, only(&["apt-get"])).is_none());
        assert!(command(PackageKind::Rpm, file, only(&["pkexec"])).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn the_private_folder_is_closed_to_others() {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_dir("mode").unwrap();
        assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o077, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tool_reason_is_the_last_line() {
        assert_eq!(
            last_line(b"Last metadata check.\nError: no match\n").as_deref(),
            Some("Error: no match")
        );
        assert_eq!(last_line(b" \n").as_deref(), None);
    }
}
