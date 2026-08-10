/**
 * The model provider is down, said once for the whole app.
 *
 * Every screen that asks a model something can fail, and each of them used to
 * fail on its own terms — a line under an activity summary, a note in the chat
 * transcript, silence where the follow-up questions would have been. Three
 * different reports of one broken connection, none of which said the connection
 * was the problem. This is the one place that does.
 *
 * It reports the last request that actually went to the provider rather than
 * probing. A probe would spend a request — real money on a hosted provider — to
 * answer a question the last real request already answered, and it would say
 * "working" for a key with exactly enough credit left for the probe.
 *
 * Lives in the window's top strip, alongside the sync readout. Unlike that one
 * it takes pointer events, because the useful thing to do about a broken
 * provider is go and change it.
 *
 * On a phone there is no strip, and the top of the screen belongs to Android's
 * status bar — so it moves to the bottom and sits where the sync readout sits,
 * for the same reasons and with the same shape. The two never show at once
 * (see the branch below), so sharing the one spot costs nothing.
 */
import { useState } from "react";
import { createPortal } from "react-dom";
import { useQuery } from "@tanstack/react-query";
import { chatHealth } from "../lib/api";
import { IS_MOBILE } from "../lib/platform";
// The router instance rather than `useNavigate`: this renders in the window
// strip, which sits outside the `RouterProvider` so that it survives every
// screen. The singleton navigates the same tree without the hook's context.
import { router } from "../router";
import { CloseIcon, ErrorIcon } from "../lib/icons";
import { CONTROLS_SIDE, CONTROLS_W, STRIP } from "./WindowChrome";
import { useSyncState } from "./SyncBar";

/**
 * How often the verdict is re-read.
 *
 * This costs a lock and a struct copy — no network, no database — so the
 * interval is about how soon a failure should surface, not about what polling
 * costs. Four seconds is faster than anyone can navigate to a screen, notice
 * nothing happened, and wonder why.
 */
const POLL_MS = 4000;

/**
 * The provider's last verdict. Exported because the phone's bottom slot holds
 * one card and the others have to know what's already in it — `UpdateBar` reads
 * this to stand down while a failure is on screen. One query key, so the two
 * subscribe to a single poll rather than each running their own.
 */
export function useChatHealth() {
  return useQuery({
    queryKey: ["chatHealth"],
    queryFn: chatHealth,
    refetchInterval: POLL_MS,
    // The window coming back is the moment to re-check: a request that failed
    // while the app was in the background should be on screen when it returns.
    refetchOnWindowFocus: true,
  });
}

export function AiBar() {
  const sync = useSyncState();
  /** The failure the athlete has already read. Cleared by a new one. */
  const [dismissed, setDismissed] = useState<string | null>(null);

  const health = useChatHealth();

  const broken = health.data && !health.data.ok ? health.data : null;

  // The strip holds one readout. A sync is transient and self-explanatory and a
  // failed provider is neither, so the sync wins the slot while it runs and the
  // failure — which is still true afterwards — comes back when it finishes.
  if (!broken || sync.running) return null;
  if (dismissed === broken.at) return null;

  const bar = (
    <div
      role="status"
      className="ai-bar"
      data-mobile={IS_MOBILE || undefined}
      style={
        IS_MOBILE
          ? // Position and surface are in `styles.css`, beside the sync bar's:
            // both have to clear the tab bar and the gesture inset under it,
            // and `env()` is only readable from a stylesheet.
            undefined
          : {
              position: "fixed",
              top: 0,
              right: CONTROLS_SIDE === "right" ? CONTROLS_W : 12,
              maxWidth: "min(52vw, 560px)",
              height: STRIP,
              display: "flex",
              alignItems: "center",
              gap: 9,
              zIndex: 60,
              // Over the drag strip and the corner resize handle, under the
              // window controls — the same order the sync readout keeps.
              pointerEvents: "auto",
            }
      }
    >
      <ErrorIcon size={14} style={{ flex: "none", color: "var(--warn)" }} aria-hidden />
      <button
        onClick={() => void router.navigate({ to: "/settings" })}
        title={broken.message ?? undefined}
        style={{
          fontSize: "var(--fs-small)",
          color: "var(--fg)",
          cursor: "pointer",
          minWidth: 0,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          textAlign: "left",
        }}
      >
        {/* The provider's own words, not a paraphrase. "Out of credit" and
            "model not found" need different things done about them, and a
            single friendly sentence covering both tells you neither. */}
        {broken.message ?? `${broken.provider} isn't responding`}
      </button>
      <button
        className="quiet"
        onClick={() => setDismissed(broken.at)}
        aria-label="Dismiss"
        title="Dismiss until the next failure"
        style={{ display: "grid", placeItems: "center", color: "var(--faint)", flex: "none" }}
      >
        <CloseIcon size={12} aria-hidden />
      </button>
    </div>
  );

  // Out of the scroller on a phone, for the reason `SyncBar` and `TabBar` are:
  // Android stretches the whole scrolling layer on an overscroll, `position:
  // fixed` inside it included.
  return IS_MOBILE ? createPortal(bar, document.body) : bar;
}
