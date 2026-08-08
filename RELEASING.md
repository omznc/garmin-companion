# Releasing

## Setup — done, and what losing it costs

None of this needs doing again. It's written down because each piece is
unrecoverable in a way that only surfaces months later.

**The updater endpoints**, of which there are two — one per manifest format,
because desktop and Android don't ship the same kind of artifact:

```
https://github.com/omznc/garmin-companion/releases/latest/download/latest.json
https://github.com/omznc/garmin-companion/releases/latest/download/latest-android.json
```

The first is in `app/src-tauri/tauri.conf.json` under
`plugins.updater.endpoints`, read by `tauri-plugin-updater`. The second is the
`MANIFEST` constant in `app/src/lib/apk.ts`, read by the app itself — the
plugin isn't compiled in on Android, so there is nothing to configure and the
URL lives next to the code that fetches it.

Those two lines are the only places the repo slug appears. If it ever moves,
both change or half the installed copies stop hearing about releases. Getting
either wrong doesn't break the build — it breaks update checks silently.

**The updater key**, which proves a bundle came from you. The app refuses any
update it can't verify against the public half baked into it.

- private: `~/.tauri/garmin-companion.key`, held by CI as
  `TAURI_SIGNING_PRIVATE_KEY` (its password is empty — the key was generated
  without one)
- public: `~/.tauri/garmin-companion.key.pub`, pasted into `tauri.conf.json` as
  `plugins.updater.pubkey`

**If the private key is gone, every installed copy stops being able to update**,
and the only fix is shipping a new key in a build people install by hand.

**The Android key**, which is not interchangeable with the one above: it decides
identity rather than authenticity. Android installs a new APK over an old one
only when both carry the same signature, so **if it's lost, every existing
install has to be uninstalled by hand before it can take another update**. No
recovery, no override.

- keystore: `~/.tauri/garmin-companion.jks` — RSA 2048, alias
  `garmin-companion`, valid 10,000 days
- password: `~/.tauri/garmin-companion.jks.password`, mode 600, one string for
  both the store and the key

Back both up somewhere that isn't this machine. If the secrets ever need
setting again:

```sh
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/garmin-companion.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body ""
base64 -w0 ~/.tauri/garmin-companion.jks | gh secret set ANDROID_KEYSTORE
gh secret set ANDROID_KEYSTORE_PASSWORD < ~/.tauri/garmin-companion.jks.password
gh secret set ANDROID_KEY_PASSWORD      < ~/.tauri/garmin-companion.jks.password
gh secret set ANDROID_KEY_ALIAS         --body 'garmin-companion'
```

The password file ends without a newline on purpose. `gh secret set` sends
stdin verbatim, so a trailing `\n` lands inside the secret, and Gradle then
fails to open a keystore whose password looks correct everywhere you'd check
it.

Losing the Android secrets is survivable in the short term: the `android` job
still runs and still proves the target compiles, it just produces an unsigned
APK and doesn't attach it to the release.

**Code signing with an OS vendor certificate** is the one thing still not done,
and the only one that's a decision rather than a task. See below.

## Cutting a release

The version in `app/src-tauri/tauri.conf.json` is the source of truth — the
updater compares against it, not against the tag. Bump it, then tag to match:

```sh
# edit "version" in app/src-tauri/tauri.conf.json, then:
git commit -am "Release v0.2.0"
git tag v0.2.0
git push origin master --tags
```

One file has to move with it: `app/src-tauri/linux/com.omznc.garmincompanion.metainfo.xml`
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
| Android | `.apk` (plus `.aab` for Play) | the `.apk` itself |

`latest.json` is generated alongside the desktop bundles and is the file those
apps fetch. `latest-android.json` is the equivalent for the APK and is written
by hand in the workflow — see below.

### Why Android is its own job

`tauri-action` builds the desktop bundles and writes `latest.json`. An Android
build produces neither, so it sits in a separate job that uploads through
`gh release upload` instead. Putting it in the matrix would have the action
either fail on a platform it doesn't bundle for, or publish a `latest.json` that
an APK can't be an update for.

### How Android updates itself

It does, and the note that used to be here saying it can't was wrong. What an
Android app can't do is replace its own package *silently* — that's reserved
for device owners. It can hand the system a new APK and have the system ask.

So the flow is: `app/src/lib/apk.ts` fetches `latest-android.json`, compares
versions, and calls the `download_apk` command in Rust, which streams the APK
into the app's cache and checks it against the hash in the manifest.
`ApkInstaller` on the Kotlin side then commits a `PackageInstaller` session, and
Android draws its own confirmation over the app. One tap. Everything before it
happens in the background.

Three things hold that up, and all three are load-bearing:

- **`REQUEST_INSTALL_PACKAGES`** in `AndroidManifest.xml`. Without it the
  session is refused outright.
- **"Install unknown apps"**, granted per-app by the user from Android 8. It is
  held by whichever browser the APK was first downloaded through, never by this
  app, so the *first* update sends them to a settings screen. After that it's
  silent until the tap. There is no callback when it's granted — the app just
  asks again next time the button is pressed.
- **The signing key.** Android installs over an existing package only when the
  new one carries the same signature. That, not the permission, is what makes
  any of this safe: a substituted APK would be offered as a separate app under
  its own name, not as an update to this one. The SHA-256 in the manifest is
  belt and braces — it stops a truncated download from reaching the installer,
  nothing more.

A fresh download waits in Settings rather than interrupting. One left over from
an earlier session offers itself at launch instead, once per version — which is
the cheapest moment to be asked to restart into something.

`latest-android.json` is written by the workflow rather than by a tool:

```json
{ "version": "0.2.0", "url": "https://…/garmin-companion_0.2.0.apk", "sha256": "…" }
```

It is not signed with the minisign key, and doesn't need to be — see the third
point above for what actually gates the install. Adding a signature would mean
carrying a verifier in the app to re-derive a guarantee the OS already enforces.

### Why the Play bundle is built twice

`REQUEST_INSTALL_PACKAGES` is on Google Play's restricted list — only
store-shaped apps may declare it — so the `.aab` has to go up without it while
the `.apk` keeps it.

The two artifacts come out of the *same* Gradle variant, so a flavour or build
type can't tell them apart, and adding a dimension that could would double the
per-ABI Rust compile: the expensive half of an Android build, four times over.
Instead `app/build.gradle.kts` registers a transform on the merged manifest that
strips the permission, active only when the `playBundle` project property is
set. The workflow runs `--apk` without it and `--aab` with
`ORG_GRADLE_PROJECT_playBundle=true`, which is the only channel available —
`tauri android build` can't pass a property through to Gradle.

The transform throws if it finds nothing to strip. That's deliberate: a silent
no-op there ships a restricted permission to Play review, and nothing about the
bundle would look wrong in the meantime.

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
  from Play, and the app needs its own grant of the same setting before it can
  update itself. Nothing fixes either except shipping on Play.

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
