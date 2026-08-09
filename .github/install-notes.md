## Installing

**macOS** — `.dmg`, `aarch64` for Apple Silicon or `x64` for Intel. Drag the app
to Applications, then run this once. The app is unsigned, and without it macOS
refuses to open it at all — the right-click → Open trick is not enough on recent
versions.

```sh
xattr -dr com.apple.quarantine "/Applications/Garmin Companion.app"
```

**Windows** — `-setup.exe`. SmartScreen calls it an unrecognised app:
**More info** → **Run anyway**.

**Linux** — `.AppImage` (`chmod +x` it, then run) or `.deb` / `.rpm`. Needs
`webkit2gtk-4.1` from your package manager. The AppImage is the one that updates
itself silently; a `.deb` or `.rpm` install is a system package, so replacing it
asks for your password the way any package install does.

**Android** — `.apk`. Not on Play, so your phone will ask you to allow installing
from wherever you downloaded it. The `.aab` beside it is for the Play Console and
is not installable directly.

Nothing here is signed with an OS vendor certificate, which is what both warnings
are about. Updates are signed, and those work either way — existing installs
update themselves from Settings on every platform, Android included. Android
downloads the new APK in the background and then asks before replacing itself,
which is as quiet as the system allows; the first time, it will also send you to
turn on "Install unknown apps" for the app.
