/**
 * The Settings read-out for the updater. All it does is display state and
 * offer the restart — the work itself runs from `lib/updater`, started at
 * launch, so it keeps going when you navigate away from this screen.
 */
import { useEffect, useState, useSyncExternalStore } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { getUpdateState, runUpdate, subscribe, type UpdateState } from "../lib/updater";
import { SyncIcon, UpdateIcon } from "../lib/icons";

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
          <button className="underlined action" onClick={() => void relaunch()}>
            <UpdateIcon size={13} aria-hidden />
            Restart now
          </button>
        )}
        {(state.at === "current" || state.at === "failed") && (
          <button className="quiet action" onClick={() => void runUpdate()}>
            <SyncIcon size={13} aria-hidden />
            Check again
          </button>
        )}
      </div>

      {state.at === "ready" && state.notes && (
        <div
          className="selectable"
          style={{
            fontSize: "var(--fs-base)",
            lineHeight: 1.65,
            color: "var(--mut)",
            marginTop: 14,
            maxWidth: "58ch",
            whiteSpace: "pre-wrap",
          }}
        >
          {state.notes}
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
      // shortcut rather than a chore you're being asked to complete.
      return `${s.version} installed — starts next time you open the app`;
    case "failed":
      return s.message;
  }
}
