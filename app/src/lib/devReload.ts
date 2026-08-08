/**
 * Ctrl/Cmd+Shift+R reloads the page, in development only.
 *
 * The webview has no browser chrome and no reload shortcut of its own, so when
 * HMR wedges — a bad edit mid-save, a module that swapped in with stale state,
 * a listener registered outside the render tree — the only way back to a clean
 * boot is to quit `tauri dev` and start it again, which also rebuilds Rust.
 *
 * `location.reload()` re-runs the whole entry: fresh module graph from the dev
 * server, fresh React root, fresh query cache. It can't bypass an HTTP cache
 * the way the browser shortcut does, but Vite serves modules from memory in
 * dev, so there's nothing stale to bypass.
 */
export function startDevReload(): () => void {
  if (!import.meta.env.DEV) return () => {};

  const onKeyDown = (e: KeyboardEvent) => {
    if (!e.shiftKey || !(e.ctrlKey || e.metaKey)) return;
    // `code`, not `key`: with Shift held the webview reports "R", and on a
    // non-US layout `key` is whatever that physical key prints.
    if (e.code !== "KeyR") return;
    e.preventDefault();
    location.reload();
  };

  // Capture phase, so a field that swallows keydown can't eat the shortcut.
  window.addEventListener("keydown", onKeyDown, true);
  return () => window.removeEventListener("keydown", onKeyDown, true);
}
