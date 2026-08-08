# Garmin Companion

A desktop app that pulls your Garmin Connect history onto your own machine and
answers questions about it — with an emphasis on where your heart rate actually
spent the session, which is the thing Garmin's own app buries.

Ships with an MCP server over the same data, so Claude Desktop can read it too.

## What it does

Everything is read from a local SQLite cache rather than from Garmin, so the
screens open instantly and a tool call can't stall on a network hiccup
mid-conversation. Sync is the only thing that talks to Garmin.

- **Today** — recovery numbers, the last session in full with its zone
  breakdown, and hard-effort drift across recent runs
- **Activities / Activity** — the history, and per-lap splits and charts for
  one session
- **Health** — resting HR, HRV, sleep, stress, body battery over time
- **Food** — calories in against out, hydration
- **Ask** — questions answered by a model you choose, given only the metrics
  the question needs
- **Insights / Reports** — correlations mined from the cache, each stating its
  sample size, and weekly summaries
- **Plan / Routes / Gear** — saved workouts, routes grouped from GPS traces,
  and shoe and bike mileage

## Where your data goes

- **Garmin credentials** live in your OS keyring, never in the database.
- **Requests to Garmin** go straight from the app to Garmin, never through a
  server of ours.
- **Chat** sends your question plus whatever a tool returned for it — a handful
  of summary rows, never the whole database and never GPS traces. Where it sends
  that depends on which of the three providers you pick:
  - **Built-in coach** (the default) — through a small proxy this project runs,
    on to a hosted model. It sees the metrics a question needs and keeps none of
    them; the code is in [`worker/`](worker) and so is what it does and doesn't
    record. This is the one option where your data touches a server you don't
    run, which is why it says so on the Ask screen and in Settings.
  - **OpenRouter** with your own key — straight from the app to OpenRouter.
  - **Ollama** on your machine — nothing leaves the computer at all.
- **Switching** is one click in Settings, in either direction, at any time.
- **The cache** is a single SQLite file you can delete. Its path is printed in
  Settings.

## Running it

Requires Rust, Node 22+, pnpm, and on Linux `webkit2gtk-4.1`.

```sh
cd app
pnpm install
pnpm tauri dev
```

Connect an account in Settings, or adopt tokens from an existing
[`garminconnect`](https://github.com/cyberjunky/python-garminconnect) install in
one click if you have one.

There's also a headless sync, useful for a cron job:

```sh
cargo run -p garmin-core --bin sync -- 30        # last 30 days
cargo run -p garmin-core --bin sync -- 365 --full
```

## The MCP server

`crates/garmin-mcp` exposes the same queries as MCP tools — zone breakdowns,
drift, cadence, recovery, nutrition, workouts, routes. It shares the client,
token store and cache with the app, so an answer given in Claude Desktop and
one given in the app's Ask screen come from identical code.

```sh
cargo build --release -p garmin-mcp
./target/release/garmin-mcp import   # adopt existing tokens, once
```

`manifest.json` describes it for Claude Desktop.

## Layout

| Path | What's in it |
|---|---|
| `crates/garmin-core` | Garmin client, token store, SQLite cache, sync, queries |
| `crates/garmin-mcp` | MCP server over `garmin-core` |
| `app/src-tauri` | Tauri commands, chat and tool dispatch |
| `app/src` | React frontend |

The two Rust binaries share `garmin-core` so they can never drift onto
different versions of the Garmin client.

## Releases

See [RELEASING.md](RELEASING.md). Tagging `v*` builds installers for macOS
(Apple Silicon and Intel), Windows and Linux, and the app updates itself from
Settings.

## Licence

MIT. See [LICENSE](LICENSE).
