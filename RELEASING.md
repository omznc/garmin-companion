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
`ANDROID_MANIFEST` constant in `app/src-tauri/src/lib.rs`, read by the app
itself — the plugin isn't compiled in on Android, so there is nothing to
configure and the URL lives next to the code that fetches it.

That fetch is in Rust rather than in the webview, and has to stay there: GitHub
serves release assets from a host that sends no `Access-Control-Allow-Origin`,
so the same URL fetched from the page is refused by the WebView every time,
which the app can only read as "no new version". A third slug appears in
`RELEASE_API` in `app/src/lib/apk.ts` — that one is `api.github.com`, which does
send the header, and it fetches the release notes rather than the manifest.

Those three lines are the only places the repo slug appears. If it ever moves,
all of them change or half the installed copies stop hearing about releases.
Getting one wrong doesn't break the build — it breaks update checks silently.

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

Without the Android secrets, the `android` job of the release workflow cannot
sign the APK. The release then fails. Set the secrets before you cut a release.

**Code signing with an OS vendor certificate** is the one thing still not done,
and the only one that's a decision rather than a task. See below.

## Cutting a release

`.github/workflows/release-rewrite.yml` is the only release workflow. The old
Tauri workflow `release.yml` is deleted. It fired on every `v*` tag, so it
would have built the old app at 0.9.1 and replaced `latest.json` and
`latest-android.json`.

Do not push a tag by hand. Publishing the draft release creates the tag, and no
workflow runs on a tag.

1. Add the `<release>` entry to the metainfo file and set `version` in
   `apps/desktop/tauri.conf.json` (see below). Merge both to `master`.
2. Open Actions, then "Release (rewrite)", then "Run workflow". Run it from
   `master`.
3. Enter the version (`major.minor.patch`, higher than 0.9.1) and the release
   notes. The old app shows the notes in Settings.
4. The `check` job refuses a bad version or a version that exists. The
   `desktop` jobs, the `android` job and the `ios` job build the files. The `draft` job writes
   `latest.json` and opens a **draft** release. The draft has no tag.
5. Check the draft. Install the files on a test machine if possible.
6. Publish the draft in the GitHub UI. This creates the tag `vX.Y.Z` and makes
   the updater see the release, because the endpoints point at
   `releases/latest`.
7. Raise `version` in `apps/mobile/app.json` to the released version. A local
   build must not be lower than the latest release.

The version is an input of the workflow. The workflow gives it to the Tauri
bundler (`--config`) and to the Expo build (`APP_VERSION`). The Android version
code is `major * 1000000 + minor * 1000 + patch`. `apps/mobile/app.config.js`
calculates it.

`app/src-tauri/linux/com.omznc.garmincompanion.metainfo.xml` needs a new
`<release>` entry for the version. The `check` job of the release workflow
stops if the entry is missing. Set `version` in `apps/desktop/tauri.conf.json`
to the same version. The `metadata` job of `ci.yml` reads that file.

Artifacts:

| Platform | Installer | Updater artifact |
|---|---|---|
| macOS | `.dmg` (new users) | `.app.tar.gz` + `.sig` |
| Windows | `-setup.exe` (NSIS) | the `-setup.exe` + `.sig` |
| Linux | `.AppImage`, `.deb`, `.rpm` | the same files + `.sig` |
| Android | `.apk` | the `.apk` itself |
| iOS | `_unsigned.ipa` | none |

`latest.json` is written by `scripts/release-rewrite.mjs manifest` from the
`.sig` files. `latest-android.json` is written by the `android` job. Test the
script with `node --test scripts/release-rewrite.test.mjs`.

The `ios` job builds an unsigned device IPA, `garmin-companion_<version>_unsigned.ipa`.
Testers sign it themselves with Sideloadly or AltStore. It is not in
`latest.json`, and no updater reads it. When a tester signs the IPA, the
new signature must keep an application identifier. The Keychain items of the
app need one, and the app cannot read its tokens without it.

> The sections below, up to "Building locally", describe the old Tauri
> release. They stay as background for the APK format, the signing keys and
> the updater.

### Why the Android job is the slow one, and what keeps it honest

It builds the Rust workspace once per ABI, and each pass ends with OpenSSL
compiled from C source (see the `openssl` dependency in
`crates/garmin-core/Cargo.toml`), which is most of the ~2m35 a pass costs. Four
ABIs made it a 20-minute job against 5–10 for each desktop bundle.

One thing cuts that, and it is load-bearing if you touch it:

- **Two ABIs, not four.** `-t aarch64 armv7` on both build steps. `i686` and
  `x86_64` are emulator ABIs — no phone runs either — so they were five minutes
  a release spent on something nobody installs. The list appears in three
  places: the two build steps, the `targets:` on the toolchain, and the RANLIB
  loop. All three have to agree.

A cold desktop build takes 5-10 minutes. Only the first release starts with a
cold cache, because a later run restores the cache of the default branch.

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

So the flow is: `app/src/lib/apk.ts` asks the `latest_apk` command in Rust for
`latest-android.json`, compares versions, and calls `download_apk`, which streams the APK
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
APK with a lower code does not install over a newer one.

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

## Why updating asks for a password on Linux, and only sometimes

Because a `.deb` or `.rpm` install is a system package, and replacing one needs
root the same way installing it did. `tauri-plugin-updater` runs `dpkg -i` or
`rpm -U` under `pkexec`, so what appears is polkit's dialog, not ours. There is
no version of "replace a package under `/usr`" that doesn't ask.

The AppImage is the exception, and it's the reason this looks inconsistent: it's
a single file the user owns, so the plugin swaps it in place and nobody is asked
anything. Same for macOS and Windows. **If you want silent updates on Linux, run
the AppImage** — that is the whole of the fix.

`latest.json` carries a `linux-x86_64-deb` and a `linux-x86_64-rpm` entry beside
the AppImage, and the plugin picks between them by reading the bundle type the
bundler stamped into the binary. Deleting those two entries would not remove the
prompt, it would break updates outright: the plugin would still know it is an
rpm, still refuse to install an AppImage over it, and fail with
`InvalidUpdaterFormat`.

What *was* worth fixing is when the prompt appears. `downloadAndInstall()` does
both halves in one call, so a background check four seconds after launch put a
password dialog on screen unasked. `app/src/lib/updater.ts` now splits the two
on deb and rpm — `download()` in the background, `install()` behind the button
in Settings — and says what the button is about to do before you press it.
Dismissing the dialog puts the offer back rather than reporting a failure; the
download is still there.

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

- **The prose in `metainfo.xml`.** The version and the date are checked, the
  paragraphs are not — nothing can check whether a description describes
  anything. Write it with the version bump; CI will tell you if you didn't.
  Everything else about the changelog is generated, see above.
- **Version bumping.** Deliberately manual. The version is in the workflow
  input, the metainfo entry, `apps/desktop/tauri.conf.json` and
  `apps/mobile/app.json`. See "Cutting a release".
- **iOS.** The Rust side is already platform-split for it — `secrets`, `paths`
  and the mobile sign-in flow are written against `cfg(mobile)` rather than
  against Android — and the frontend branches on `IS_MOBILE`, not on `android`.
  What's missing is an Apple Developer account, and a signed build. The `ios`
  job makes only an unsigned IPA (see "Cutting a release").

## TestFlight

The workflow `.github/workflows/testflight.yml` builds a signed iOS app and
uploads it to TestFlight. Run it by hand after you publish the release:
Actions, then "TestFlight", then "Run workflow". Enter the tag, for example
`v1.0.5`. The tag must exist.

Without the six secrets below, the run prints a notice, skips the build and
passes. The workflow is not tested on a real account yet. Expect to fix the
first run.

Secrets:

- `ASC_KEY_ID`: key ID of the App Store Connect API key.
- `ASC_ISSUER_ID`: issuer ID of the same page.
- `ASC_KEY_P8`: the text of the `AuthKey_<id>.p8` file.
- `IOS_DIST_CERT_P12`: the Apple Distribution certificate, as base64 of a `.p12` file.
- `IOS_DIST_CERT_PASSWORD`: the password of that `.p12` file.
- `IOS_PROFILE`: base64 of an App Store provisioning profile for `com.omznc.garmincompanion`.

Setup:

1. Join the Apple Developer Program.
2. Make the App ID `com.omznc.garmincompanion` in the developer portal.
3. Make the app record in App Store Connect.
4. In Users and Access, then Integrations, make an API key with the role
   App Manager. Download the `.p8` file one time only.
5. Make an Apple Distribution certificate. Export it from Keychain Access as `.p12`.
6. Make an App Store provisioning profile for the App ID and the certificate.
7. Set the secrets: `base64 -w0 file.p12 | gh secret set IOS_DIST_CERT_P12`.
   Use the same method for `IOS_PROFILE`. Use `gh secret set ASC_KEY_P8 < AuthKey.p8`.

The build number is the run number of the workflow. The version comes from the
tag. The workflow does not set the export compliance answer. Set it in App Store
Connect, or the build stays at "Missing Compliance".

The script `scripts/check-privacy-manifest.mjs` checks that the app declares the
required-reason APIs (`ios.privacyManifests` in `apps/mobile/app.json`). CI runs
it on each pull request. The iOS jobs run it on the manifest that prebuild
makes. If a new module uses another such API, add its category to the script and
to `app.json`.

## Google Play

The workflow `.github/workflows/play.yml` builds a signed bundle (`.aab`) and
uploads it to a Play track. Run it by hand after you publish the release, with
the same version: Actions, then "Play", then "Run workflow". It builds the tag
`v<version>`, so the tag must exist. Choose the track (the default is `alpha`,
the closed testing track) and the status.

- Secret: `PLAY_SERVICE_ACCOUNT_JSON`. It holds the key of the service account
  `play-publisher@service-accounts-512208.iam.gserviceaccount.com`.
- Play App Signing uses the same keystore as the APK. On 2026-10-20 the
  keystore went to Play with PEPK ("Export and upload a key from Java
  keystore"), before the first bundle. So the Play build and the standalone APK
  can update each other. The App signing page must show the SHA-256
  `81:31:76:E8:DA:FE:AF:56:07:83:3A:CB:BD:BB:28:66:0C:06:9D:25:0F:06:64:BD:F2:DF:E9:22:60:94:8E:D9`.
  If it shows a different value, Play made its own key. Do not upload a
  bundle then.
- The Play build has no self-update. The workflow sets `EXPO_PUBLIC_SELF_UPDATE=0`,
  so the bundle has no `REQUEST_INSTALL_PACKAGES`. The workflow checks this.
- Until the app is first published, Play accepts only the status `draft`. Use
  `draft`, then roll out in the Play Console.
