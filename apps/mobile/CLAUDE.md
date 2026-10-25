# apps/mobile

The Expo app for Android and iOS (Expo SDK 58, React Native 0.88, expo-router).
It reaches Rust only through `crates/garmin-ffi`. Read the root `CLAUDE.md`
first.

## Commands

pnpm 10 and Node 22. The package installs on its own.

```bash
pnpm -C apps/mobile install --frozen-lockfile
pnpm -C apps/mobile start          # expo start --dev-client
pnpm -C apps/mobile android        # or ios
pnpm -C apps/mobile test           # node --test "src/**/*.test.ts"
pnpm -C apps/mobile typecheck      # tsc -p tsconfig.check.json
pnpm -C apps/mobile format:check   # Biome
pnpm -C apps/mobile prebuild:android

source scripts/android-env.sh && pnpm -C apps/mobile bridge:android
IPHONEOS_DEPLOYMENT_TARGET=16.4 pnpm -C apps/mobile bridge:ios --targets aarch64-apple-ios
```

`scripts/android-env.sh` sets NDK 28.2.13676358, JDK 21 (Gradle refuses 25),
`llvm-ranlib` and the Perl modules for the vendored OpenSSL.
`scripts/android-perl-shim.sh` is the rootless fallback. There is no EAS: each
build is local Gradle or Xcode, in GitHub Actions for a release.

## Layout

- `index.ts` imports `src/notify/backgroundSync` before `expo-router/entry`.
  The headless background task needs it.
- `app/`: the routes. `_layout.tsx` is the root stack, the query client, the
  foreground sync and the notification taps. `(tabs)/{today,activities,health,
  sleep,more}`. `ask/` is a root stack with no tab bar. `sign-in.tsx` is a
  webview of the Garmin sign-in page. `setup.tsx`.
- Most screens come as `.android.tsx` and `.ios.tsx` pairs (`moduleSuffixes`
  in `tsconfig.json`). Screens are in `src/screens/`, the kits in
  `src/components/`, and the rest in `src/more/`, `src/ask/`, `src/nav/`.
- `src/data/source.ts`: the one data source. It uses the Rust bridge when the
  native module exists, and the JSON fixtures if not.
- `src/data/generated/`: types from Rust (`scripts/gen-types.sh`).
  `src/data/types.ts` re-exports them. Never copy a shape by hand.
- `packages/garmin-bridge/`: the ubrn turbo module, built from
  `crates/garmin-ffi`. Git holds only `package.json`, `ubrn.config.yaml` and
  `.gitignore`. ubrn writes the rest. `types/garmin-bridge.d.ts` is a stub for
  the typecheck. Keep it equal to the exports of `crates/garmin-ffi/src/lib.rs`.
- `modules/`: Android-only Kotlin modules (`apk-updater`, `back-gesture`,
  `night-mode`, `sync-service`). The JS uses `requireOptionalNativeModule`, so
  it gets `null` on iOS.
- `plugins/`: config plugins (`self-update.js`, `data-extraction.js`,
  `night-mode.js`, `top-refresh-rate.js`).
- State: TanStack Query plus small `useSyncExternalStore` stores
  (`theme/prefs`, `nav/tabPrefs`, `data/focus`, `data/useSync`). No Redux.
- Notifications are local only, with no push. Background sync is
  `expo-background-task` (see `docs/domains/sync.md`).

## Design rules (user decisions)

- **Android must feel Material 3 Expressive and Material You,** like the Farmer
  app, not plain M3. iOS follows the Apple HIG with `PlatformColor`.
- `src/theme/md.ts` is for Android only. Import it only from `.android.tsx`
  files. Spread `useHostTheme()` on every `Host`.
- **Haptics are off by default** (`noVibration: true` in `prefs.ts`). Every
  haptic goes through the gate in `src/ui/haptics.ts`. A failed haptic never
  fails an action. No haptic on a tab switch.
- **Notifications are off by default.** Ask for the permission only when the
  person turns them on. Send only from 08:00 to 20:59. A notification never
  fails a sync.
- **No purchase in the mobile app.** No checkout call (`src/data/useCloud.ts`).
- **No local coach provider on a phone** (no Ollama, no CLI agent).

## Rules

- **Never edit `android/` or `ios/`.** They are generated (CNG) and Git
  ignores them. Change `app.json`, `app.config.js` or a plugin, then prebuild
  again.
- **Rebuild the native app** after a change to `crates/`, `modules/*/android`,
  `plugins/` or the native config in `app.json`. Metro reloads only JS.
- **R8 keep rules.** R8 removed constructors that WorkManager reaches by
  reflection, and 1.1.0 crashed at start (`WorkDatabase_Impl()`, then
  `OverwritingInputMerger()`). The keep rules are in `app.json`
  `extraProguardRules`. `scripts/check-r8-keeps.sh` checks them after a
  release build. Test a new native library in a release build: a debug build
  has no R8.
- **The Play build has no `REQUEST_INSTALL_PACKAGES`.** `EXPO_PUBLIC_SELF_UPDATE=0`
  removes it. The standalone APK updates itself from `latest-android.json`.
- **Backup rules.** `plugins/data-extraction.js` keeps `secrets.bin` and
  `secrets.key` out of an Android backup. iOS excludes the data folder from
  backup, and the tokens stay in the Keychain on this device only.
- **Window colour.** Set only the Android `backgroundColor`. A top-level or iOS
  `backgroundColor` keeps the window light in dark mode
  (`src/appConfig.test.ts`).
- **iOS header.** Keep the navigation `ThemeProvider` in `app/_layout.tsx`, or
  the dark mode header text is black. A nested `ScrollView` gets no inset under
  the translucent header: pad it with `useHeaderHeight()`.
  `headerLargeTitle: false` draws no title on iOS 26. The native tab bar cannot
  reorder at runtime.
- **A native Android `Switch` ignores `pointerEvents`.** It flips itself, so
  it must report the change.
- **Keep the fixture `require` inline** in `source.ts`, so Metro removes the
  fixtures from a release build.
- **The privacy manifest.** A new required-reason API goes in
  `scripts/check-privacy-manifest.mjs` (`REQUIRED`) and in `app.json`
  `ios.privacyManifests`.
- **The version.** `app.json` `version` is the floor for a local build. It must
  not be lower than the latest release (`app.config.js`). The version code is
  `major * 1000000 + minor * 1000 + patch`.
- **Release ABIs** are arm64-v8a and armeabi-v7a, with minSdk 24, so old APK
  users still get updates. Dev builds use arm64-v8a and x86_64.

## Gotchas

- Do not run `pkill -f "expo start"`. It also stops the Metro server of a
  different session on port 8081. Start Metro on a free port.
- Dev env vars: `EXPO_PUBLIC_FIXTURE=1` (fixtures, no Setup),
  `EXPO_PUBLIC_JS_FIXTURE=1`, `EXPO_PUBLIC_DEMO_DATE`,
  `EXPO_PUBLIC_PERSONA=golfer|gym|health_only|new_user`.
- A background task keeps its interval across updates. A new interval needs a
  new registration (`backgroundSync.ts`).
- `.github/workflows/play.yml` and `testflight.yml` copy the setup steps of
  the `android` and `ios` jobs in `release-rewrite.yml`. A change to those
  jobs also goes into the copies.
