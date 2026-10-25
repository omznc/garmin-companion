# The coach

Read this before you change `crates/garmin-coach`, the desktop coach providers,
the coach tools, or the cloud path to the worker.

## The engine (`crates/garmin-coach`)

- The model reads the cache only through tools. It has no network path to
  Garmin.
- `run_turn` (`turn.rs`) opens the cache for each tool call, so no DB handle
  lives across an `await`. It emits `Event`s to an `EventSink`.
- The `drafts` flag adds the `draft_workout` tool and its prompt text. Set it
  only for a host that shows a workout card. A saved draft goes to Garmin one
  time only, through `workout_create`, after a button press.
- The tools must not favour one sport (`tools.rs`). The tools in
  `external.rs` (for the CLI agents) are read-only and have no draft tool.
- `transport.rs` is a trait with a fake. Coach tests use the fake and never
  touch the OS keyring (CI has no keyring).
- Timeouts are a connect limit, a header deadline and an idle limit, not one
  total timeout (`dd23601`). A long answer must not stop half way.

## Providers (`config.rs`)

- `Cloud`: the `coach` worker (`cloud.rs`, `CLOUD_BASE`, contract in
  `worker/V2.md`). It costs credits.
- `Openrouter` (the person's own key) and `Ollama`.
- `ClaudeCode`, `OpenCode`, `Codex` (`is_cli`): desktop only.
- `is_local` (Ollama or a CLI) needs the same computer. The mobile app never
  offers a local provider.
- Opening Ask must not use up an install id. The app enrols an install only
  when the person sends a question.

## The CLI providers (`apps/desktop/src/cli_agent.rs`)

`claude_code.rs`, `opencode.rs` and `codex.rs` share `cli_agent.rs`:

- Each question starts the CLI headless. The CLI is stateless, so the app
  sends the trimmed history each turn.
- The CLI gets one MCP server for that run only: the desktop binary itself
  (`garmin-companion mcp`, `apps/desktop/src/mcp.rs`). Claude Code and Codex
  may use only those MCP tools. OpenCode also allows `execute` and `read`
  (`opencode.rs`). Its `fetch` can make HTTP requests. The binary path is `$APPIMAGE` or `current_exe`, with the
  " (deleted)" suffix removed after an update.
- `claude_code.rs` removes the env vars of a parent Claude Code session
  (`CLAUDECODE`, `CLAUDE_CODE_ENTRYPOINT` and others) before the start.
- Kill the whole process tree on cancel and on app quit. A run must not
  outlive the app. Temp folders use mode 0700.
- On Windows, `claude.cmd` takes the prompt on stdin and a config file, not
  arguments. Child processes start with `CREATE_NO_WINDOW`.
- The flags of OpenCode changed between v1 and v2 (v2 removed `run --pure` and
  `--dir`, and needs `--standalone`). This broke five times. Probe `--help`.
  Do not assume a flag.
- The parser fixtures are `apps/desktop/tests/*.jsonl`. The live tests are
  `#[ignore]` and need `GC_LIVE_DB=<cache copy>` and `APPIMAGE=<binary>`.

## The cloud model

The worker chooses the model: `MODEL_PROFILE` in `worker/cloudflare.config.ts`
picks a profile in `worker/src/v2/models.ts`.
The rules learned in outages are in `worker/CLAUDE.md`.
