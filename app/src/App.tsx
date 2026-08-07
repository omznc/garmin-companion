import { useEffect, useState } from "react";
import { RouterProvider } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { garminStatus } from "./lib/api";
import { router } from "./router";
import { Setup } from "./screens/Setup";
import { useTheme } from "./lib/useTheme";
import { WindowChrome } from "./components/WindowChrome";

const SETUP_DONE = "garmin-companion:setup-complete";

export function App() {
  // Applied here rather than per-screen so the very first paint is already in
  // the right palette.
  useTheme();

  const status = useQuery({ queryKey: ["garminStatus"], queryFn: garminStatus });
  const [dismissed, setDismissed] = useState(
    () => localStorage.getItem(SETUP_DONE) === "1",
  );

  // A disconnect from Settings should drop back into setup on next launch, so
  // the flag tracks the connection rather than being set once and forgotten.
  useEffect(() => {
    if (status.data && !status.data.connected) {
      localStorage.removeItem(SETUP_DONE);
      setDismissed(false);
    }
  }, [status.data?.connected]);

  // The window has no decorations, so the chrome renders on every branch below
  // — including the loading one, where an undraggable window would look hung.
  const needsSetup = !status.data?.connected || !dismissed;

  return (
    <>
      <WindowChrome />
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
