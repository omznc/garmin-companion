// The `today` result of the Rust core (`garmin_api::TodayView`), as JSON.
// The core owns the numbers and the sentences. `present.ts` turns them into
// the labels that the screens draw.
export type Latest = { value: number; date: string };

export type Activity = {
  activityId: number;
  name: string | null;
  typeKey: string | null;
  startTimeLocal: string | null;
  localDate: string | null;
  distanceM: number | null;
  durationS: number | null;
  movingDurationS: number | null;
  avgHr: number | null;
  maxHr: number | null;
  avgCadence: number | null;
  aerobicTe: number | null;
  steps: number | null;
  // Seconds in zones 1 to 5.
  zoneSecs: number[] | null;
};

export type Observation = {
  accent: boolean;
  text: string;
  spark?: number[];
  sparkUnit?: string;
};

export type SectionKey =
  | "health_tiles"
  | "attention"
  | "fuel"
  | "weight"
  | "last_session"
  | "week"
  | "zone_drift";

export type TodayView = {
  date: string;
  empty: boolean;
  lede: string;
  sleep: Latest | null;
  bodyBattery: Latest | null;
  readiness: Latest | null;
  hrv: Latest | null;
  steps: Latest | null;
  stress: Latest | null;
  restingHr: { value: number; baseline: number; delta: number; accent: boolean } | null;
  fuel: { balance: number | null; accent: boolean };
  fuelSentence: string;
  hydrationSentence: string | null;
  weight: { trendKg: number | null; rateKgPerWeek: number | null; count: number } | null;
  attention: Observation[];
  lastSession: {
    activity: Activity;
    hasZoneData: boolean;
    aboveZ2Pct: number;
    showZoneVerdict: boolean;
    sportLabel: string;
  } | null;
  zoneDrift: {
    targetPct: number;
    recentAvgPct: number | null;
    runs: { activityId: number; date: string | null; aboveZ2Pct: number }[];
  };
  week: {
    dailyMinutes: number[];
    totalMinutes: number;
    totalKm: number;
    sessions: number;
    sentence: string;
  };
  // The core sends the sections in the order to draw them, and leaves out an empty one.
  sections: SectionKey[];
};

export type BridgeEvent = { kind: string; payload: unknown };

// What the screens draw.
export type Metric = {
  id: string;
  label: string;
  value: string;
  unit?: string;
  accent?: boolean;
};

export type LastSession = {
  name: string;
  sport: string;
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

export type Today = {
  dateLabel: string;
  greeting: string;
  lede: string;
  sections: Section[];
};
