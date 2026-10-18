// The one data source of the app. Every screen reads through `call`.
// The Rust bridge is the source when the native module is in the build.
// Expo Go and the web have no native module, so they read the fixture.
import { File, Paths } from "expo-file-system";
import { Platform } from "react-native";
import activitiesFixture from "../../../../crates/garmin-api/fixtures/activities.sample.json";
import activityFixture from "../../../../crates/garmin-api/fixtures/activity.sample.json";
import bodyEmpty from "../../../../crates/garmin-api/fixtures/body.empty.json";
import bodySample from "../../../../crates/garmin-api/fixtures/body.sample.json";
import fixture from "../../../../crates/garmin-api/fixtures/today.sample.json";
import golfer from "../../../../crates/garmin-api/fixtures/today.golfer.json";
import gym from "../../../../crates/garmin-api/fixtures/today.gym.json";
import healthOnly from "../../../../crates/garmin-api/fixtures/today.health_only.json";
import newUser from "../../../../crates/garmin-api/fixtures/today.new_user.json";
import { presentToday } from "./present";
import { wellnessFixture } from "./wellnessFixtures";
import type { BridgeEvent, SettingsView, Status, SyncSummary, Today, TodayView } from "./types";

type Source = {
  call: (cmd: string, args?: unknown) => Promise<string>;
  subscribe: (listener: (event: BridgeEvent) => void) => () => void;
  version: () => string;
};

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
let fixtureStop = false;

// A canned answer for the Ask screen, so it runs in Expo Go with no model.
const FIXTURE_ANSWER =
  "This is a sample answer, so the screen works with no setup.\n\nWith a model set up, the coach reads **your own Garmin data** first, then answers. Health comes first:\n\n- Sleep, resting heart rate and HRV\n- Recovery, stress and body battery\n- Activity of any sport, but only the sport your data shows\n\nA number that the data cannot support is left out, and the answer says why.";

async function fixtureTurn() {
  const send = (payload: unknown) => fixtureListeners.forEach((l) => l({ kind: "ask", payload }));
  const tool = { type: "tool", callId: "fixture", label: "Reading 14 days of recovery signals" };
  send({ ...tool, running: true, ok: true });
  await new Promise((r) => setTimeout(r, 400));
  send({ ...tool, running: false, ok: true });
  for (const text of FIXTURE_ANSWER.split(/(?<= )/)) {
    if (fixtureStop) break;
    send({ type: "delta", text });
    await new Promise((r) => setTimeout(r, 40));
  }
  send({ type: "done", sources: [] });
}

const fixtureSource: Source = {
  async call(cmd, args) {
    if (cmd === "today") return JSON.stringify(fixture);
    if (cmd === "activities") return JSON.stringify(activitiesFixture);
    if (cmd === "activity_detail") {
      // Keep the list and the detail in agreement: answer with the id that was asked for.
      const id = (args as { activityId?: number } | undefined)?.activityId;
      return JSON.stringify(
        id === undefined ? activityFixture : { ...activityFixture, activityId: id },
      );
    }
    const account = fixtureAccountCall(cmd, args);
    if (account !== undefined) return JSON.stringify(account);
    const recorded = wellnessFixture(cmd);
    if (recorded) return JSON.stringify(recorded);
    const body = bodyCommands[cmd];
    if (body) return JSON.stringify(bodyViews[body]);
    if (cmd === "ask_send") {
      fixtureStop = false;
      void fixtureTurn();
      return "{}";
    }
    if (cmd === "ask_stop") {
      fixtureStop = true;
      return "{}";
    }
    if (cmd === "ask_config")
      return JSON.stringify({ provider: "ollama", model: "sample", hasKey: false });
    throw new Error(`no fixture for command: ${cmd}`);
  },
  subscribe: (listener) => {
    fixtureListeners.add(listener);
    return () => fixtureListeners.delete(listener);
  },
  version: () => "fixture",
};

const fixtureAllowed = __DEV__ || process.env.EXPO_PUBLIC_FIXTURE === "1";

let sampleData = false;

// The generated error has no message. Read the reason from its fields.
type Inner = { reason?: string; json?: string };

function innerOf(error: unknown): Inner | undefined {
  const inner =
    typeof error === "object" && error !== null ? (error as { inner?: unknown }).inner : undefined;
  return typeof inner === "object" && inner !== null ? (inner as Inner) : undefined;
}

// Keep the error kind. A message that is only a payload gets the kind in front.
function readable(error: unknown): Error {
  const inner = innerOf(error);
  if (inner?.reason) return new Error(inner.reason);
  if (inner?.json) {
    const detail = JSON.parse(inner.json) as { message?: string; kind?: string };
    const message = detail.message ?? detail.kind ?? inner.json;
    const text = detail.kind && message !== detail.kind ? `${detail.kind}: ${message}` : message;
    return Object.assign(new Error(text), { kind: detail.kind });
  }
  return error instanceof Error ? error : new Error(String(error));
}

function bridgeSource(): Source | null {
  try {
    // shortcut: a missing native module throws here, so the fixture takes over.
    const bridge =
      require("react-native-garmin-bridge") as typeof import("react-native-garmin-bridge");
    // The old Tauri app keeps its files in the app data folder (the parent of
    // `files/`). Open the same folder on Android so the cache and the login stay.
    const root = Platform.OS === "android" ? Paths.document.parentDirectory : Paths.document;
    const base = root.uri.replace(/^file:\/\//, "").replace(/\/$/, "");
    // Sample data only in a dev build or with EXPO_PUBLIC_FIXTURE=1. A release build fakes data only when that flag is set.
    const noCache = !new File(root, "cache.sqlite3").exists;
    sampleData = noCache && fixtureAllowed;
    bridge.init(base, sampleData);
    return {
      version: () => bridge.version(),
      call: (cmd, args = {}) =>
        bridge.call(cmd, JSON.stringify(args)).catch((error) => {
          throw readable(error);
        }),
      subscribe: (listener) => {
        const sub = bridge.subscribe({
          onEvent: (kind, payload) => listener({ kind, payload: JSON.parse(payload) }),
        });
        return () => sub.cancel();
      },
    };
  } catch (error) {
    console.warn("Rust bridge is not available, so the app reads the fixture.", error);
    return null;
  }
}

const bridgeImpl = bridgeSource();
const unavailable: Source = {
  call: () => Promise.reject(new Error("The Rust bridge is not available in this build.")),
  subscribe: () => () => {},
  version: () => "unavailable",
};
const impl: Source = bridgeImpl ?? (fixtureAllowed ? fixtureSource : unavailable);

export const sourceKind = bridgeImpl ? "bridge" : impl === fixtureSource ? "fixture" : "none";

export async function callJson<T>(cmd: string, args: unknown = {}): Promise<T> {
  return JSON.parse(await impl.call(cmd, args)) as T;
}

export const subscribe = impl.subscribe;
// True when the bridge shows the fixture, so Setup can offer to skip.
export const isSampleData = () => sampleData || impl === fixtureSource;
// Sign-in worked, so the Rust side left sample mode. Show real data from now on.
export const leaveSampleData = () => {
  sampleData = false;
};
export const bridgeVersion = impl.version;

// Demo only: EXPO_PUBLIC_DEMO_DATE=YYYY-MM-DD shows Today for a day, for a cache that is not current.
const demoDate = process.env.EXPO_PUBLIC_DEMO_DATE;

// Dev only: EXPO_PUBLIC_PERSONA=golfer|gym|health_only|new_user shows a recorded persona, to check the layouts.
const personas: Record<string, unknown> = {
  sample: fixture,
  golfer,
  gym,
  health_only: healthOnly,
  new_user: newUser,
};
const persona = __DEV__ ? personas[process.env.EXPO_PUBLIC_PERSONA ?? ""] : undefined;

export async function fetchToday(): Promise<Today> {
  const view = persona
    ? (persona as TodayView)
    : await callJson<TodayView>("today", demoDate ? { date: demoDate } : {});
  const today = presentToday(view);
  const sample = sampleData || impl === fixtureSource || persona !== undefined;
  return sample ? { ...today, dateLabel: `${today.dateLabel}, sample data` } : today;
}
