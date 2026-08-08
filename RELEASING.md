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

**3. Push the repo.** It currently has no commits and no remote.

```sh
git add -A && git commit -m "Initial commit"
gh repo create omznc/garmin-companion --private --source=. --push
```

**4. Decide about code signing** (or decide not to care). See below.

## Cutting a release

The version in `app/src-tauri/tauri.conf.json` is the source of truth — the
updater compares against it, not against the tag. Bump it, then tag to match:

```sh
# edit "version" in app/src-tauri/tauri.conf.json, then:
git commit -am "Release v0.2.0"
git tag v0.2.0
git push origin master --tags
```

`.github/workflows/release.yml` fires on the tag and builds four bundles in
parallel: macOS arm64, macOS x64, Linux x64, Windows x64. It opens the release
as a **draft** so you can look at it before anyone downloads — publish it in the
GitHub UI when all four jobs are green. Publishing is what makes the updater
see it, because the endpoint points at `releases/latest`.

Artifacts:

| Platform | Installer | Updater artifact |
|---|---|---|
| macOS | `.dmg` | `.app.tar.gz` + `.sig` |
| Windows | `-setup.exe` (NSIS) | `.nsis.zip` + `.sig` |
| Linux | `.AppImage`, `.deb`, `.rpm` | `.AppImage.tar.gz` + `.sig` |

`latest.json` is generated alongside them and is the file the app fetches.

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

## Building locally

```sh
cd app && pnpm tauri build
```

Bundles land in `target/release/bundle/`. Unsigned unless
`TAURI_SIGNING_PRIVATE_KEY` is in the environment, which is fine for testing —
just don't hand a locally built bundle to someone expecting updates to work.

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

For a personal app this is a shrug. If you ever hand it to someone else, macOS
notarization is the one worth buying.

## What isn't automated

- **The changelog.** The release body is a fixed blurb in the workflow. If you
  want per-release notes, write them into the draft before publishing.
- **Version bumping.** Deliberately manual — one number, and it has to agree
  with the tag.
- **Anything mobile.** `tauri icon` generates iOS and Android assets; they were
  deleted. This is a desktop app.
