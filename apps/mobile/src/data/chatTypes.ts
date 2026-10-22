// The shapes of the Ask threads. They come from `garmin_api::chat` and the
// `garmin-coach` events, through `generated/`. A step is a tool call, or a
// note: what the model said between two tool rounds (`label` is then the
// text). A stored step with no `kind` is a tool.
import type { Events } from "./generated/commands";

export type { ChatStep as Step } from "./generated/ChatStep";
// A workout that the coach proposed. `savedWorkoutId` is set once the person saved it to Garmin.
export type { DraftEnd } from "./generated/DraftEnd";
export type { DraftEntry } from "./generated/DraftEntry";
export type { DraftExec } from "./generated/DraftExec";
export type { DraftKind } from "./generated/DraftKind";
export type { DraftSport } from "./generated/DraftSport";
export type { DraftStep } from "./generated/DraftStep";
export type { DraftTarget } from "./generated/DraftTarget";
export type { HistoryRow } from "./generated/HistoryRow";
export type { Thread } from "./generated/Thread";
export type { ThreadMeta } from "./generated/ThreadMeta";
export type { Turn } from "./generated/Turn";
export type { TurnStatus } from "./generated/TurnStatus";
export type { WorkoutDraft } from "./generated/WorkoutDraft";

// An `ask` event: the JSON of the turn event, with its session and its turn.
export type AskEventPayload = Events["ask"];
