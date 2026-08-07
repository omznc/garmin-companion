import { useEffect, useRef, useState } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { chatConfig, chatSend, type ChatMessage } from "../lib/api";
import { Empty, ErrorNote, Loading, PageTitle, Rule } from "../components/ui";
import { Markdown } from "../components/Markdown";

const SUGGESTIONS = [
  "Am I recovered enough to go hard today?",
  "How much of my last five runs was above Z2?",
  "Compare my last three runs.",
  "Is my cadence improving?",
];

type ChatEvent =
  | { type: "status"; text: string }
  | { type: "delta"; text: string }
  | { type: "done"; sources: string[] }
  | { type: "error"; text: string };

export function Ask() {
  const config = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });

  const [history, setHistory] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [pending, setPending] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const bottom = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottom.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [pending, history.length]);

  if (config.isLoading) return <Loading label="Checking your model settings" />;

  const ready = config.data?.provider && config.data.model;

  async function send(text: string) {
    const question = text.trim();
    if (!question || busy) return;

    const next: ChatMessage[] = [...history, { role: "user", content: question }];
    setHistory(next);
    setDraft("");
    setError(null);
    setStatus(null);
    setPending("");
    setBusy(true);

    // One channel per turn, so a slow previous turn can't write into this one.
    const id = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    let answer = "";
    const sources: string[] = [];

    const unlisten = await listen<ChatEvent>(`chat:${id}`, (e) => {
      const ev = e.payload;
      if (ev.type === "delta") {
        answer += ev.text;
        setPending(answer);
        setStatus(null);
      } else if (ev.type === "status") {
        setStatus(ev.text);
      } else if (ev.type === "done") {
        sources.push(...ev.sources);
      } else if (ev.type === "error") {
        setError(ev.text);
      }
    });

    try {
      await chatSend(id, next);
      if (answer.trim()) {
        setHistory((h) => [...h, { role: "assistant", content: answer, sources }]);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      unlisten();
      setPending(null);
      setStatus(null);
      setBusy(false);
    }
  }

  return (
    <div>
      <PageTitle style={{ marginBottom: 6 }}>Ask</PageTitle>
      <p style={{ fontSize: 13.5, color: "var(--faint)", margin: "0 0 30px" }}>
        {ready
          ? `Reading your cached activities, zones, cadence and recovery through ${config.data!.model}. Only the metrics a question needs are sent.`
          : "No model configured yet."}
      </p>

      {!ready ? (
        <Empty
          title="Choose a model first."
          body="Answers come from a model you point this at — OpenRouter with your own key, or a local Ollama. Neither is configured yet, and nothing is sent anywhere until one is."
          action={
            <Link className="cta" to="/settings">
              Open settings
            </Link>
          }
        />
      ) : (
        <>
          <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
            <input
              className="input-bare"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void send(draft);
                }
              }}
              placeholder="Ask about your training…"
              disabled={busy}
              style={{ flex: 1 }}
            />
            <button
              onClick={() => void send(draft)}
              disabled={busy || !draft.trim()}
              style={{
                fontSize: 12.5,
                color: busy || !draft.trim() ? "var(--faint)" : "var(--acc)",
                whiteSpace: "nowrap",
                cursor: busy || !draft.trim() ? "default" : "pointer",
              }}
            >
              {busy ? "Thinking" : "Send"}
            </button>
          </div>
          <div style={{ fontSize: 11.5, color: "var(--faint)", marginTop: 10 }}>
            {config.data!.provider === "ollama"
              ? "Sent to Ollama on this machine. Nothing leaves your computer."
              : "The question and the metrics it needs are sent to OpenRouter. Raw GPS never is."}
          </div>

          {history.length === 0 && !pending && (
            <div style={{ display: "flex", gap: 22, flexWrap: "wrap", marginTop: 22 }}>
              {SUGGESTIONS.map((s) => (
                <button key={s} className="underlined" style={{ fontSize: 12.5 }} onClick={() => void send(s)}>
                  {s}
                </button>
              ))}
            </div>
          )}

          {error && <ErrorNote error={error} />}

          {(history.length > 0 || pending != null) && (
            <>
              <Rule m="52px 0 20px" />
              <div className="eyebrow" style={{ marginBottom: 34 }}>
                This session
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 44 }}>
                {history.map((m, i) =>
                  m.role === "user" ? (
                    <Question key={i}>{m.content}</Question>
                  ) : (
                    <Answer key={i} text={m.content} sources={m.sources} />
                  ),
                )}
                {pending != null && (
                  <Answer text={pending} status={status ?? "Thinking"} streaming />
                )}
              </div>
            </>
          )}
          <div ref={bottom} />
        </>
      )}
    </div>
  );
}

function Question({ children }: { children: React.ReactNode }) {
  return (
    <div
      className="serif"
      style={{
        fontStyle: "italic",
        fontSize: 23,
        lineHeight: 1.45,
        paddingLeft: 18,
        borderLeft: "1px solid var(--line)",
      }}
    >
      {children}
    </div>
  );
}

function Answer({
  text,
  sources,
  status,
  streaming = false,
}: {
  text: string;
  sources?: string[];
  status?: string;
  streaming?: boolean;
}) {
  return (
    <div>
      {/* `selectable` because the answer is the one thing here worth copying,
          and the app otherwise suppresses selection. */}
      <div
        className="md-body selectable"
        style={{ fontSize: 16, lineHeight: 1.75, maxWidth: "72ch", textWrap: "pretty" }}
      >
        <Markdown>{text}</Markdown>
        {/* A caret while streaming, so a long tool round doesn't read as a hang. */}
        {streaming && (
          <span style={{ color: "var(--faint)", fontSize: 13.5 }}>
            {text ? "▍" : `${status ?? "Thinking"}…`}
          </span>
        )}
      </div>
      {sources && sources.length > 0 && (
        <div style={{ fontSize: 12, color: "var(--faint)", marginTop: 12 }}>
          Read: {dedupe(sources).join(" · ")}
        </div>
      )}
    </div>
  );
}

const dedupe = (xs: string[]) => [...new Set(xs)];
