//! The OS setting for reduced motion.

/// The trimmed output of a command, or `None` when it is missing or fails.
#[cfg(target_os = "linux")]
fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// True when the user turned animations off. The answer is read once, at the
/// first call, so a change of the setting needs a restart. `GC_REDUCE_MOTION=1`
/// or `=0` overrides the OS. Linux reads GNOME and KDE. macOS and Windows do
/// not read the setting yet, so they always animate.
pub fn reduced() -> bool {
    static REDUCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REDUCED.get_or_init(read)
}

fn read() -> bool {
    if let Ok(value) = std::env::var("GC_REDUCE_MOTION") {
        return value == "1";
    }
    #[cfg(target_os = "linux")]
    {
        let gnome = output(
            "gsettings",
            &["get", "org.gnome.desktop.interface", "enable-animations"],
        );
        let kde = output(
            "kreadconfig6",
            &[
                "--file",
                "kdeglobals",
                "--group",
                "KDE",
                "--key",
                "AnimationDurationFactor",
            ],
        );
        gnome.is_some_and(|v| v == "false")
            || kde
                .and_then(|v| v.parse::<f32>().ok())
                .is_some_and(|f| f == 0.0)
    }
    #[cfg(not(target_os = "linux"))]
    false
}
