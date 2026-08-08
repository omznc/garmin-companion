# app

The Tauri shell and React frontend. See the [root README](../README.md) for
what this is and how to run it.

- `src/` — screens, components, and the formatting and derivation helpers they
  share. No data fetching lives in components; everything goes through
  `src/lib/api.ts` to a Tauri command.
- `src-tauri/` — the Rust side: commands, chat and tool dispatch, bundling and
  updater config.
