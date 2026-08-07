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

export const routes = () => invoke<Route[]>("routes");

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

export type ChatProvider = "openrouter" | "ollama";

export interface ChatConfig {
  provider: ChatProvider | null;
  model: string | null;
  hasKey: boolean;
  ollamaReachable: boolean;
  ollamaModels: string[];
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  /** Which cache queries the model ran to answer. Shown under the answer. */
  sources?: string[];
}

export const chatConfig = () => invoke<ChatConfig>("chat_config");

export const setChatProvider = (provider: ChatProvider, model: string) =>
  invoke<void>("set_chat_provider", { provider, model });

export const setOpenrouterKey = (key: string) =>
  invoke<void>("set_openrouter_key", { key });

export const clearOpenrouterKey = () => invoke<void>("clear_openrouter_key");

export const openrouterModels = () => invoke<string[]>("openrouter_models");

/** Streams over the `chat:{id}` event channel; resolves when the turn ends. */
export const chatSend = (id: string, history: ChatMessage[]) =>
  invoke<void>("chat_send", { id, history });

/* ------------------------------------------------------------------ auth --- */

export const garminLogin = () => invoke<void>("garmin_login");
