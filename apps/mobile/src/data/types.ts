// The `today` result of the Rust core (`garmin_api::TodayView`), as JSON.
// The core owns the numbers and the sentences. `present.ts` turns them into
// the labels that the screens draw.
export type Latest = { value: number; date: string };

export type Activity = {
  activityId: number;
  name: string | null;
  typeKey: string | null;
  startTimeLocal: string | null;
  localDate: string;
  distanceM: number | null;
  durationS: number | null;
  movingDurationS: number | null;
  avgHr: number | null;
  maxHr: number | null;
  avgCadence: number | null;
  aerobicTe: number | null;
  // Seconds in zones 1 to 5.
  zoneSecs: number[] | null;
};

export type Observation = {
  accent: boolean;
  text: string;
  spark?: number[];
  sparkUnit?: string;
};

export type TodayView = {
  date: string;
  empty: boolean;
  lede: string;
  sleep: Latest | null;
  bodyBattery: Latest | null;
  readiness: Latest | null;
  hrv: Latest | null;
  restingHr: { value: number; baseline: number; delta: number; accent: boolean } | null;
  fuel: { balance: number | null; accent: boolean };
  fuelSentence: string;
  hydrationSentence: string | null;
  attention: Observation[];
  lastSession: { activity: Activity; hasZoneData: boolean; aboveZ2Pct: number } | null;
  zoneDrift: {
    targetPct: number;
    recentAvgPct: number | null;
    runs: { activityId: number; date: string | null; aboveZ2Pct: number }[];
  };
  week: { dailyKm: number[]; totalKm: number; priorKm: number; sessions: number; sentence: string };
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
  aboveZ2Pct: number | null;
  // Percent of time in zones 1 to 5.
  zones: number[];
  stats: { label: string; value: string }[];
};

export type Today = {
  dateLabel: string;
  greeting: string;
  lede: string;
  metrics: Metric[];
  fuelLine: string;
  attention: Observation[];
  lastSession: LastSession | null;
  zoneDrift: { summary: string; runs: { id: string; label: string; aboveZ2Pct: number }[] } | null;
  week: { labels: string[]; km: number[]; summary: string } | null;
};
