// The wire types come from the Rust core. `generated/` is written by
// `scripts/gen-types.sh` from the view models of `garmin_api`. Do not copy a
// shape here by hand: re-export it. The core owns the numbers and the
// sentences, and `present.ts` turns them into the labels that the screens draw.
import type { Events } from "./generated/commands";
import type { Observation } from "./generated/Observation";
import type { TodayView } from "./generated/TodayView";

export type { CachedActivity as Activity } from "./generated/CachedActivity";
export type { Latest } from "./generated/Latest";
export type { SettingsView } from "./generated/SettingsView";
export type { Status } from "./generated/Status";
export type { SyncEvent as SyncProgress } from "./generated/SyncEvent";
export type { SyncSummary } from "./generated/SyncSummary";
export type { ThemeMode } from "./generated/ThemeMode";
export type { Observation, TodayView };

// The keys of `TodayView.sections`, in the order that the core draws them.
export type SectionKey = TodayView["sections"][number];

// An event of `subscribe`: the kind says which payload it carries.
export type BridgeEvent = { [K in keyof Events]: { kind: K; payload: Events[K] } }[keyof Events];
// What the screens draw.
export type Metric = {
  id: string;
  label: string;
  value: string;
  unit?: string;
  accent?: boolean;
};

export type LastSession = {
  // The Garmin activity id. It opens the activity detail.
  id: number;
  name: string;
  sport: string;
  // The Garmin activity type, such as "running". It picks the icon.
  typeKey: string | null;
  when: string;
  // Set only when the core says a zone verdict fits this sport.
  zones: { aboveZ2Pct: number; percents: number[] } | null;
  stats: { label: string; value: string }[];
};

export type Section =
  | { kind: "health"; metrics: Metric[] }
  | { kind: "attention"; notes: Observation[] }
  | { kind: "text"; key: "fuel" | "weight"; text: string }
  | { kind: "session"; session: LastSession }
  | { kind: "week"; labels: string[]; minutes: number[]; summary: string }
  | { kind: "drift"; summary: string; runs: { id: string; label: string; aboveZ2Pct: number }[] };

// The key figure of the day: readiness, else body battery, else sleep.
// `pct` is set for a 0 to 100 score.
export type Hero = { id: string; label: string; text: string; pct: number | null };

export type Today = {
  hero: Hero | null;
  dateLabel: string;
  lede: string;
  sections: Section[];
};
