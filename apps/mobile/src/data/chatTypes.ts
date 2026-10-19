// The shapes of the Ask threads. They match the chat_* commands of the core.
export type Step = { callId: string; label: string; running: boolean; ok: boolean | null };
export type TurnStatus = "running" | "done" | "error" | "stopped";
export type Turn = {
  id: string;
  question: string;
  answer: string;
  steps: Step[];
  status: TurnStatus;
  error: string | null;
};
export type ThreadMeta = {
  sessionId: string;
  title: string;
  updatedAt: string;
  turnCount: number;
  running: boolean;
};
export type Thread = {
  sessionId: string;
  title: string;
  updatedAt: string;
  running: boolean;
  turns: Turn[];
};
export type HistoryRow = { role: "user" | "assistant"; content: string };
// An `ask` event: the JSON of the turn event, with its session and its turn.
export type AskEventPayload = { sessionId: string; turnId: string } & (
  | { type: "delta"; text: string }
  | { type: "tool"; callId: string; label: string; running: boolean; ok: boolean }
  | { type: "done"; sources: string[] }
  | { type: "error"; text: string }
);
