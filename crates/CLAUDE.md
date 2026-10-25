# crates

The shared Rust code. Read the root `CLAUDE.md` first for the commands and the
rules for all areas.

## Layout

- `garmin-core`: the Garmin client, the tokens, the SQLite cache and the sync.
  Every other crate and the desktop app depend on it.
  - `paths.rs`: the data folder (`set_base_dir`) and `SANDBOXED`.
  - `secrets.rs` + `store.rs`: the secret store. The OS keyring on desktop,
    the Keychain on iOS, an AES-GCM file on Android. `secrets` is the only
    module that knows the platform.
  - `auth.rs`, `client.rs` (`GarminClient`, retries), `db.rs` (SQLite,
    migrations), `sync.rs` + `sync_extra.rs`.
  - `query.rs`: the analyses that the MCP tools and the coach tools share.
  - Domain modules: `analysis`, `findings`, `coach` (nudges), `stats`,
    `sleep`, `records`, `goals`, `workout`, `calendar`, `courses`, `devices`,
    `intraday`, `nutrition`, `signal`, `sport`, `strength`, `theme`, `vo2`.
  - `src/bin/sync.rs` and `src/bin/spike.rs` are live tools. They read the real
    tokens and call Garmin.
- `garmin-api`: the view models over `Db`, and the JSON command table. It
  makes no network call.
  - One module per screen (`today`, `activities`, `health`, `sleep_view`,
    `body`, `fitness`, `insights`, `reports`, `tiles`, `charts`, `plan`,
    `routes`, `places`, `gear`, `export`).
  - `commands.rs` (`COMMANDS`) + `dispatch.rs` (`call`, `Source::{Live,
    Sample}`): the one command table. Mobile reaches the views only through
    it. The desktop app calls the `*_view` functions directly.
  - `sample.rs` + `personas.rs`: the sample data and the personas (the
    sample runner, golfer, gym, health only, new user).
  - `bin/garmin-api-fixture.rs`: writes `fixtures/*.json`.
- `garmin-coach`: the chat engine. `turn.rs` (`run_turn`, `Event`), `tools.rs`,
  `prompt.rs`, `transport.rs` (a trait, with a fake for tests), `cloud.rs` (the
  worker client, `CLOUD_BASE`), `external.rs` (read-only tools for the CLI
  agents). See `docs/domains/coach.md`.
- `garmin-ffi`: the uniffi bridge for mobile. It exports `init`, `version`,
  `subscribe` and an async `call(cmd, args_json)`. `chat.rs` holds the local
  chat threads. `contract.rs` (test only) writes `commands.ts` and lists the
  FFI commands and events.
- `garmin-mcp`: a stdio MCP server that depends only on `garmin-core`.
  `garmin-mcp [import [PATH] | status | sync [DAYS] [--full] | coach]`. Every
  tool reads SQLite, except `sync`.

## Rules

- **The test sandbox.** `GarminClient::new`, sign-in, refresh and
  `client_from_keyring` refuse to run under `SANDBOXED`, and secrets stay in
  memory. A new crate that links `garmin-core` enables `test-sandbox` in
  `[dev-dependencies]` and never in `[dependencies]`. In a test, call `init`
  with a temp folder, and never send the account commands (`sign_out`,
  `import_tokens`) through the live path.
- **`garmin-core` has no dependency on an app,** because `garmin-mcp` links
  it. `set_base_dir` works on the first call only. `base_dir()` returns an
  error, not the working directory. In `garmin-ffi`, `init` runs before
  `call`.
- **Tokens stay in the secret store.** Never in a config file, the cache or a
  log. A token refresh must save the rotated token (`auth.rs`).
- **Migrations only add.** The version is in `PRAGMA user_version`
  (`SCHEMA_VERSION`). `migrate_v1` and the `SCHEMA` constants are frozen. For a
  change, add a new `if version < N` step and raise the version. A read-only
  copy of the cache skips the version stamp.
- **One cache file, more than one process.** The desktop app, the mobile app
  and `garmin-mcp` can open the same file. The DB uses WAL and a 5 s busy
  timeout. `sync_all_with` takes the cross-process `sync.lock`.
- **Every public `*_view` needs a command in `COMMANDS`, or a stated reason.**
  Tests in `commands.rs` check that `COMMANDS` and the dispatch agree. A new
  command also needs a sample answer (see `docs/domains/sync.md`).
- **Release profiles.** `[profile.*]` works only in the root `Cargo.toml`.
  Build `garmin-ffi` with `--profile release-mobile` (unwind). The plain
  `release` profile aborts on a panic, and a `compile_error!` refuses it.
- **Keep `native-tls`.** Nobody has tested rustls against the Cloudflare edge
  of Garmin. Android uses vendored OpenSSL.
- **MCP uses stdout for the protocol.** All diagnostics go to stderr.
- **The audience rule.** The coach tools and the sentences must not favour one
  sport. Sport words stay in the core (`sport.rs`). A
  motor sport (for example motorcycling) does not count as exercise. When the
  sample is too small, an insight returns `None`, not a weak sentence.

## Testing

- `cargo test --workspace`. The `garmin-ffi` tests share process state and
  take `TEST_LOCK`.
- The fixtures in `crates/garmin-api/fixtures` are committed. The
  `committed_fixtures_are_current` tests (in `sleep_view.rs`, `places.rs`,
  `training_fixtures.rs` and others) fail when a view change makes them stale. Regenerate them with
  `cargo run -p garmin-api --bin garmin-api-fixture -- --personas` (also
  `--body`, `--training`, `--views`, `--sample`).
- `garmin-api-fixture <db copy>` writes `fixtures/today.json` from a real
  cache. That file holds real data. Git ignores it. Never commit it.
- A change to a wire type (a `ts-rs` export) needs `scripts/gen-types.sh`.
  `.cargo/config.toml` sets `TS_RS_EXPORT_DIR`. Without it, the `garmin-ffi`
  contract test panics.
- Live checks: `cargo run -p garmin-coach --example ask_live -- "question"`.
  It uses the real keyring and network, and each run takes a new install id.
