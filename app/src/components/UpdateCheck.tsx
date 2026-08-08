/**
 * The Settings read-out for the updater. All it does is display state and
 * offer the last step — the work itself runs from `lib/updater`, started at
 * launch, so it keeps going when you navigate away from this screen.
 *
 * What that last step is differs by platform and this file doesn't decide:
 * `apply()` restarts a desktop build into a version already installed, and
 * hands an Android one to the system installer. Only the wording below knows.
 */
import { useEffect, useState, useSyncExternalStore } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { apply, getUpdateState, runUpdate, subscribe, type UpdateState } from "../lib/updater";
import { IS_MOBILE } from "../lib/platform";
import { SyncIcon, UpdateIcon } from "../lib/icons";
import { Markdown } from "./Markdown";

export function useUpdateState(): UpdateState {
  return useSyncExternalStore(subscribe, getUpdateState);
}

export function UpdateCheck() {
  const [version, setVersion] = useState<string | null>(null);
  const state = useUpdateState();

  useEffect(() => {
    void getVersion().then(setVersion);
    // Opening Settings is as good a moment as any to retry a check that
    // failed earlier, or to run the first one if the launch check hasn't
    // fired yet. `runUpdate` is idempotent.
    void runUpdate();
  }, []);

  return (
    // No heading of its own: Settings puts this in a section like any other,
    // and it used to render one from inside the model settings, which stacked
    // "Version" under "Model" as if it were part of choosing one.
    <div>
      <div style={{ fontSize: "var(--fs-md)", lineHeight: 1.6 }}>
        <span className="mono" style={{ fontSize: "var(--fs-base)" }}>
          {version ?? "—"}
        </span>
        <span style={{ color: "var(--mut)" }}> · {caption(state)}</span>
      </div>

      {state.at === "downloading" && state.pct != null && (
        <div className="bar bar-live" style={{ marginTop: 12, maxWidth: 260 }}>
          <span style={{ transform: `scaleX(${state.pct})` }} />
        </div>
      )}

      <div style={{ display: "flex", gap: 22, marginTop: 14, fontSize: "var(--fs-small)" }}>
        {state.at === "ready" && (
          <button className="underlined action" onClick={apply}>
            <UpdateIcon size={13} aria-hidden />
            {IS_MOBILE ? "Install now" : "Restart now"}
          </button>
        )}
        {/* Same button, second time round. Granting the permission is a trip to
            a settings screen with no way to notify us it happened, so coming
            back and pressing this is the only signal there is. */}
        {state.at === "blocked" && (
          <button className="underlined action" onClick={apply}>
            <UpdateIcon size={13} aria-hidden />
            Try again
          </button>
        )}
        {(state.at === "current" || state.at === "failed") && (
          <button className="quiet action" onClick={() => void runUpdate()}>
            <SyncIcon size={13} aria-hidden />
            Check again
          </button>
        )}
      </div>

      {/* Release notes are written as Markdown on GitHub, so they're rendered
          as Markdown here — as prose with headings and bullets, the same way an
          answer on Ask is, rather than as the asterisks and hyphens someone
          typed to produce them. */}
      {state.at === "ready" && state.notes && (
        <div
          className="md-body selectable"
          style={{
            fontSize: "var(--fs-base)",
            lineHeight: 1.65,
            color: "var(--mut)",
            marginTop: 14,
            maxWidth: "58ch",
          }}
        >
          <Markdown>{state.notes}</Markdown>
        </div>
      )}
    </div>
  );
}

function caption(s: UpdateState): string {
  switch (s.at) {
    case "idle":
    case "checking":
      return "checking for updates…";
    case "current":
      return "up to date";
    case "downloading":
      return s.pct == null
        ? `downloading ${s.version}…`
        : `downloading ${s.version} — ${Math.round(s.pct * 100)}%`;
    case "ready":
      // States what already happened and what's left, so the button reads as a
      // shortcut rather than a chore you're being asked to complete. The two
      // platforms have got to different places by here and saying so matters:
      // a desktop build is *already* on the new version and only the running
      // process is stale, where an Android one has the new version sitting in a
      // file and needs permission to become it.
      return IS_MOBILE
        ? `${s.version} downloaded — Android will ask before replacing this app`
        : `${s.version} installed — starts next time you open the app`;
    case "blocked":
      // Names the setting rather than the permission. "Install unknown apps" is
      // the string on the screen they were just sent to, and the one they have
      // to find in a list of every app on the phone.
      return `${s.version} is ready — turn on "Install unknown apps" for this app, then come back`;
    case "failed":
      return s.message;
  }
}
