import { useSyncExternalStore } from "react";
import { useColorScheme } from "react-native";
import { Snackbar } from "./Snackbar";
import { currentNotice, hideSnackbar, subscribeNotice } from "./snackbar-store";

// iOS: a dark bar in the light scheme, a light bar in the dark scheme.
export function SnackbarHost() {
  const notice = useSyncExternalStore(subscribeNotice, currentNotice);
  const dark = useColorScheme() === "dark";
  return (
    <Snackbar
      notice={notice}
      onHide={hideSnackbar}
      colors={
        dark ? { background: "#e5e5ea", ink: "#1c1c1e" } : { background: "#2c2c2e", ink: "#f2f2f7" }
      }
      radius={14}
    />
  );
}
