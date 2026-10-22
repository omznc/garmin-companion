// The JSON fixture source. Only a dev build or EXPO_PUBLIC_FIXTURE=1 loads this file,
// so a release build does not ship the fixtures.
import activitiesFixture from "../../../../crates/garmin-api/fixtures/activities.sample.json";
import activityFixture from "../../../../crates/garmin-api/fixtures/activity.sample.json";
import bodyEmpty from "../../../../crates/garmin-api/fixtures/body.empty.json";
import bodySample from "../../../../crates/garmin-api/fixtures/body.sample.json";
import fixture from "../../../../crates/garmin-api/fixtures/today.sample.json";
import golfer from "../../../../crates/garmin-api/fixtures/today.golfer.json";
import gym from "../../../../crates/garmin-api/fixtures/today.gym.json";
import healthOnly from "../../../../crates/garmin-api/fixtures/today.health_only.json";
import newUser from "../../../../crates/garmin-api/fixtures/today.new_user.json";
import { fixtureChat } from "./chatFixtures";
import { placesFixture } from "./placesFixtures";
import type { Events } from "./generated/commands";
import type { Source } from "./source";
import { trainingFixture } from "./trainingFixtures";
import type { BridgeEvent, SettingsView, Status, SyncSummary } from "./types";
import { wellnessFixture } from "./wellnessFixtures";

// Weight, Food and Strength read one fixture each. Only the runner and the gym persona log a body.
const bodyViews = ["sample", "gym", undefined].includes(process.env.EXPO_PUBLIC_PERSONA)
  ? bodySample
  : bodyEmpty;
const bodyCommands: Record<string, keyof typeof bodySample> = {
  weight_view: "weight",
  food_view: "food",
  strength_view: "strength",
};
// The Expo Go account state. It lives in memory and resets on reload.
const fixtureAccount = { connected: false, theme: "system" as SettingsView["theme"] };
const fixtureStatus = (): Status => ({
  connected: fixtureAccount.connected,
  importableTokenPath: null,
  signInLine: "Sample data. Nothing is sent to Garmin from this build.",
});
const fixtureSettings = (): SettingsView => ({
  connected: fixtureAccount.connected,
  accountLine: fixtureAccount.connected ? "Connected (sample)." : "Not connected.",
  cacheLine: "Sample data only.",
  dataFolder: null,
  importableTokenPath: null,
  theme: fixtureAccount.theme,
  version: "fixture",
  privacyLine: "This build shows sample data and sends nothing anywhere.",
});
const fixtureSync: SyncSummary = {
  line: "Sample sync finished. No data was changed.",
  activitiesSeen: 0,
  activitiesWritten: 0,
  daysWritten: 0,
  warnings: [],
};

function fixtureAccountCall(cmd: string, args: unknown): unknown {
  if (cmd === "set_theme") fixtureAccount.theme = (args as { theme: SettingsView["theme"] }).theme;
  if (cmd === "redeem_ticket") fixtureAccount.connected = true;
  if (cmd === "sign_out") fixtureAccount.connected = false;
  switch (cmd) {
    case "status":
    case "redeem_ticket":
    case "sign_out":
      return fixtureStatus();
    case "settings":
      return fixtureSettings();
    case "set_theme":
      return { theme: fixtureAccount.theme };
    case "sign_in_url":
      return "about:blank";
    case "sync":
      return fixtureSync;
    default:
      return undefined;
  }
}

const fixtureListeners = new Set<(event: BridgeEvent) => void>();
const emitFixture = (payload: object) =>
  fixtureListeners.forEach((l) => l({ kind: "ask", payload: payload as Events["ask"] }));

// The Expo Go coach state. It lives in memory and resets on reload.
let fixtureProvider = "cloud";
const fixtureCloud = {
  premium: false,
  email: null,
  creditsRemaining: 27,
  creditsLine: "27 of 30 credits left today.",
  planLine: "Free plan: 30 credits a day.",
  out: false,
  low: false,
};

// Keep the list and the detail in agreement: answer with the id that was asked for.
const activityDetail = (args: unknown) => {
  const id = (args as { activityId?: number } | undefined)?.activityId;
  return id === undefined ? activityFixture : { ...activityFixture, activityId: id };
};

const saveConfig = (args: unknown) => {
  fixtureProvider = (args as { provider: string }).provider;
  return {};
};

const firstFixtures: Record<string, (args: unknown) => unknown> = {
  today: () => fixture,
  activities: () => activitiesFixture,
  activity_detail: activityDetail,
};

const lastFixtures: Record<string, (args: unknown) => unknown> = {
  ask_config: () => ({ provider: fixtureProvider, model: "sample", hasKey: false }),
  ask_save_config: saveConfig,
  cloud_account: () => fixtureCloud,
  cloud_sign_out: () => ({}),
};

export const fixtureSource: Source = {
  async call(cmd, args) {
    const first = firstFixtures[cmd];
    if (first) return JSON.stringify(first(args));
    const account = fixtureAccountCall(cmd, args);
    if (account !== undefined) return JSON.stringify(account);
    const recorded = wellnessFixture(cmd) ?? trainingFixture(cmd, args) ?? placesFixture(cmd);
    if (recorded) return JSON.stringify(recorded);
    const body = bodyCommands[cmd];
    if (body) return JSON.stringify(bodyViews[body]);
    const chat = fixtureChat(cmd, args, emitFixture);
    if (chat !== undefined) return JSON.stringify(chat);
    const last = lastFixtures[cmd];
    if (last) return JSON.stringify(last(args));
    if (cmd.startsWith("cloud_")) throw new Error("Email sign-in is not available in this build.");
    throw new Error(`no fixture for command: ${cmd}`);
  },
  subscribe: (listener) => {
    fixtureListeners.add(listener);
    return () => fixtureListeners.delete(listener);
  },
  version: () => "fixture",
};

// Dev only: EXPO_PUBLIC_PERSONA=golfer|gym|health_only|new_user shows a recorded persona, to check the layouts.
export const todayPersonas: Record<string, unknown> = {
  sample: fixture,
  golfer,
  gym,
  health_only: healthOnly,
  new_user: newUser,
};
