/**
 * The updater surface: a version line, a manual check, and a download that
 * ends in a restart.
 *
 * Deliberately not automatic. The app is a coaching notebook you open to look
 * something up, and having it swap itself out underneath you mid-question
 * would be worse than being a version behind. It checks once on mount so you
 * find out an update exists, and then waits to be told.
 */
import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type Phase =
  | { at: "checking" }
  | { at: "current" }
  | { at: "available"; update: Update }
  | { at: "downloading"; pct: number | null }
  | { at: "ready" }
  | { at: "failed"; message: string };

export function UpdateCheck() {
  const [version, setVersion] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>({ at: "checking" });

  useEffect(() => {
    void getVersion().then(setVersion);
    void runCheck();
  }, []);

  async function runCheck() {
    setPhase({ at: "checking" });
    try {
      const update = await check();
      setPhase(update ? { at: "available", update } : { at: "current" });
    } catch (e) {
      setPhase({ at: "failed", message: describe(e) });
    }
  }

  async function install(update: Update) {
    setPhase({ at: "downloading", pct: null });
    try {
      // The event stream reports a total up front only when the server sends a
      // content-length, so the bar has to cope with never knowing the size.
      let total = 0;
      let got = 0;
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        else if (e.event === "Progress") {
          got += e.data.chunkLength;
          setPhase({ at: "downloading", pct: total ? got / total : null });
        } else if (e.event === "Finished") setPhase({ at: "ready" });
      });
      setPhase({ at: "ready" });
    } catch (e) {
      setPhase({ at: "failed", message: describe(e) });
    }
  }

  return (
    <div style={{ marginBottom: 44 }}>
      <div className="eyebrow" style={{ marginBottom: 14 }}>
        Version
      </div>
      <div style={{ fontSize: 15, lineHeight: 1.6 }}>
        <span className="mono" style={{ fontSize: 13.5 }}>
          {version ?? "—"}
        </span>
        <span style={{ color: "var(--mut)" }}> · {caption(phase)}</span>
      </div>

      {phase.at === "downloading" && phase.pct != null && (
        <div className="bar" style={{ marginTop: 12, maxWidth: 260 }}>
          <span style={{ width: `${Math.round(phase.pct * 100)}%` }} />
        </div>
      )}

      <div style={{ display: "flex", gap: 22, marginTop: 14, fontSize: 13 }}>
        {phase.at === "available" && (
          <button className="underlined" onClick={() => void install(phase.update)}>
            Download {phase.update.version}
          </button>
        )}
        {phase.at === "ready" && (
          <button className="underlined" onClick={() => void relaunch()}>
            Restart to finish
          </button>
        )}
        {(phase.at === "current" || phase.at === "failed") && (
          <button className="quiet" onClick={() => void runCheck()}>
            Check again
          </button>
        )}
      </div>

      {phase.at === "available" && phase.update.body && (
        <div
          className="selectable"
          style={{
            fontSize: 13.5,
            lineHeight: 1.65,
            color: "var(--mut)",
            marginTop: 14,
            maxWidth: "58ch",
            whiteSpace: "pre-wrap",
          }}
        >
          {phase.update.body}
        </div>
      )}
    </div>
  );
}

function caption(p: Phase): string {
  switch (p.at) {
    case "checking":
      return "checking for updates…";
    case "current":
      return "up to date";
    case "available":
      return `${p.update.version} is available`;
    case "downloading":
      return p.pct == null ? "downloading…" : `downloading ${Math.round(p.pct * 100)}%`;
    case "ready":
      return "update installed";
    case "failed":
      return p.message;
  }
}

/** A failed check is worth a line, not a red box — being offline is normal. */
function describe(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  return /network|dns|connect|timed? out|resolve/i.test(raw)
    ? "couldn't reach the update server"
    : `update check failed — ${raw}`;
}
