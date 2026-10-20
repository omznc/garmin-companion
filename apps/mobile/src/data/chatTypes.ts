// The shapes of the Ask threads. They match the chat_* commands of the core.
// A step is a tool call, or a note: what the model said between two tool rounds
// (`label` is then the text). A stored step with no `kind` is a tool.
export type Step = {
  kind?: "tool" | "note";
  callId: string;
  label: string;
  running: boolean;
  ok: boolean | null;
};
// A workout that the coach proposed. It matches `WorkoutDraft` of the core.
export type DraftSport = "running" | "cycling" | "cardio" | "strength_training";
export type DraftKind = "warmup" | "interval" | "recovery" | "rest" | "cooldown";
export type DraftEnd =
  | { type: "time"; seconds: number }
  | { type: "distance"; metres: number }
  | { type: "lap_button" };
export type DraftTarget =
  | { type: "none" }
  | { type: "hr_zone"; zone: number }
  | { type: "bpm"; low: number; high: number };
export type DraftExec = { kind: DraftKind; end: DraftEnd; target?: DraftTarget; note?: string };
export type DraftStep =
  | ({ type: "exec" } & DraftExec)
  | { type: "repeat"; times: number; steps: DraftExec[] };
export type WorkoutDraft = {
  name: string;
  sport: DraftSport;
  description?: string;
  steps: DraftStep[];
};
// `savedWorkoutId` is set once the person saved the draft to Garmin.
export type DraftEntry = { callId: string; draft: WorkoutDraft; savedWorkoutId: number | null };
export type TurnStatus = "running" | "done" | "error" | "stopped";
export type Turn = {
  id: string;
  question: string;
  answer: string;
  steps: Step[];
  status: TurnStatus;
  error: string | null;
  /** The key is absent when the coach proposed no workout. */
  drafts?: DraftEntry[];
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
  | { type: "note"; text: string }
  | { type: "tool"; callId: string; label: string; running: boolean; ok: boolean }
  | { type: "draft"; callId: string; draft: WorkoutDraft }
  | { type: "done"; sources: string[] }
  | { type: "error"; text: string }
);
