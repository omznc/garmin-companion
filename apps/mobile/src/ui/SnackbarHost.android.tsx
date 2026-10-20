import { useSyncExternalStore } from "react";
import { useMd } from "../theme/md";
import { Snackbar } from "./Snackbar";
import { currentNotice, hideSnackbar, subscribeNotice } from "./snackbar-store";

// The Material 3 snackbar: the inverse colours, 4 dp corners.
export function SnackbarHost() {
  const notice = useSyncExternalStore(subscribeNotice, currentNotice);
  const md = useMd();
  return (
    <Snackbar
      notice={notice}
      onHide={hideSnackbar}
      colors={{ background: md.inverseSurface, ink: md.inverseOnSurface }}
      radius={4}
    />
  );
}
