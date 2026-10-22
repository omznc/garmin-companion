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

The first is in `apps/desktop/tauri.conf.json` under `plugins.updater.endpoints`.
The desktop app has the same URL in `MANIFEST_URL` in `apps/desktop/src/updater.rs`.
The second is the constant in
`apps/mobile/src/update/manifest.ts`.

The desktop app also has `RELEASE_PAGE` in `apps/desktop/src/updater.rs`. It
points to the release page. Run `rg 'omznc/garmin-companion'` to find the
other places. If the repository moves, change all of them. A wrong URL does not
break the build. It breaks the update check without a message.

**The updater key**, which proves a bundle came from you. The app refuses any
update it can't verify against the public half baked into it.

- private: `~/.tauri/garmin-companion.key`, held by CI as
  `TAURI_SIGNING_PRIVATE_KEY` (its password is empty — the key was generated
  without one)
- public: `~/.tauri/garmin-companion.key.pub`, pasted into `apps/desktop/tauri.conf.json` as
  `plugins.updater.pubkey`. The same value is `PUBKEY` in `apps/desktop/src/updater.rs`.

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

`apps/desktop/linux/com.omznc.garmincompanion.metainfo.xml` needs a new
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

### Why Windows is NSIS and not MSI

`bundle.targets` is `"all"` everywhere except Windows, where the release
workflow passes `--bundles nsis`. JSON cannot hold the reason, so
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

## Why updating asks for a password on Linux, and only sometimes

Because a `.deb` or `.rpm` install is a system package, and replacing one needs
root the same way installing it did. The desktop app
(`apps/desktop/src/updater_pkg.rs`) checks the signature, then runs `apt-get`
or `dpkg` (deb), or `dnf` or `rpm` (rpm), under `pkexec`. The dialog is
polkit's, not ours. No way to replace a package under `/usr` avoids the prompt.

The AppImage is the exception. It is a single file that the user owns, so the
app replaces it in place and nobody sees a prompt. The same is true for macOS
and Windows. **For silent updates on Linux, run the AppImage.**

The install starts only when the user presses the button in Settings. The
background check does not show the dialog.

`pkexec` exit codes:

- 0: the package is installed. The app restarts.
- 126: the user closed the dialog. The app keeps the update offer and shows no
  error. The user can try again.
- 127: no polkit agent. The app opens the release page.

A missing `pkexec` or a missing package tool also opens the release page. Any
other exit code shows the last line of the tool's message as an error.

## Code signing — what "unsigned" actually costs

Nothing here is signed with an OS vendor certificate, only with the updater's
own minisign key. That key proves an update came from you; it does nothing for
Gatekeeper or SmartScreen.

- **macOS**: Gatekeeper refuses to open the app on first launch. Users have to
  right-click → Open, or run `xattr -dr com.apple.quarantine`. Fixing this
  properly needs an Apple Developer account (~$99/yr) and notarization —
  `@tauri-apps/cli bundle` supports it through `APPLE_CERTIFICATE`,
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
- **iOS.** The app in `apps/mobile` is an Expo app and builds for iOS. The
  `ios` job makes only an unsigned IPA (see "Cutting a release"). The workflow
  `testflight.yml` makes a signed build, but it needs an Apple Developer
  account. See "TestFlight".

## TestFlight

The workflow `.github/workflows/testflight.yml` builds a signed iOS app and
uploads it to TestFlight. Run it by hand after you publish the release:
Actions, then "TestFlight", then "Run workflow". Enter the tag, for example
`v1.1.0`. The tag must exist. The tag must include `scripts/check-privacy-manifest.mjs`,
so v1.0.5 and the earlier tags do not work.

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
uploads it to a Play track. The release run calls it in the `play` job. The job
uploads the bundle to the `alpha` track with the status `completed`. A failed
`play` job does not stop the `draft` job.

Run the workflow by hand only to upload again, or to use another track:
Actions, then "Play", then "Run workflow". Use the same version. It builds the
tag `v<version>`, so the tag must exist. Choose the track and the status.

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
