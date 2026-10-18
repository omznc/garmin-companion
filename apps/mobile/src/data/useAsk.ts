// The state of the Ask screen: the turns, and the events of the running turn.
// The core streams `ask` events through the bridge: delta, tool, done, error.
import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { callJson, subscribe } from "./source";

export type Step = { callId: string; label: string; running: boolean; ok: boolean };
export type Turn = { who: "you" | "coach"; text: string; steps: Step[]; error?: string };

type AskEvent =
  | { type: "delta"; text: string }
  | { type: "tool"; callId: string; label: string; running: boolean; ok: boolean }
  | { type: "done"; sources: string[] }
  | { type: "error"; text: string };

// Questions for an empty thread. None of them names a sport.
export const OPENERS = [
  "How did I sleep this week?",
  "Am I recovered enough for a hard day?",
  "Is my resting heart rate moving?",
  "What does my HRV say about this week?",
  "How active was I compared with last month?",
  "How is my weight trending?",
  "How did my last activity go?",
  "What is worth my attention today?",
];

export function openers(): string[] {
  const day = Math.floor(Date.now() / 86_400_000);
  return [0, 3, 6].map((i) => OPENERS[(day + i) % OPENERS.length] ?? "");
}

// The last turn takes the event. Steps are keyed by call id: a call comes twice.
export function applyEvent(turns: Turn[], event: AskEvent): Turn[] {
  const last = turns[turns.length - 1];
  if (!last || last.who !== "coach") return turns;
  let next = last;
  if (event.type === "delta") next = { ...last, text: last.text + event.text };
  else if (event.type === "error") next = { ...last, error: event.text };
  else if (event.type === "tool") {
    const step: Step = {
      callId: event.callId,
      label: event.label,
      running: event.running,
      ok: event.ok,
    };
    const known = last.steps.some((s) => s.callId === step.callId);
    next = {
      ...last,
      steps: known
        ? last.steps.map((s) => (s.callId === step.callId ? step : s))
        : [...last.steps, step],
    };
  }
  return [...turns.slice(0, -1), next];
}

export function historyOf(turns: Turn[]) {
  return turns
    .filter((t) => t.text.trim())
    .map((t) => ({ role: t.who === "you" ? "user" : "assistant", content: t.text }));
}

export function useAsk() {
  const [turns, setTurns] = useState<Turn[]>([]);
  const [busy, setBusy] = useState(false);
  const turnsRef = useRef<Turn[]>([]);
  turnsRef.current = turns;

  useEffect(
    () =>
      subscribe((event) => {
        if (event.kind !== "ask") return;
        const e = event.payload as AskEvent;
        setTurns((t) => applyEvent(t, e));
        if (e.type === "done" || e.type === "error") setBusy(false);
      }),
    [],
  );

  const send = useCallback(async (text: string) => {
    const question = text.trim();
    if (!question) return;
    const asked: Turn[] = [...turnsRef.current, { who: "you", text: question, steps: [] }];
    setTurns([...asked, { who: "coach", text: "", steps: [] }]);
    setBusy(true);
    try {
      await callJson("ask_send", { history: historyOf(asked) });
    } catch (error) {
      setTurns((t) => applyEvent(t, { type: "error", text: (error as Error).message }));
      setBusy(false);
    }
  }, []);

  const stop = useCallback(() => {
    void callJson("ask_stop").catch(() => {});
  }, []);

  return { turns, busy, send, stop };
}

export type CoachConfig = {
  provider: string | null;
  model: string | null;
  hasKey: boolean;
};

// Whether the coach can answer: an OpenRouter key, or a local model (not on a phone).
export function needsSetup(config: CoachConfig | undefined): boolean {
  if (!config) return false;
  return !(config.provider === "openrouter" && config.hasKey) && config.provider !== "ollama";
}

export async function saveKey(key: string): Promise<void> {
  await callJson("ask_save_config", { provider: "openrouter", key });
}

export const useCoachConfig = () =>
  useQuery({
    queryKey: ["askConfig"],
    queryFn: () => callJson<CoachConfig>("ask_config"),
    retry: false,
  });
