import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  cacheSummary,
  chatConfig,
  clearOpenrouterKey,
  garminDisconnect,
  garminStatus,
  openrouterModels,
  setChatProvider,
  setOpenrouterKey,
  syncNow,
  type ChatProvider,
} from "../lib/api";
import { ErrorNote, PageTitle, Rule } from "../components/ui";
import { UpdateCheck } from "../components/UpdateCheck";
import { since } from "../lib/format";
import { useTheme } from "../lib/useTheme";
import type { Theme } from "../lib/theme";

export function Settings() {
  const qc = useQueryClient();
  const { theme, setTheme } = useTheme();

  const status = useQuery({ queryKey: ["garminStatus"], queryFn: garminStatus });
  const cache = useQuery({ queryKey: ["cacheSummary"], queryFn: cacheSummary });
  const chat = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });

  const [syncError, setSyncError] = useState<string | null>(null);
  const sync = useMutation({
    mutationFn: (full: boolean) => syncNow(full ? 365 : 30, full),
    onMutate: () => setSyncError(null),
    onSuccess: () => qc.invalidateQueries(),
    onError: (e) => setSyncError(e instanceof Error ? e.message : String(e)),
  });

  const disconnect = useMutation({
    mutationFn: garminDisconnect,
    onSuccess: () => qc.invalidateQueries(),
  });

  return (
    <div>
      <PageTitle>Settings</PageTitle>
      <p style={{ fontSize: 15.5, lineHeight: 1.7, color: "var(--mut)", margin: "0 0 46px", maxWidth: "60ch" }}>
        Your Garmin history lives on this machine, in a SQLite file you can
        delete. Questions you ask are sent to the model you choose, with only
        the metrics needed to answer them.
      </p>

      {/* ------------------------------------------------------- appearance */}
      <div className="eyebrow" style={{ marginBottom: 14 }}>
        Appearance
      </div>
      <div style={{ display: "flex", gap: 26, fontSize: 15, marginBottom: 44 }}>
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

      {/* ---------------------------------------------------------- garmin */}
      <div className="eyebrow" style={{ marginBottom: 14 }}>
        Garmin
      </div>
      <div style={{ display: "flex", alignItems: "baseline", gap: 12, fontSize: 15, lineHeight: 1.6 }}>
        <span
          style={{
            width: 5,
            height: 5,
            borderRadius: "50%",
            background: status.data?.connected ? "var(--acc)" : "var(--line)",
            flex: "none",
            transform: "translateY(-3px)",
          }}
        />
        <span>
          {status.data?.connected
            ? "Connected. Tokens are held in your OS keyring, never in the database."
            : "Not connected."}
          {cache.data && (
            <span style={{ color: "var(--mut)" }}>
              {" "}
              {cache.data.activities.toLocaleString()} activities cached
              {cache.data.lastSync ? `, last synced ${since(cache.data.lastSync)}` : ", never synced"}.
            </span>
          )}
        </span>
      </div>
      {cache.data?.path && (
        <div className="mono" style={{ fontSize: 11.5, color: "var(--faint)", marginTop: 10 }}>
          {cache.data.path}
        </div>
      )}

      <div style={{ display: "flex", gap: 26, marginTop: 24, fontSize: 13, color: "var(--mut)" }}>
        <button className="quiet" style={{ color: "var(--mut)" }} onClick={() => sync.mutate(false)} disabled={sync.isPending}>
          {sync.isPending ? "Syncing…" : "Sync recent"}
        </button>
        <button className="quiet" style={{ color: "var(--mut)" }} onClick={() => sync.mutate(true)} disabled={sync.isPending}>
          Full re-sync
        </button>
        <button className="quiet" style={{ color: "var(--acc)" }} onClick={() => disconnect.mutate()}>
          Disconnect Garmin
        </button>
      </div>
      {syncError && <ErrorNote error={syncError} />}
      {sync.data && (
        <div style={{ fontSize: 13, color: "var(--mut)", marginTop: 14 }}>
          {sync.data.activitiesSeen} seen, {sync.data.activitiesWritten} new or
          updated, {sync.data.daysWritten} days of wellness data.
          {sync.data.warnings.length > 0 && (
            <div style={{ color: "var(--faint)", marginTop: 6 }}>
              {sync.data.warnings.length} warning
              {sync.data.warnings.length === 1 ? "" : "s"}: {sync.data.warnings[0]}
            </div>
          )}
        </div>
      )}

      <Rule m="44px 0 20px" />

      {/* ----------------------------------------------------------- model */}
      <div className="eyebrow" style={{ marginBottom: 14 }}>
        Model
      </div>
      <ModelSettings
        current={chat.data}
        onChanged={() => qc.invalidateQueries({ queryKey: ["chatConfig"] })}
      />

      <Rule m="44px 0 20px" />

      {/* ------------------------------------------------------------ data */}
      <div className="eyebrow" style={{ marginBottom: 14 }}>
        What leaves this machine
      </div>
      <div style={{ fontSize: 14.5, lineHeight: 1.7, color: "var(--mut)", maxWidth: "62ch", textWrap: "pretty" }}>
        <p style={{ margin: "0 0 12px" }}>
          Garmin requests go straight from this app to Garmin, using tokens in
          your keyring. Nothing is proxied through a server of ours, because
          there isn't one.
        </p>
        <p style={{ margin: 0 }}>
          Chat sends your question plus whatever a tool returned for it — a
          handful of summary rows, never the whole database and never GPS
          traces. With Ollama selected, not even that leaves the machine.
        </p>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ model --- */

function ModelSettings({
  current,
  onChanged,
}: {
  current: Awaited<ReturnType<typeof chatConfig>> | undefined;
  onChanged: () => void;
}) {
  const [key, setKey] = useState("");
  const [model, setModel] = useState("");
  const [error, setError] = useState<string | null>(null);

  const provider = current?.provider ?? null;

  const models = useQuery({
    queryKey: ["openrouterModels"],
    queryFn: openrouterModels,
    enabled: provider === "openrouter",
    staleTime: 60 * 60_000,
    retry: false,
  });

  const save = useMutation({
    mutationFn: async (p: ChatProvider) => {
      const chosen = model.trim() || current?.model || defaultModel(p, current);
      if (!chosen) throw new Error("Pick a model first.");
      if (p === "openrouter" && key.trim()) await setOpenrouterKey(key.trim());
      await setChatProvider(p, chosen);
    },
    onSuccess: () => {
      setKey("");
      setError(null);
      onChanged();
    },
    onError: (e) => setError(e instanceof Error ? e.message : String(e)),
  });

  const forget = useMutation({
    mutationFn: clearOpenrouterKey,
    onSuccess: onChanged,
  });

  const options = provider === "openrouter" ? (models.data ?? []) : (current?.ollamaModels ?? []);

  return (
    <div>
      {/* Provider choice */}
      {(
        [
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
            note: current?.ollamaReachable
              ? `${current.ollamaModels.length} model${current.ollamaModels.length === 1 ? "" : "s"} pulled locally`
              : "Not running on localhost:11434",
            tag: "Local",
            available: !!current?.ollamaReachable,
          },
        ] satisfies Array<{
          id: ChatProvider;
          name: string;
          note: string;
          tag: string;
          available: boolean;
        }>
      ).map((p) => (
        <button
          key={p.id}
          className="row"
          onClick={() => p.available && save.mutate(p.id)}
          disabled={!p.available}
          style={{ cursor: p.available ? "pointer" : "default" }}
        >
          <span
            style={{
              width: 5,
              height: 5,
              borderRadius: "50%",
              flex: "none",
              background: provider === p.id ? "var(--acc)" : "var(--line)",
              transform: "translateY(-3px)",
            }}
          />
          <span
            style={{
              flex: 1,
              minWidth: 0,
              fontSize: 15,
              color: provider === p.id ? "var(--fg)" : "var(--mut)",
            }}
          >
            {p.name}
            <span style={{ color: "var(--faint)", fontSize: 12.5, marginLeft: 10 }}>
              {p.note}
            </span>
          </span>
          <span style={{ fontSize: 12, color: "var(--faint)", flex: "none" }}>{p.tag}</span>
        </button>
      ))}

      {/* Key */}
      {provider === "openrouter" && (
        <div style={{ marginTop: 30 }}>
          <input
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={current?.hasKey ? "Key stored — type to replace" : "sk-or-v1-…"}
            className="input input-lg"
          />
          <div style={{ fontSize: 11.5, color: "var(--faint)", marginTop: 9 }}>
            Stored in your system keychain, never in the database.
            {current?.hasKey && (
              <>
                {" "}
                <button className="underlined" style={{ fontSize: 11.5 }} onClick={() => forget.mutate()}>
                  Forget it
                </button>
              </>
            )}
          </div>
        </div>
      )}

      {/* Model */}
      {provider && (
        <div style={{ marginTop: 26 }}>
          <div className="eyebrow" style={{ marginBottom: 10 }}>
            Model
          </div>
          <input
            list="model-options"
            value={model || current?.model || ""}
            onChange={(e) => setModel(e.target.value)}
            placeholder={defaultModel(provider, current) ?? "model id"}
            className="input input-lg"
          />
          <datalist id="model-options">
            {options.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
          <div style={{ fontSize: 11.5, color: "var(--faint)", marginTop: 9 }}>
            {provider === "openrouter"
              ? models.isLoading
                ? "Loading the list of tool-capable models…"
                : models.error
                  ? "Couldn't fetch the model list — type an id directly."
                  : `${options.length} OpenRouter model${options.length === 1 ? "" : "s"} support${options.length === 1 ? "s" : ""} tool calls, which this app needs.`
              : options.length
                ? `${options.length} local model${options.length === 1 ? "" : "s"} available. It must support tool calls.`
                : "No local models found."}
          </div>
          <button
            className="cta"
            style={{ marginTop: 22, fontSize: 20 }}
            onClick={() => save.mutate(provider)}
            disabled={save.isPending}
          >
            {save.isPending ? "Saving…" : "Save model settings"}
          </button>
        </div>
      )}

      {error && <ErrorNote error={error} />}

      <Rule m="48px 0 30px" />
      <UpdateCheck />
    </div>
  );
}

/** A sensible starting point, never silently applied — it only prefills. */
function defaultModel(
  p: ChatProvider,
  current: Awaited<ReturnType<typeof chatConfig>> | undefined,
): string | null {
  if (p === "openrouter") return "anthropic/claude-sonnet-4.5";
  return current?.ollamaModels[0] ?? null;
}
