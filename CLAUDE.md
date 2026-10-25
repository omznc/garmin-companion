# CLAUDE.md

This file gives guidance to Claude Code for work in this repository.

This file loads at each start. Each area has its own `CLAUDE.md`, which loads
when you touch a file in that area. Longer subsystem notes are under
`docs/domains/`. See **Where the rest lives** below.

Personal context (biometrics, baselines, the home city) goes in
`CLAUDE.local.md`. Git ignores that file. Never put personal data in a
committed file: the repository is public.

## Commands

A Cargo workspace (`crates/*`, `apps/desktop`, `apps/login`) and three pnpm
packages that install separately (`apps/mobile`, `worker`, and the root, which
has no dependencies). Run from the repository root. Use `pnpm -C <dir>`, not
`cd <dir> &&`.

```bash
cargo fmt --all --check                               # CI
cargo clippy --workspace --all-targets -- -D warnings # CI: a warning fails the build
cargo test --workspace                                # CI: the live tests are #[ignore]d
scripts/gen-types.sh                                  # regenerate apps/mobile/src/data/generated
pnpm format                                           # worker + mobile (Biome) + cargo fmt
pnpm format:check

cargo build -p garmin-companion-login && cargo run -p garmin-companion-desktop

pnpm -C apps/mobile install --frozen-lockfile
pnpm -C apps/mobile test        # node --test "src/**/*.test.ts"
pnpm -C apps/mobile typecheck
pnpm -C worker install --frozen-lockfile
pnpm -C worker test             # vitest
pnpm -C worker typecheck

node scripts/check-metainfo.mjs           # CI metadata job
node scripts/check-privacy-manifest.mjs   # CI mobile job
node --test scripts/release-rewrite.test.mjs
```

CI (`.github/workflows/ci.yml`) runs on Linux only. It also runs a `jq` check
on `cargo metadata` for the test sandbox (see the rules below), and a
`git diff --exit-code` on the generated TypeScript types. Only the release
workflow builds macOS, Windows and iOS.

Biome only formats. Its linter is off (`biome.json`).

## Architecture

One Rust core, two apps, one cloud service:

- `crates/`: the shared Rust code. `garmin-core` (Garmin client, tokens,
  SQLite cache, sync), `garmin-api` (view models and the JSON command table),
  `garmin-coach` (the AI chat engine and its tools), `garmin-ffi` (the uniffi
  bridge for mobile) and `garmin-mcp` (a stdio MCP server). See
  `crates/CLAUDE.md`.
- `apps/desktop`: the gpui-ce desktop app, binary `garmin-companion`. It calls
  `garmin-api` functions directly. `apps/login` is its sign-in helper (a wry
  webview). See `apps/desktop/CLAUDE.md`.
- `apps/mobile`: the Expo app for Android and iOS. It reaches Rust only
  through `garmin-ffi` and the commands that `garmin-api` dispatches. See
  `apps/mobile/CLAUDE.md`.
- `worker/`: the `coach` Cloudflare Worker. It proxies the cloud coach to
  OpenRouter and holds credits, login and billing. See `worker/CLAUDE.md`.

The data flow is cache-first. A sync writes Garmin data into one SQLite file.
Every screen, coach tool and MCP tool reads that file, never Garmin. The
desktop app, the mobile app and `garmin-mcp` can share one file.

No Tauri app exists any more (`app/` was deleted in `f2d3f14`). The Tauri
bundler still packages the desktop app, so `apps/desktop/tauri.conf.json`
holds the version, the bundle settings and the updater key.

## Where the rest lives

These files do not load by themselves. **Read the file before you change the
area.**

| Read this | Before you touch |
|---|---|
| `docs/domains/sync.md` | `garmin-core` sync, the Garmin client, retries, day keys, sample mode, or a new view command |
| `docs/domains/coach.md` | `garmin-coach`, the desktop coach providers (cloud, OpenRouter, Ollama, Claude Code, OpenCode, Codex), or the coach tools |
| `RELEASING.md` | a version, the release, Play or TestFlight workflows, the updater manifests, or a signing key |
| `worker/V2.md` | the `/v2` contract between the apps and the worker |
| `worker/README.md` | the worker setup, its secrets or a deploy |
| `PRIVACY.md` | anything that sends data off the device |

## Rules for all areas

- **A test never reaches real user state.** No real data folder, no OS
  keyring, no Garmin call. Each crate that links `garmin-core` enables the
  `test-sandbox` feature in `[dev-dependencies]` only, and CI checks it. A
  test once ran `sign_out` and deleted the keyring tokens of the developer
  (`9928a60`). See `crates/garmin-core/src/paths.rs`
- **The core writes the sentences, and the apps only draw them.** Every
  sentence and plural of a Today card or an insight comes from Rust, so the
  desktop and mobile apps show the same text. See `crates/CLAUDE.md`
- **Serve every Garmin user, not only runners.** Health data is the base.
  A sport section shows only when the data shows that sport. Goals start
  empty. No sport words outside the core, and no personal facts of the owner
  in shipped text. See `crates/CLAUDE.md`
- **Never hand-edit `apps/mobile/src/data/generated/`.** Change the Rust type,
  run `scripts/gen-types.sh`, and commit the result. CI fails on a difference.
- **No panic in release code.** The `release` profile uses `panic = "abort"`.
  Do not `unwrap` a value that can fail, and use `total_cmp`, not
  `partial_cmp().unwrap()`, for an `f64` that can be NaN (`83e2549`).
- **The only write to Garmin is `create_workout`,** and only a button press
  starts it. A write is never retried. See `crates/garmin-core/src/client.rs`
- **The worker never logs or stores message content.** See `worker/CLAUDE.md`

## Conventions

- Commits use conventional commits with a scope: `fix(mobile):`,
  `feat(desktop):`, `fix(core):`, `feat(api):`, `fix(worker):`,
  `chore(release):`. The body says why, not what.
- Each change is a branch and a pull request into `master`, merged with a
  merge commit. `master` is the default branch and the only release branch.
- A release starts with a `release-X.Y.Z` branch with one commit,
  `chore(release): prepare X.Y.Z`. It adds the metainfo `<release>` entry and
  sets `version` in `apps/desktop/tauri.conf.json` and `apps/mobile/app.json`.
  Then dispatch the workflow from `master`. See `RELEASING.md`.
- Do not commit scratch notes, screenshots or a new file under `docs/` unless
  the user asks for it. A pull request holds the code change and its tests.

## Gotchas

- The CI of a pull request runs on the merge commit. After a fix on `master`,
  run `gh pr update-branch`. A re-run of the old check does not use the new
  `master`.
- `gh workflow run` runs a `workflow_dispatch` workflow only from `master`.
  `release-rewrite.yml` needs `-f version=X.Y.Z`. Without it, the `check` job
  fails.
- Do not share one `CARGO_TARGET_DIR` between worktrees. Cargo linked a stale
  crate from a different worktree.
- The desktop crate needs the GTK, WebKitGTK and cairo dev packages
  (the `apt-get` line in `ci.yml` lists them). `garmin-core` and `garmin-mcp`
  build without them.
- Some comments still talk about "the Tauri app" (`paths.rs`, `dispatch.rs`).
  They mean the desktop app.
