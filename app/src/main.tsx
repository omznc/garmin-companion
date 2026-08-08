import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { IconContext } from "@phosphor-icons/react/dist/lib/context";
import { App } from "./App";
import { blockNativeContextMenu } from "./components/ContextMenu";
import { startAppearance } from "./lib/theme";
import { startAutoUpdate } from "./lib/updater";
import { startDayRollover } from "./lib/dayRollover";
import { startBackgroundRefresh } from "./lib/backgroundRefresh";
import { startDevReload } from "./lib/devReload";
import { startBack } from "./lib/back";
import { applyPlatform } from "./lib/platform";
import { setEdgeToEdge } from "./lib/android";
import "./styles.css";

// Before anything renders: the window chrome and the window's corner radius are
// both keyed on the platform in CSS, and both should be right on the first
// paint rather than corrected a frame later.
applyPlatform();

// Sign-in drops the window out of edge-to-edge for the duration of Garmin's
// page. The activity puts it back on its own — this is only the shortcut, taken
// on the same frame as the returning page rather than up to a poll later, and
// it says the same thing the watcher would. No-op off Android.
setEdgeToEdge(true);

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Cache reads are local SQLite queries, so refetching is cheap — but
      // there's no server pushing changes either, so nothing goes stale on its
      // own. Refetch on focus, not on an interval.
      staleTime: 30_000,
      refetchOnWindowFocus: true,
      retry: 1,
    },
  },
});

// Before the first render, not in an effect after it: the window would
// otherwise paint one frame in the default palette before switching.
startAppearance();

// Outside the render tree on purpose: StrictMode double-invokes effects in
// development, and an update check is not something to run twice.
startAutoUpdate();

// Same reasoning: one timer for the life of the process, not one per mount.
startDayRollover(queryClient);

// And one listener: the desktop build syncs on its own while the window sits
// open, and the screens have no other way to hear about it.
startBackgroundRefresh(queryClient);

// And one listener, for the same reason. The app draws its own right-click
// menus; the webview's belongs to a browser.
blockNativeContextMenu();

// And one more, in dev only: the webview has no reload shortcut of its own.
startDevReload();

// Before the first render, so the very first back press has somewhere to go —
// the activity asks for this by name and treats its absence as "not handled".
startBack();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      {/* One weight and one size for the whole app. Duotone is the only weight
          that survives this palette: the hairlines here are 6–13% opacity, and
          a solid `fill` icon next to them lands like a blot, while `thin` at
          15px disappears. `currentColor` means an icon inherits whatever the
          text beside it is doing — hover, accent, faint — without being told. */}
      <IconContext.Provider value={{ weight: "duotone", size: 16, color: "currentColor" }}>
        <App />
      </IconContext.Provider>
    </QueryClientProvider>
  </React.StrictMode>,
);
