import { useEffect, useState } from "react";
import { RouterProvider } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { garminStatus, scheduleNudges } from "./lib/api";
import { router } from "./router";
import { Setup } from "./screens/Setup";
import { useTypeface } from "./lib/useTypeface";
import { IS_MOBILE } from "./lib/platform";
import { WindowChrome } from "./components/WindowChrome";
import { SyncBar } from "./components/SyncBar";
import { AiBar } from "./components/AiBar";

const SETUP_DONE = "garmin-companion:setup-complete";

export function App() {
  // The face is applied here rather than per-screen, so the very first paint
  // already has it. The palette isn't: `lib/theme` writes it to the document at
  // import, which is earlier still, and subscribing to it up here would re-run
  // the whole tree on a change that CSS variables handle on their own.
  useTypeface();

  const status = useQuery({ queryKey: ["garminStatus"], queryFn: garminStatus });
  const [dismissed, setDismissed] = useState(() => localStorage.getItem(SETUP_DONE) === "1");

  // A disconnect from Settings should drop back into setup on next launch, so
  // the flag tracks the connection rather than being set once and forgotten.
  useEffect(() => {
    if (status.data && !status.data.connected) {
      localStorage.removeItem(SETUP_DONE);
      setDismissed(false);
    }
  }, [status.data?.connected]);

  // Re-lay the coach's notification plan, once there is a connected account to
  // have data for.
  //
  // Launch is one of only two moments this can happen — the other is after a
  // sync — because nothing evaluates the rules while the app is closed. So each
  // launch replaces the queued days with a plan built from what the cache knows
  // now, and the phone keeps nudging for a few days after this without the app
  // ever running again.
  useEffect(() => {
    if (!status.data?.connected) return;
    void scheduleNudges().catch(() => {
      // A refused notification permission is not worth interrupting a launch
      // over. The same nudge is on Today either way.
    });
  }, [status.data?.connected]);

  // The window has no decorations, so the chrome renders on every branch below
  // — including the loading one, where an undraggable window would look hung.
  const needsSetup = !status.data?.connected || !dismissed;

  return (
    <>
      {/* Nothing to draw on a phone: the window is the screen, it has no title
          bar, and it cannot be moved, resized or closed from inside the app.
          The status bar above it belongs to Android. */}
      {!IS_MOBILE && <WindowChrome />}
      {/* Above the router, so a sync started in Settings keeps narrating
          itself while you go and read Today. It draws into the window strip
          the chrome above reserves, so the two belong side by side here. */}
      <SyncBar />
      {/* Beside it, and only once there is a router to send you to Settings.
          A broken model provider is worth saying once for the whole app rather
          than three times in three different words on three screens. */}
      {!needsSetup && <AiBar />}
      {status.isLoading ? null : needsSetup ? (
        <Setup
          onDone={() => {
            localStorage.setItem(SETUP_DONE, "1");
            setDismissed(true);
          }}
        />
      ) : (
        <RouterProvider router={router} />
      )}
    </>
  );
}
