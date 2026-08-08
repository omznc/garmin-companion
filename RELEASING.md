# Releasing

## Before the first release — four things only you can do

**1. Pick the GitHub repo and make the URL match.** The updater endpoint is
hardcoded and I had to guess at it. It is currently

```
https://github.com/omznc/garmin-companion/releases/latest/download/latest.json
```

in `app/src-tauri/tauri.conf.json` under `plugins.updater.endpoints`. If the
repo ends up at a different owner or name, change that one string. Nothing else
refers to the slug. Getting it wrong doesn't break the build — it breaks update
checks silently, months later.

**2. Add the signing key to GitHub.** The updater refuses any bundle it can't
verify against the public key baked into the app, so CI has to sign with the
matching private key. One was generated for you at:

- private: `~/.tauri/garmin-companion.key`
- public: `~/.tauri/garmin-companion.key.pub` (already pasted into
  `tauri.conf.json` as `plugins.updater.pubkey`)

Add the private key as a repository secret:

```sh
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/garmin-companion.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body ""
```

The password is empty because the key was generated without one. Back the
private key up somewhere you won't lose it: **if it's gone, every installed
copy of the app stops being able to update**, and the only fix is shipping a
new key in a build people have to install by hand.

**3. Generate the Android signing key.** Separate from the updater key above and
not interchangeable — this one is Android's own, and it decides identity rather
than authenticity. Android will only install a new APK over an old one when both
carry the same signature, so **if this key is lost, every existing install has
to be uninstalled by hand before it can take another update**. There is no
recovery and no override.

```sh
keytool -genkey -v -keystore ~/.tauri/garmin-companion.jks \
  -keyalg RSA -keysize 2048 -validity 10000 -alias garmin-companion
```

Then hand CI the keystore and the three strings that open it:

```sh
base64 -w0 ~/.tauri/garmin-companion.jks | gh secret set ANDROID_KEYSTORE
gh secret set ANDROID_KEYSTORE_PASSWORD --body '<store password>'
gh secret set ANDROID_KEY_PASSWORD      --body '<key password>'
gh secret set ANDROID_KEY_ALIAS         --body 'garmin-companion'
```

Skipping this is survivable: the `android` job still runs and still proves the
target compiles, it just produces an unsigned APK and doesn't attach it to the
release. Back the `.jks` up somewhere you won't lose it.

**4. Push the repo.** It currently has no commits and no remote.

```sh
git add -A && git commit -m "Initial commit"
gh repo create omznc/garmin-companion --private --source=. --push
```

**5. Decide about code signing** (or decide not to care). See below.

## Cutting a release

The version in `app/src-tauri/tauri.conf.json` is the source of truth — the
updater compares against it, not against the tag. Bump it, then tag to match:

```sh
# edit "version" in app/src-tauri/tauri.conf.json, then:
git commit -am "Release v0.2.0"
git tag v0.2.0
git push origin master --tags
```

One file has to move with it: `app/src-tauri/linux/no.omznc.garmincoach.metainfo.xml`
needs a new `<release>` entry with the same version and its date. That is what
GNOME Software and KDE Discover show as "what's new", and what they compare an
installed copy against — a stale list there doesn't break the build, it just
means the Linux packages advertise the wrong version to every software centre.

`.github/workflows/release.yml` fires on the tag and builds five bundles in
parallel: macOS arm64, macOS x64, Linux x64, Windows x64, and Android. It opens
the release as a **draft** so you can look at it before anyone downloads —
publish it in the GitHub UI when all five jobs are green. Publishing is what
makes the updater see it, because the endpoint points at `releases/latest`.

Artifacts:

| Platform | Installer | Updater artifact |
|---|---|---|
| macOS | `.dmg` | `.app.tar.gz` + `.sig` |
| Windows | `-setup.exe` (NSIS) | `.nsis.zip` + `.sig` |
| Linux | `.AppImage`, `.deb`, `.rpm` | `.AppImage.tar.gz` + `.sig` |
| Android | `.apk` (plus `.aab` for Play) | — none, see below |

`latest.json` is generated alongside them and is the file the app fetches.

### Why Android is its own job

`tauri-action` builds the desktop bundles and writes `latest.json`. An Android
build produces neither, so it sits in a separate job that uploads through
`gh release upload` instead. Putting it in the matrix would have the action
either fail on a platform it doesn't bundle for, or publish a `latest.json` that
an APK can't be an update for.

Android is not self-updating, and that isn't an omission. An installed Android
app may not replace its own package, so `tauri-plugin-updater` is `cfg`'d out on
that target (`app/src-tauri/Cargo.toml`) and Settings hides the update section.
A new version means downloading the new APK over the old one — which works
silently, provided the signing key hasn't changed.

### The Android version code

Google Play orders builds by `versionCode`, an integer, not by the version
string. Tauri derives it as `major * 1000000 + minor * 1000 + patch`, so
bumping the version in `tauri.conf.json` carries it along and there is nothing
extra to remember. It only matters if you ever put this on Play; a sideloaded
APK installs over an older one regardless.

### Why Windows is NSIS and not MSI

`bundle.targets` is `"all"` everywhere except Windows, which
`tauri.windows.conf.json` narrows to `["nsis"]`. JSON can't hold the reason, so
it lives here: the MSI bundler drives WiX v3, whose `light.exe` runs its
validation through **VBSCRIPT**, and Microsoft is removing VBScript from
Windows. `windows-latest` is now Windows Server 2025, where it's no longer
reliably present, and the build dies with `failed to run light.exe` after the
Rust compile has already succeeded — which is how v0.1.0's Windows job failed.

Pinning `windows-2022` would also have fixed it, and was rejected: it keeps a
deprecated runner alive to feed a deprecated Windows feature, and breaks again
when either goes. NSIS needs neither. If you ever want the `.msi` back, that
tradeoff is the thing to re-read.

### Why `icons/128x128@2x.png` isn't in `bundle.icon`

Same problem, same place. The Linux bundler names each hicolor directory after
the icon's filename rather than its pixel size, so `128x128@2x.png` — a 256px
image — installs to `hicolor/256x256@2/`, which claims a 512px icon. A desktop
on a HiDPI display believes the directory and picks it, then draws a 256px file
at 512px. The plain sizes cover the same ground honestly: `32x32`, `64x64`,
`128x128` and `icon.png` at 512. macOS and Windows read `icon.icns` and
`icon.ico` and never touch the list, so dropping it costs nothing there.

### Why the Android launcher icon is partly hand-written

`pnpm tauri icon src-tauri/icons/icon.svg` regenerates every desktop icon and
the Android raster mipmaps, and that covers Android 7 only. From Android 8 the
launcher wants an *adaptive* icon — a background and a foreground it masks to
whatever shape the device uses and parallaxes on touch — and Tauri writes no
such thing, so the phone falls back to treating our full-bleed plate as a
legacy icon: shrunk, and pasted onto a white circle the launcher draws itself.

The three vectors under `gen/android/.../res/drawable/` are that adaptive icon,
plus a `monochrome` layer for Android 13's themed icons, and
`mipmap-anydpi-v26/ic_launcher.xml` binds them. They are written by hand, no
tool regenerates them, and if the mark in `icon.svg` ever changes they need the
same edit — the arc paths are copied across verbatim. See the comments in
`ic_launcher_foreground.xml` for the sizing, which is the part that isn't
obvious.

## Building locally

```sh
cd app && pnpm tauri build
```

Bundles land in `target/release/bundle/`. Unsigned unless
`TAURI_SIGNING_PRIVATE_KEY` is in the environment, which is fine for testing —
just don't hand a locally built bundle to someone expecting updates to work.

### Android

```sh
source scripts/android-env.sh
cd app && pnpm tauri android build --apk --debug
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

`scripts/android-env.sh` exists because three things about an Android build fail
in ways that don't say what's wrong, and it settles all three: the SDK and NDK
paths, a JDK that Gradle accepts (it refuses anything past 21, and Fedora ships
25 on `PATH`), and the Perl modules OpenSSL's `./Configure` needs. On Fedora
that last one is:

```sh
sudo dnf install perl-FindBin perl-IPC-Cmd perl-version perl-Time-Piece
```

`scripts/android-perl-shim.sh` does the same thing without root, for a machine
where you can't install packages. Neither is needed in CI — the Ubuntu runner
has a complete Perl already.

**Run `pnpm tauri android build` through the package manager, not through
`./node_modules/.bin/tauri`.** Tauri records the command it was invoked with
into the generated Gradle project so the Rust step can call back into it, and
invoking the bin shim directly makes it record `node tauri`, which resolves to
nothing. The symptom is a `MODULE_NOT_FOUND` from `:app:rustBuildArm64Debug`
three minutes into the build.

Note that `pnpm tauri android init` **regenerates** `app/src-tauri/gen/android`,
which is committed and has been hand-edited in two places: the backup lockdown
in `AndroidManifest.xml` (see `garmin-core::secrets` for why it matters) and the
release signing config in `app/build.gradle.kts`. Both are commented as such.
Check `git diff` after re-running it.

## Code signing — what "unsigned" actually costs

Nothing here is signed with an OS vendor certificate, only with the updater's
own minisign key. That key proves an update came from you; it does nothing for
Gatekeeper or SmartScreen.

- **macOS**: Gatekeeper refuses to open the app on first launch. Users have to
  right-click → Open, or run `xattr -dr com.apple.quarantine`. Fixing this
  properly needs an Apple Developer account (~$99/yr) and notarization —
  `tauri-action` supports it through `APPLE_CERTIFICATE`,
  `APPLE_SIGNING_IDENTITY`, `APPLE_ID` and `APPLE_PASSWORD`. **Updates on
  macOS work either way**; it's only the first install that gets stopped.
- **Windows**: SmartScreen shows "unrecognized app" until the download builds
  reputation. A code-signing certificate is several hundred a year.
- **Linux**: nobody signs AppImages. Nothing to do.
- **Android**: the APK *is* signed, with your own keystore — that is what
  Android requires, and there is no vendor certificate to buy. Installing it
  still needs "allow from this source" the first time, because it didn't come
  from Play. Nothing fixes that except shipping on Play.

For a personal app this is a shrug. If you ever hand it to someone else, macOS
notarization is the one worth buying.

## What isn't automated

- **The changelog.** The release body is a fixed blurb in the workflow. If you
  want per-release notes, write them into the draft before publishing.
- **Version bumping.** Deliberately manual — one number, and it has to agree
  with the tag.
- **iOS.** The Rust side is already platform-split for it — `secrets`, `paths`
  and the mobile sign-in flow are written against `cfg(mobile)` rather than
  against Android — and the frontend branches on `IS_MOBILE`, not on `android`.
  What's missing is an Apple Developer account, `tauri ios init`, and iOS icons.
  Nobody has tried it.
