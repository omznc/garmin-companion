/**
 * First-run setup: connect Garmin, choose a model, set preferences.
 *
 * The design draws step 1 as an email-and-password form. This app can't
 * implement that honestly: Garmin's sign-in sits behind Cloudflare's TLS
 * fingerprinting, which rejects requests from anything that isn't a browser, so
 * an in-app credential form would have to either fail or ship the password
 * somewhere it shouldn't go. Instead the same step opens Garmin's own sign-in
 * page in a real browser window — the credentials never touch this app — and
 * keeps the design's layout, typography and rhythm.
 */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import {
  chatConfig,
  garminImportTokens,
  garminLogin,
  garminStatus,
  prepareCloudChat,
  setChatProvider,
  CLOUD_MODEL,
  DEFAULT_MODEL,
  setOpenrouterKey,
  type ChatProvider,
} from "../lib/api";
import { runSync } from "../lib/syncProgress";
import { ArrowRight, BackLink, ErrorNote } from "../components/ui";
import { ScrollFade } from "../components/ScrollFade";
import { useTheme } from "../lib/useTheme";
import type { Theme } from "../lib/theme";

export function Setup({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState(1);

  return (
    <div
      style={{
        minHeight: "100vh",
        background: "var(--bg)",
        color: "var(--fg)",
        display: "flex",
        justifyContent: "center",
        padding: "0 56px",
      }}
    >
      {/* Full width here: setup has no nav to stop short of. */}
      <ScrollFade left="0" />
      <div style={{ width: "100%", maxWidth: step === 1 ? 460 : 520, padding: "150px 0 120px" }}>
        {step === 1 && <StepGarmin onNext={() => setStep(2)} />}
        {step === 2 && <StepModel onBack={() => setStep(1)} onNext={() => setStep(3)} />}
        {step === 3 && <StepPreferences onBack={() => setStep(2)} onDone={onDone} />}
      </div>
    </div>
  );
}

function StepHeader({
  step,
  onBack,
}: {
  step: number;
  onBack?: () => void;
}) {
  return (
    <div style={{ display: "flex", alignItems: "baseline", gap: 16, marginBottom: 52 }}>
      {onBack && <BackLink onClick={onBack}>Back</BackLink>}
      <div className="eyebrow-lg">Step {step} of 3</div>
    </div>
  );
}

/* ------------------------------------------------------------------ step 1 --- */

function StepGarmin({ onNext }: { onNext: () => void }) {
  const qc = useQueryClient();
  const status = useQuery({ queryKey: ["garminStatus"], queryFn: garminStatus });
  const [progress, setProgress] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const p = listen<string>("garmin-login", (e) => setProgress(e.payload));
    return () => {
      void p.then((un) => un());
    };
  }, []);

  const login = useMutation({
    mutationFn: garminLogin,
    onMutate: () => setError(null),
    onSuccess: async () => {
      await qc.invalidateQueries();
      onNext();
    },
    onError: (e) => {
      setProgress(null);
      setError(e instanceof Error ? e.message : String(e));
    },
  });

  const importTokens = useMutation({
    mutationFn: () => garminImportTokens(),
    onMutate: () => setError(null),
    onSuccess: async () => {
      await qc.invalidateQueries();
      onNext();
    },
    onError: (e) => setError(e instanceof Error ? e.message : String(e)),
  });

  const connected = status.data?.connected;
  const importable = status.data?.importableTokenPath;
  const busy = login.isPending || importTokens.isPending;

  return (
    <div>
      <StepHeader step={1} />
      <h1 className="serif" style={{ fontSize: 38, lineHeight: 1.12, margin: "0 0 12px" }}>
        {connected ? "Garmin is connected." : "Connect Garmin."}
      </h1>
      <p className="lede" style={{ margin: "0 0 36px", maxWidth: "52ch" }}>
        {connected
          ? "A session is already stored in your keyring. You can carry on, or sign in again to replace it."
          : "You'll sign in on Garmin's own page, in its own window. This app never sees your password — it only keeps the token Garmin issues afterwards, in your OS keyring."}
      </p>

      {importable && !connected && (
        <div
          style={{
            fontSize: "var(--fs-base)",
            lineHeight: 1.65,
            color: "var(--mut)",
            paddingLeft: 16,
            borderLeft: "1px solid var(--acc)",
            marginBottom: 36,
            maxWidth: "52ch",
          }}
        >
          Found an existing token file at{" "}
          <span className="mono" style={{ fontSize: "var(--fs-caption)" }}>
            {importable}
          </span>
          . Importing it skips the sign-in entirely.
          <div style={{ marginTop: 12 }}>
            <button
              className="underlined"
              style={{ fontSize: "var(--fs-small)" }}
              onClick={() => importTokens.mutate()}
              disabled={busy}
            >
              Import those tokens
            </button>
          </div>
        </div>
      )}

      {progress && (
        <div style={{ fontSize: "var(--fs-base)", color: "var(--mut)", marginBottom: 24 }}>{progress}</div>
      )}
      {error && <ErrorNote error={error} />}

      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 24,
          marginTop: 40,
        }}
      >
        {connected ? (
          <button className="cta" onClick={onNext}>
            Continue
            <ArrowRight />
          </button>
        ) : (
          <button className="cta" onClick={() => login.mutate()} disabled={busy}>
            {login.isPending ? "Waiting for Garmin…" : "Sign in with Garmin"}
            <ArrowRight />
          </button>
        )}
        {connected && (
          <button className="quiet" style={{ fontSize: "var(--fs-small)" }} onClick={() => login.mutate()} disabled={busy}>
            Sign in again
          </button>
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ step 2 --- */

function StepModel({ onBack, onNext }: { onBack: () => void; onNext: () => void }) {
  const qc = useQueryClient();
  const config = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });
  /**
   * Preselected rather than blank. The hosted coach needs nothing from anyone —
   * no key, no account, no model pulled — so leaving this unset would be asking
   * a question whose answer is already right for almost everyone. The other two
   * are one click away and the copy below says what each one means.
   */
  const [picked, setPicked] = useState<ChatProvider | null>("cloud");
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: async () => {
      if (!picked) throw new Error("Pick a provider.");
      // No model field on first run: the hosted coach has exactly one, and
      // OpenRouter gets the default. Choosing between 300 models is a Settings
      // job, not something to put between someone and their own data.
      const chosen =
        picked === "cloud"
          ? CLOUD_MODEL
          : picked === "openrouter"
            ? DEFAULT_MODEL
            : (config.data?.ollamaModels[0] ?? "");
      if (!chosen) throw new Error("No model available — pull one in Ollama first.");
      if (picked === "openrouter") {
        if (!key.trim() && !config.data?.hasKey) throw new Error("An OpenRouter key is needed.");
        if (key.trim()) await setOpenrouterKey(key.trim());
      }
      await setChatProvider(picked, chosen);
      // The id the hosted coach counts against its budget, fetched here so the
      // first question is a question rather than an introduction. Swallowed on
      // purpose: a coach that can't be reached during setup is not a reason to
      // hold someone on step 2, and asking again is what the first question
      // already does.
      if (picked === "cloud") await prepareCloudChat().catch(() => {});
    },
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["chatConfig"] });
      onNext();
    },
    onError: (e) => setError(e instanceof Error ? e.message : String(e)),
  });

  const providers = [
    {
      id: "cloud" as const,
      name: "Built-in coach",
      note: "Nothing to set up",
      tag: "Hosted",
      available: true,
    },
    {
      id: "openrouter" as const,
      name: "OpenRouter",
      note: "Any hosted model, one key",
      tag: "Hosted",
      available: true,
    },
    {
      id: "ollama" as const,
      name: "Ollama",
      note: config.data?.ollamaReachable
        ? `${config.data.ollamaModels.length} pulled locally`
        : "Not running on localhost:11434",
      tag: "Local",
      available: !!config.data?.ollamaReachable,
    },
  ];

  return (
    <div>
      <StepHeader step={2} onBack={onBack} />
      <h1 className="serif" style={{ fontSize: 38, lineHeight: 1.12, margin: "0 0 12px" }}>
        Which model should read your data?
      </h1>
      <p className="lede" style={{ margin: "0 0 32px", maxWidth: "52ch" }}>
        A local model never sends anything off this machine. A hosted one
        receives only the metrics a question needs — never your whole history,
        and never GPS.
      </p>

      {providers.map((p) => (
        <button
          key={p.id}
          className="row"
          onClick={() => p.available && setPicked(p.id)}
          disabled={!p.available}
          style={{ cursor: p.available ? "pointer" : "default" }}
        >
          <span
            style={{
              width: 5,
              height: 5,
              borderRadius: "50%",
              flex: "none",
              background: picked === p.id ? "var(--acc)" : "var(--line)",
              transform: "translateY(-3px)",
            }}
          />
          <span
            style={{
              flex: 1,
              minWidth: 0,
              fontSize: "var(--fs-md)",
              color: picked === p.id ? "var(--fg)" : "var(--mut)",
            }}
          >
            {p.name}
            <span style={{ color: "var(--faint)", fontSize: "var(--fs-small)", marginLeft: 10 }}>{p.note}</span>
          </span>
          <span style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", flex: "none" }}>{p.tag}</span>
        </button>
      ))}

      {picked === "openrouter" && (
        <div style={{ marginTop: 30 }}>
          <input
            type="password"
            className="input input-lg"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={config.data?.hasKey ? "Key already stored" : "sk-or-v1-…"}
          />
          <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 9 }}>
            Stored in your system keychain, never in the database.
          </div>
        </div>
      )}

      {picked && (
        <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 26, lineHeight: 1.6 }}>
          {picked === "cloud"
            ? "Questions go to a small server this project runs, which forwards them to a model and pays for the answer. It sees the metrics a question needs — heart rates, sleep, weight — and keeps none of them. Settings can move you to your own key or a local model at any point."
            : picked === "openrouter"
              ? `Starts on ${DEFAULT_MODEL} — cheap, fast, and it calls tools, which every answer here needs. Settings has the full list.`
              : "Uses the first model you've pulled. It must support tool calls. Settings has the rest."}
        </div>
      )}

      {error && <ErrorNote error={error} />}

      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 24,
          marginTop: 40,
        }}
      >
        <button className="cta" onClick={() => save.mutate()} disabled={!picked || save.isPending}>
          Continue
          <ArrowRight />
        </button>
        <button className="quiet" style={{ fontSize: "var(--fs-small)" }} onClick={onNext}>
          {picked ? "" : "Skip — set this up later"}
        </button>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ step 3 --- */

function StepPreferences({ onBack, onDone }: { onBack: () => void; onDone: () => void }) {
  const qc = useQueryClient();
  const { theme, setTheme } = useTheme();
  const [error, setError] = useState<string | null>(null);

  const sync = useMutation({
    // Full, not a 60-day sample: this is the one chance to pull the whole
    // history, and `sync_all` widens the window to however far the watch goes
    // back. It's slow, which is what the progress bar is for.
    mutationFn: () => runSync(365, true),
    onMutate: () => setError(null),
    onSuccess: async () => {
      await qc.invalidateQueries();
      onDone();
    },
    onError: (e) => setError(e instanceof Error ? e.message : String(e)),
  });

  return (
    <div>
      <StepHeader step={3} onBack={onBack} />
      <h1 className="serif" style={{ fontSize: 38, lineHeight: 1.12, margin: "0 0 36px" }}>
        A few preferences.
      </h1>

      <div className="eyebrow" style={{ marginBottom: 14 }}>
        Appearance
      </div>
      <div style={{ display: "flex", gap: 26, fontSize: "var(--fs-md)", marginBottom: 40 }}>
        {(["light", "dark", "system"] as Theme[]).map((t) => (
          <button
            key={t}
            onClick={() => setTheme(t)}
            style={{
              cursor: "pointer",
              color: theme === t ? "var(--fg)" : "var(--mut)",
              borderBottom: `1px solid ${theme === t ? "var(--acc)" : "transparent"}`,
              paddingBottom: 3,
              textTransform: "capitalize",
            }}
          >
            {t === "system" ? "Match system" : t}
          </button>
        ))}
      </div>

      <div className="eyebrow" style={{ marginBottom: 6 }}>
        What this app reads
      </div>
      {[
        ["Activities", "Every session Garmin has, with its HR zone breakdown", "On"],
        ["Sleep", "Duration and Garmin's sleep score", "On"],
        ["Heart rate & HRV", "Resting HR and overnight variability", "On"],
        ["Body battery & stress", "Garmin's derived daily metrics", "On"],
        ["GPS traces", "Not stored — only fetched when you open one activity", "Off"],
      ].map(([label, note, state]) => (
        <div key={label} className="row-static" style={{ alignItems: "center" }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: "var(--fs-md)" }}>{label}</div>
            <div style={{ fontSize: "var(--fs-small)", color: "var(--faint)", marginTop: 4 }}>{note}</div>
          </div>
          <div
            style={{
              fontSize: "var(--fs-small)",
              color: state === "On" ? "var(--fg)" : "var(--faint)",
              width: 44,
              textAlign: "right",
              flex: "none",
            }}
          >
            {state}
          </div>
        </div>
      ))}
      {/* These describe what the sync actually does rather than offering
          switches that wouldn't be wired to anything. */}
      <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 14, maxWidth: "52ch", lineHeight: 1.6 }}>
        This is what the sync fetches today, not a set of toggles — per-category
        opt-outs aren't implemented yet, and a switch that did nothing would be
        worse than saying so.
      </div>

      {error && <ErrorNote error={error} />}

      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 24,
          marginTop: 44,
        }}
      >
        <button className="cta" style={{ fontSize: 25 }} onClick={() => sync.mutate()} disabled={sync.isPending}>
          {sync.isPending ? "Importing your history…" : "Open Companion"}
          <ArrowRight />
        </button>
        <button className="quiet" style={{ fontSize: "var(--fs-small)" }} onClick={onDone}>
          Skip the first sync
        </button>
      </div>
    </div>
  );
}
