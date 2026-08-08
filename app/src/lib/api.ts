/**
 * Typed wrappers over the Tauri commands.
 *
 * Everything the UI reads comes from the local SQLite cache except the four
 * calls marked `live` — those hit Garmin directly, and only ever for one
 * activity the user has explicitly opened.
 */
import { invoke } from "@tauri-apps/api/core";

export interface CachedActivity {
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
  calories: number | null;
  elevationGain: number | null;
  steps: number | null;
  aerobicTe: number | null;
  anaerobicTe: number | null;
  /** Seconds in HR zones 1–5. All zeros when the session had no HR strap. */
  zoneSecs: [number, number, number, number, number];
}

export interface DailyMetrics {
  date: string;
  restingHr: number | null;
  hrvLastNight: number | null;
  hrvWeeklyAvg: number | null;
  hrvStatus: string | null;
  trainingReadiness: number | null;
  sleepSecs: number | null;
  sleepScore: number | null;
  steps: number | null;
  stressAvg: number | null;
  bodyBatteryHigh: number | null;
  bodyBatteryLow: number | null;
  // The fuel side of the same row. The backend has always sent these; they
  // went untyped for a while, which quietly kept them out of every derived
  // paragraph on Today, Insights and Reports. Null on days with no food log —
  // which is not a zero-calorie day, and nothing here may treat it as one.
  consumedKcal: number | null;
  totalBurnKcal: number | null;
  activeKcal: number | null;
  bmrKcal: number | null;
  netCalorieGoal: number | null;
  hydrationMl: number | null;
  hydrationGoalMl: number | null;
  sweatLossMl: number | null;
}

export interface ConnectionStatus {
  connected: boolean;
  importableTokenPath: string | null;
}

export interface CacheSummary {
  activities: number;
  lastSync: string | null;
  path: string | null;
}

export interface SyncReport {
  activitiesSeen: number;
  activitiesWritten: number;
  daysWritten: number;
  warnings: string[];
}

export interface Profile {
  displayName: string;
  fullName: string | null;
  profileId: number | null;
}

/* ------------------------------------------------------------ connection --- */

export const garminStatus = () => invoke<ConnectionStatus>("garmin_status");

export const garminImportTokens = (path?: string) =>
  invoke<void>("garmin_import_tokens", { path: path ?? null });

export const garminDisconnect = () => invoke<void>("garmin_disconnect");

export const garminProfile = () => invoke<Profile>("garmin_profile");

/* ----------------------------------------------------------------- cache --- */

export const cacheSummary = () => invoke<CacheSummary>("cache_summary");

export const cachedActivities = (limit = 30, typeKey?: string) =>
  invoke<CachedActivity[]>("cached_activities", {
    limit,
    typeKey: typeKey ?? null,
  });

export const cachedActivitiesSince = (from: string) =>
  invoke<CachedActivity[]>("cached_activities_since", { from });

export const cachedActivity = (activityId: number) =>
  invoke<CachedActivity | null>("cached_activity", { activityId });

export const cachedDaily = (days = 30) =>
  invoke<DailyMetrics[]>("cached_daily", { days });

export const syncNow = (days = 30, full = false) =>
  invoke<SyncReport>("sync_now", { days, full });

/* ------------------------------------------------------------- nutrition --- */

export interface NutritionDay {
  date: string;
  consumedKcal: number | null;
  totalBurnKcal: number | null;
  activeKcal: number | null;
  bmrKcal: number | null;
  netCalorieGoal: number | null;
  /** Eaten minus burned; negative is a deficit. Null unless both are known. */
  balanceKcal: number | null;
  hydrationMl: number | null;
  hydrationGoalMl: number | null;
  sweatLossMl: number | null;
  /** False when no food was logged that day — not the same as eating nothing. */
  logged: boolean;
}

export interface NutritionReport {
  days: NutritionDay[];
  daysLogged: number;
  /** Averaged over logged days only. */
  avgConsumedKcal: number | null;
  avgBurnKcal: number | null;
  avgBalanceKcal: number | null;
}

export const nutrition = (days = 30) =>
  invoke<NutritionReport>("nutrition", { days });

/* ---------------------------------------------------------------- weight --- */

export interface WeightPoint {
  date: string;
  kg: number;
  /** The smoothed trend at this point — the figure to quote as "your weight". */
  trendKg: number | null;
  /**
   * A reading that disagrees with both its neighbours by more than a body can
   * move. Shown, never hidden, but excluded from the trend and the rate.
   */
  outlier: boolean;
  /** `MFP`, `MANUAL`, `USER_SETTING`, or a scale's name. */
  source: string | null;
}

/** The food log against the scale, over the span the weigh-ins cover. */
export interface EnergyCheck {
  spanDays: number;
  /** Days in the span carrying a food log. Everything else here is from these. */
  loggedDays: number;
  coveragePct: number;
  balanceKcal: number;
  /**
   * The logged balance in kilograms. Not scaled up to cover unlogged days —
   * that would be a prediction about data that doesn't exist.
   */
  predictedChangeKg: number;
  actualChangeKg: number | null;
}

export interface WeightGoal {
  targetKg: number;
  /** Signed: negative means there is weight to lose. */
  deltaKg: number;
  /** Null when the trend is flat or pointing away from the target. */
  etaDate: string | null;
  etaDays: number | null;
}

export interface WeightReport {
  /** Oldest first. */
  points: WeightPoint[];
  count: number;
  latestKg: number | null;
  latestDate: string | null;
  daysSinceLatest: number | null;
  trendKg: number | null;
  changeKg: number | null;
  /** Null until there are enough readings, spread widely enough, to mean anything. */
  rateKgPerWeek: number | null;
  spanDays: number | null;
  bmi: number | null;
  heightCm: number | null;
  /** False on every account without a smart scale, which is most of them. */
  hasBodyComposition: boolean;
  energy: EnergyCheck | null;
  goal: WeightGoal | null;
}

export const weight = (days = 180) => invoke<WeightReport>("weight", { days });

/**
 * The target is the app's own, not Garmin's — the account exposes no readable
 * weight goal. Pass null to clear it. Weigh-ins themselves are always Garmin's
 * and are never written from here.
 */
export const setWeightGoal = (targetKg: number | null) =>
  invoke<void>("set_weight_goal", { targetKg });

export interface WeightSummary {
  text: string;
  generatedAt: string;
  /** True when this is the stored summary rather than one just written. */
  cached: boolean;
}

/** Kept until the numbers behind it move; `force` is the regenerate control. */
export const weightSummary = (days = 180, force = false) =>
  invoke<WeightSummary>("weight_summary", { days, force });

/* -------------------------------------------------------------- workouts --- */

export interface Workout {
  workoutId: number;
  name: string | null;
  sportType: string | null;
  description: string | null;
  estDurationS: number | null;
  estDistanceM: number | null;
  updatedAt: string | null;
}

export const workouts = () => invoke<Workout[]>("workouts");

/* --------------------------------------------------------- workout drafts --- */

/**
 * A workout the model has proposed but nobody has agreed to.
 *
 * The shape mirrors `garmin_core::workout` exactly — it crosses the boundary
 * once as JSON, gets edited by the card on the Ask screen, and crosses back to
 * be validated and posted. Anything this file invents that Rust doesn't have
 * would fail to deserialize on the way home.
 */
export type WorkoutSport = "running" | "cycling" | "cardio" | "strength_training";

export type StepKind = "warmup" | "interval" | "recovery" | "rest" | "cooldown";

export type EndCondition =
  | { type: "time"; seconds: number }
  | { type: "distance"; metres: number }
  | { type: "lap_button" };

export type StepTarget =
  | { type: "none" }
  | { type: "hr_zone"; zone: number }
  | { type: "bpm"; low: number; high: number };

export interface ExecStep {
  kind: StepKind;
  end: EndCondition;
  target: StepTarget;
  note?: string;
}

/** One level of nesting only, matching the Rust enum — a repeat holds plain steps. */
export type DraftStep =
  | ({ type: "exec" } & ExecStep)
  | { type: "repeat"; times: number; steps: ExecStep[] };

export interface WorkoutDraft {
  name: string;
  sport: WorkoutSport;
  description?: string;
  steps: DraftStep[];
  /**
   * The Garmin id this draft became, once it has been sent.
   *
   * This side's field, not Rust's — it is saved with the conversation so that
   * reopening one shows what was already created rather than offering the
   * button again. `createWorkout` strips it on the way out.
   */
  savedWorkoutId?: number;
}

/**
 * Save a drafted workout to the Garmin account, returning its new id.
 *
 * The one call in this app that changes anything on Garmin. It is reachable
 * only from a button on a workout card the athlete is looking at; the model
 * that drafted it cannot reach this.
 */
export const createWorkout = ({ savedWorkoutId: _, ...draft }: WorkoutDraft) =>
  invoke<number>("create_workout", { draft });

/* ---------------------------------------------------------------- routes --- */

export interface RouteOuting {
  activityId: number;
  name: string | null;
  localDate: string | null;
  distanceM: number | null;
  durationS: number | null;
  /** `[lat, lon]` pairs, already thinned for drawing. */
  points: [number, number][];
}

export interface Route {
  name: string | null;
  typeKey: string | null;
  /** How many outings matched into this route. */
  times: number;
  avgDistanceM: number | null;
  outings: RouteOuting[];
}

/**
 * Ordering for the routes list. Applied by the query rather than here: only
 * the first forty routes in this order are sent with their coordinates, so
 * re-sorting on this side would leave the top of the list untraced.
 */
export type RouteSort = "recent" | "repeats" | "distance";

export const routes = (sort: RouteSort = "recent") =>
  invoke<Route[]>("routes", { sort });

/* -------------------------------------------------------------- analysis --- */

/**
 * The sampled series, one array per column and all the same length.
 *
 * Nulls are gaps, never zeros — a null pace is a moment spent standing still,
 * and drawing it as 0 min/km would put a spike through every chart. `lat`/`lon`
 * are all-null on anything recorded indoors.
 */
export interface ActivitySeries {
  elapsedS: (number | null)[];
  hr: (number | null)[];
  paceMinKm: (number | null)[];
  cadence: (number | null)[];
  elevationM: (number | null)[];
  distanceM: (number | null)[];
  lat: (number | null)[];
  lon: (number | null)[];
}

export interface ActivityLap {
  index: number;
  distanceM: number | null;
  durationS: number | null;
  paceMinKm: number | null;
  avgHr: number | null;
  maxHr: number | null;
  avgCadence: number | null;
  elevationGainM: number | null;
}

export interface ZoneProfile {
  /** Lower bound of Z1–Z5, in bpm. */
  floors: [number, number, number, number, number];
  secs: [number, number, number, number, number];
  percent: [number, number, number, number, number];
  /** False when Garmin didn't send boundaries and the fallback ladder was used. */
  measured: boolean;
}

/** Not a severity — a session can be remarkable without anything being wrong. */
export type HighlightTone = "good" | "note" | "watch";

export interface Highlight {
  /** Stable slug, so the screen can pick an icon without parsing the prose. */
  kind: string;
  tone: HighlightTone;
  title: string;
  detail: string;
  /** Elapsed seconds. Null for a highlight about the session as a whole. */
  atS: number | null;
  untilS: number | null;
}

export interface ActivityComparison {
  sessions: number;
  avgPaceMinKm: number | null;
  avgHr: number | null;
  avgCadence: number | null;
  avgPercentAboveZ2: number | null;
  avgDurationS: number | null;
  /** This session minus the average. Negative pace is faster. */
  paceDelta: number | null;
  hrDelta: number | null;
  cadenceDelta: number | null;
  percentAboveZ2Delta: number | null;
}

/**
 * Everything the activity screen draws below the summary numbers.
 *
 * One query rather than the three it replaced: the charts, the map, the splits
 * and the highlights all come off the same fetch, and the written summary is
 * generated from this exact struct — so the prose and the pins on the map can't
 * disagree about what happened.
 */
/**
 * What the sport means for how the session can be read. `paced` and `endurance`
 * are continuous aerobic work, where time in Z2 is a target; `interval` and
 * `other` are sets and rests, where it is a description of the rest.
 */
export type Discipline = "paced" | "endurance" | "interval" | "other";

export interface ActivityAnalysis {
  activityId: number;
  name: string | null;
  typeKey: string | null;
  discipline: Discipline;
  startTimeLocal: string | null;
  distanceM: number | null;
  durationS: number | null;
  movingDurationS: number | null;
  avgHr: number | null;
  maxHr: number | null;
  avgCadence: number | null;
  elevationGainM: number | null;
  calories: number | null;
  aerobicTe: number | null;
  anaerobicTe: number | null;
  paceMinKm: number | null;
  zones: ZoneProfile;
  series: ActivitySeries;
  laps: ActivityLap[];
  highlights: Highlight[];
  comparison: ActivityComparison | null;
  tags: string[];
  /** No position recorded at all — a treadmill, a rower, a strength session. */
  indoor: boolean;
  computedAt: string;
}

/**
 * Cached against a fingerprint of the activity after the first open, so
 * reopening a session costs nothing and works offline.
 */
export const activityAnalysis = (activityId: number, force = false) =>
  invoke<ActivityAnalysis>("activity_analysis", { activityId, force });

export interface ActivityCritique {
  text: string;
  generatedAt: string;
  /** True when this is the stored critique rather than one just written. */
  cached: boolean;
}

/**
 * The critique already written about a session, or null. Free — a read of the
 * local table — so it can run on every open.
 */
export const cachedActivityCritique = (activityId: number) =>
  invoke<ActivityCritique | null>("cached_activity_critique", { activityId });

/**
 * Write one. Bills a request every time, so this is only ever called from the
 * button; the result is kept until the numbers or the tags behind it move.
 */
export const activityCritique = (activityId: number) =>
  invoke<ActivityCritique>("activity_critique", { activityId });

/* ------------------------------------------------------------------ tags --- */

export const activityTags = (activityId: number) =>
  invoke<string[]>("activity_tags", { activityId });

/**
 * Replace an activity's tags, returning them as stored.
 *
 * What comes back is rarely what went in — tags are trimmed, lowercased and
 * deduplicated on the way to disk, so the editor renders the response rather
 * than what was typed.
 */
export const setActivityTags = (activityId: number, tags: string[]) =>
  invoke<string[]>("set_activity_tags", { activityId, tags });

export interface TagCount {
  tag: string;
  count: number;
}

/** Every tag in use, commonest first. */
export const allTags = () => invoke<TagCount[]>("all_tags");

/* ------------------------------------------------------------------ live --- */

export const activitySplits = (activityId: number) =>
  invoke<unknown>("activity_splits", { activityId });

export const activityDetails = (activityId: number, points = 400) =>
  invoke<unknown>("activity_details", { activityId, points });

export const gearList = () => invoke<GearRow[]>("gear_list");

export interface GearRow {
  gear: {
    uuid: string;
    displayName: string | null;
    customMakeModel: string | null;
    gearTypeName: string | null;
    maximumMeters: number | null;
    gearStatusName: string | null;
    dateBegin: string | null;
    dateEnd: string | null;
  };
  stats: { totalDistance?: number; totalActivities?: number } | null;
}

/* ------------------------------------------------------------------ chat --- */

export type ChatProvider = "cloud" | "openrouter" | "ollama";

/** Whether a provider sends anything off this machine. */
export const isHosted = (p: ChatProvider | null) => p !== null && p !== "ollama";

export interface ChatConfig {
  provider: ChatProvider | null;
  model: string | null;
  hasKey: boolean;
  /** Whether the chosen model takes a JSON schema, recorded when it was picked. */
  structured: boolean;
  ollamaReachable: boolean;
  ollamaModels: string[];
}

/** An OpenRouter model this app can actually use — i.e. one that calls tools. */
export interface ModelInfo {
  id: string;
  name: string;
  /** Tokens of context. */
  context: number;
  /** USD per million tokens. */
  promptPerM: number;
  completionPerM: number;
  /** Takes a JSON schema. Preferred, not required. */
  structured: boolean;
}

/** OpenRouter's default: cheap, fast, long-context, and it calls tools. */
export const DEFAULT_MODEL = "inclusionai/ling-3.0-flash";

/**
 * The only model the hosted proxy serves, and the reason there is no picker for
 * it — the worker rejects anything else, so offering a choice would be offering
 * a list of ways to get an error. Mirrors `chat::CLOUD_MODEL`.
 */
export const CLOUD_MODEL = DEFAULT_MODEL;

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  /** Which cache queries the model ran to answer. Shown under the answer. */
  sources?: string[];
  /**
   * Workouts the model proposed in this turn, shown as cards under the answer.
   * Saved with the conversation, so reopening it offers them again — a session
   * you didn't send on Tuesday is still a session on Thursday.
   *
   * Stripped before the history goes back to the model (`chatSend` sends role
   * and content only), so a card can't be mistaken for something it said.
   */
  drafts?: WorkoutDraft[];
}

export const chatConfig = () => invoke<ChatConfig>("chat_config");

/**
 * Running totals for what the model has cost, since one answer is several
 * requests and none of it is visible from the outside.
 *
 * Per provider, because whose money it is is the question these answer: your own
 * OpenRouter key, the project's proxy, or a local Ollama that bills nobody.
 */
export interface AiUsage {
  /** Whose spending this is. */
  provider: ChatProvider;
  /** Requests, not questions — a question is up to seven of these. */
  requests: number;
  promptTokens: number;
  completionTokens: number;
  /** Prompt tokens the provider served from its own cache, billed at a fraction. */
  cachedTokens: number;
  /** USD, as OpenRouter reports it. Ollama costs nothing and reports nothing. */
  costUsd: number;
  /** When counting started. Null until the first request. */
  since: string | null;
}

/** The totals split by who is being billed. */
export interface AiUsageReport {
  /** The configured provider's totals. Null before one has been chosen. */
  current: AiUsage | null;
  /**
   * What the providers you aren't using ran up while you were. Only the ones
   * with requests against them — switching away doesn't unspend the money, so
   * the number stays visible rather than reading as a fresh zero.
   */
  others: AiUsage[];
}

/**
 * Whether the last request that reached the configured provider worked.
 *
 * Read, not probed: an active health check would spend a request — real money
 * on a hosted provider — to answer what the last real request already answered.
 * Null means nothing has been asked of it yet this run, which is not the same
 * as it being broken.
 */
export interface AiHealth {
  ok: boolean;
  /** Why it failed, ready to show. Null when the last call worked. */
  message: string | null;
  provider: string;
  at: string;
}

export const chatHealth = () => invoke<AiHealth | null>("chat_health");

export const chatUsage = () => invoke<AiUsageReport>("chat_usage");

/** Clears one provider's totals; the configured one when none is named. */
export const resetChatUsage = (provider?: ChatProvider) =>
  invoke<void>("reset_chat_usage", { provider: provider ?? null });

export const setChatProvider = (provider: ChatProvider, model: string, structured = false) =>
  invoke<void>("set_chat_provider", { provider, model, structured });

export const setOpenrouterKey = (key: string) =>
  invoke<void>("set_openrouter_key", { key });

export const clearOpenrouterKey = () => invoke<void>("clear_openrouter_key");

/**
 * Get this install's id from the hosted coach now, so the first question
 * doesn't wait on it. Call it when the hosted coach is chosen.
 *
 * A no-op once there is an id — it fills a gap, it never swaps one. Rejects if
 * the coach can't be reached or has no new installs left today; both are worth
 * ignoring at the call site, since the next question asks again and the health
 * banner already carries the reason.
 */
export const prepareCloudChat = () => invoke<void>("prepare_cloud_chat");

// There is no `clearDeviceId`. Forgetting this install's id used to mint a new
// one on the next question, which started the hosted proxy's daily quota over —
// a cap you can clear yourself is not a cap. Ids come from the proxy now, and
// the command is gone from the Rust side, so this isn't an omission to be
// helpfully restored.

export const openrouterModels = () => invoke<ModelInfo[]>("openrouter_models");

/**
 * Streams over the `chat:{id}` event channel; resolves when the turn ends.
 *
 * Only role and content go over — `sources` and `drafts` are this side's record
 * of how an answer was produced, and sending them back would put the model's
 * own tool bookkeeping into its context as if it were part of the conversation.
 */
export const chatSend = (id: string, history: ChatMessage[], activityId?: number) =>
  invoke<void>("chat_send", {
    id,
    history: history.map((m) => ({ role: m.role, content: m.content })),
    // Scopes the turn to one session: its analysis goes in front of the model
    // so "was that too hard?" has an antecedent. Same tools underneath — the
    // model still reaches past the session when the answer needs the weeks
    // around it.
    activityId: activityId ?? null,
  });

/* -------------------------------------------------------- chat sessions --- */

/** A saved conversation's headline. The bodies load separately. */
export interface ChatSessionMeta {
  sessionId: string;
  startedAt: string;
  updatedAt: string;
  /** The first question asked, which is what a conversation is "about". */
  title: string;
  messageCount: number;
}

export interface ChatSession extends ChatSessionMeta {
  /** JSON-encoded `ChatMessage[]`; the cache stores it opaquely. */
  messages: string;
}

export const chatSessions = (limit = 20, offset = 0) =>
  invoke<ChatSessionMeta[]>("chat_sessions", { limit, offset });

export const chatSession = (sessionId: string) =>
  invoke<ChatSession | null>("chat_session", { sessionId });

export const saveChatSession = (s: {
  sessionId: string;
  title: string;
  startedAt: string;
  messages: ChatMessage[];
}) =>
  invoke<void>("save_chat_session", {
    sessionId: s.sessionId,
    title: s.title,
    startedAt: s.startedAt,
    messages: JSON.stringify(s.messages),
    messageCount: s.messages.length,
  });

export const deleteChatSession = (sessionId: string) =>
  invoke<void>("delete_chat_session", { sessionId });

/** Three things worth asking next. Returns [] rather than throwing on a dud. */
export const chatFollowups = (history: ChatMessage[]) =>
  invoke<string[]>("chat_followups", {
    history: history.map((m) => ({ role: m.role, content: m.content })),
  });

/* ---------------------------------------------------------------- themes --- */

export type Appearance = "light" | "dark";

/**
 * A theme as it sits on disk, one JSON file each.
 *
 * Only seven colours, because the rest of the tokens are derived from them —
 * `lib/customTheme.ts` does that, and `garmin_core::theme` says why the split
 * falls where it does.
 */
export interface CustomTheme {
  /** The filename's stem. Assigned on save from the name, never typed. */
  slug: string;
  name: string;
  appearance: Appearance;
  note: string;
  colors: {
    bg: string;
    bg2: string;
    fg: string;
    muted: string;
    faint: string;
    acc: string;
    warn: string;
  };
  /** Absent means "pick one from the appearance". */
  iconTintAlpha?: number;
}

export const themesList = () => invoke<CustomTheme[]>("themes_list");

/** Returns the theme as filed — the slug is derived from the name, so a save
 *  is also how you find out what the selection should be stored as. */
export const themeSave = (theme: CustomTheme) =>
  invoke<CustomTheme>("themes_save", { theme });

export const themeDelete = (slug: string) => invoke<void>("themes_delete", { slug });

/** Created on the way past, so this never names a folder that isn't there. */
export const themesDir = () => invoke<string>("themes_dir");

/** Show the folder in the file manager. Opened from Rust — see `themes_open`. */
export const themesOpen = () => invoke<void>("themes_open");

/* ------------------------------------------------------------------ auth --- */

export const garminLogin = () => invoke<void>("garmin_login");
