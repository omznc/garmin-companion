# apps/desktop

`garmin-companion-desktop`, binary `garmin-companion`: the gpui-ce desktop app
for Linux, macOS and Windows. This file also covers `apps/login`
(`apps/login/CLAUDE.md` points here). Read the root `CLAUDE.md` first.

## Layout

- `main.rs`: the CLI flags (`--persona`, `--db <copy>`, `--date`, `--screen`,
  `--ask`, `--focus` and others, listed at the top of the file) and the
  subcommand `garmin-companion mcp`. The header list is not complete. A demo (`--db` or `--persona`) saves no
  state. `instance.rs` keeps one copy per user.
- Shell: `shell.rs` (root view), `titlebar.rs`, `sidebar.rs`, `tabs.rs` (at
  most 8 tabs), `statusbar.rs` + `statusmenu.rs`, `chrome.rs` (client-side
  decorations), `command_palette.rs` + `commands.rs` + `fuzzy.rs`.
- Screens: `blocks/*.rs`, one file for each screen. `blocks/mod.rs` is the
  registry. `blocks/layout.rs` + `blocks/edit.rs` are the Today layout and its
  edit mode.
- Data loaders: `data.rs`, `views.rs`, `wellness.rs`, `body.rs`, `training.rs`,
  `places.rs`, `activities.rs`. The date focus: `focus.rs` and `calendar.rs`
  (the picker). `insight.rs`: the insight detail.
- UI kit: `kit.rs`, `ui.rs`, `panel.rs`, `controls.rs`, `charts.rs`,
  `icons.rs`, `markdown.rs`. gpui has no text input, so `field.rs`,
  `editor.rs` and `composer.rs` are our own.
- `panes.rs`: the resizable panes. `window.rs` saves the sizes in
  `window.json`.
- Themes: `theme.rs` (the tokens), `themes.rs`, `appearance.rs`,
  `theme_dialog.rs`, `openvsx.rs` + `vscode.rs` (a VS Code theme to our
  colours).
- `settings.rs`: the Settings screen.
- Coach: `ask.rs` + `ask/` (threads, recents), `cloud.rs`, and the CLI
  providers. See `docs/domains/coach.md`.
- Account and sync: `account.rs`, `autosync.rs`. A job runs on a thread and
  reports over a channel. The UI never waits.
- Updates: `updater.rs` (`latest.json`, minisign, AppImage, macOS bundle,
  NSIS), `updater_pkg.rs` (deb and rpm through `pkexec`), `update_ui.rs`.
- Packaging: `tauri.conf.json` (the version, the bundle, the updater key; the
  Tauri bundler only), `linux/` (the `.desktop` template and the metainfo),
  `icons/`, `assets/` (fonts and icons compiled into the binary). `build.rs`
  takes the version from `$APP_VERSION`, or from `tauri.conf.json`.
- `apps/login` (`garmin-companion-login`): a tao + wry webview, because the
  Garmin sign-in sits behind a Cloudflare check that a real browser engine
  passes. It prints the ticket on stdout. Exit 0 is a ticket, 1 is no ticket,
  2 is bad arguments. It accepts only https URLs on `garmin.com`. The desktop
  app finds it next to its own binary, or at `$GARMIN_COMPANION_LOGIN`.

## UI rules (user decisions)

- **No entrance, fade-in or stagger animation.** Content shows at once
  (`b7c161e`). Reorder, the Add blocks panel and the edit mode also apply at
  once. Smooth scroll and the spinner are the only motion, and both obey
  `motion::reduced()`.
- **Every tab can close, Today too.**
- **Settings looks like the settings of Zed.** A change applies at once,
  with no Save button.
- **Panes resize by drag.** A double click resets a pane. Use breadcrumbs, not
  back links.
- **Show more data, not less.** A block with no data returns `None` and does
  not show.

## Rules

- **No colour literal outside `theme.rs`.**
- **A block draws only from `Ctx`,** never from the state of a different
  screen. A block that shows a series clips it to the date focus.
- **Give each card body its own id scope (`blocks::scoped`).** In gpui, two
  elements with the same id path share one click state, so a click on the
  second one does nothing (`1dbc99d`). A new row id under a parent with no id
  has the same problem.
- **`percentage()` asserts 0..1 in a debug build.** Clamp the value first.
- **Use `scale.rs` `px()` for sizes,** so the interface scale applies.
- **A demo or a test never writes the real data folder.** For a screenshot run
  with `--persona`, also set `XDG_DATA_HOME` to a temp folder. A screenshot run
  once reset the real `window.json` of the user.
- **`mcp.rs`: nothing else prints on stdout.** The loop stays on one thread,
  because `with_as_of` is thread-local.
- **The updater.** Download URLs are https only. After `Outcome::Installed`
  the app quits. `updater_pkg.rs` never writes over files that dpkg or rpm
  own, runs the install only through `pkexec`, and never lets the `PATH` of
  the user choose what runs as root. A `pkexec` exit 126 means that the user
  cancelled: keep the update offer.
- **Keep in sync:** the endpoint and the public key in `updater.rs` and
  `tauri.conf.json`. `StartupWMClass` and `mainBinaryName` match the app id
  `garmin-companion`. The newest metainfo `<release>` matches the
  `tauri.conf.json` version.
- Handlebars also fills the comment lines of the `.desktop` file. Do not name
  a variable in a comment there.

## Platform gotchas

- **Wayland:** gpui-ce has no primary display at start. At a fractional scale
  the output size is wrong, so the window shrinks again after start
  (`window.rs`). The fix skips a fractional-scale window. The login helper sets
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
- **Screenshots:** `scripts/shoot.sh <out.png> [args]` needs the native
  Wayland session. Xvfb and Xwayland give black Vulkan frames.
- **macOS:** the app keeps its own traffic lights, so the title bar has no
  brand. The login helper needs an Edit menu with no Quit, because Quit would
  exit 0 with no ticket.
- **Windows:** NSIS only. WiX refuses the "ć" in the publisher name. An update
  into a folder that the user cannot write asks for UAC.
- **deb/rpm:** the self-update tries `apt-get install` or `dnf install` first,
  and `dpkg` or `rpm` only as a fallback, which does not resolve dependencies.
  Keep the dependency list short. Vulkan is a recommend.
- **gpui-ce** is pinned at 0.2.2 with `gpui_ce_platform` 0.1 (`wayland`, `x11`,
  `font-kit`), the same pin as mbrd-rs.
- **Env hooks** for screenshots, most of them debug only: `GC_OPEN_PALETTE`,
  `GC_OPEN_THEMES`, `GC_SCALE`, `GC_SHOW_SETUP` and others. Run
  `rg -n 'GC_[A-Z_]+' apps/desktop/src` for the full list.
  `GC_REDUCE_MOTION=1` also works in a release build.
