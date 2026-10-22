// The one data source of the app. Every screen reads through `call`.
// The Rust bridge is the source when the native module is in the build.
// Expo Go and the web have no native module, so they read the fixture.
import { File, Paths } from "expo-file-system";
import { Platform } from "react-native";
import { presentToday } from "./present";
import type { BridgeEvent, Today, TodayView } from "./types";

export type Source = {
  call: (cmd: string, args?: unknown) => Promise<string>;
  subscribe: (listener: (event: BridgeEvent) => void) => () => void;
  version: () => string;
};

// The condition stays inline, so Metro drops the require (and the fixtures) from a release build.
// A release build reads sample data from the Rust core, not from these files.
const fixtures = (
  __DEV__ || process.env.EXPO_PUBLIC_FIXTURE === "1" ? require("./fixtureSource") : null
) as typeof import("./fixtureSource") | null;
const fixtureSource = fixtures?.fixtureSource;

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

// The old Tauri app keeps its files in the app data folder (the parent of
// `files/`). The Rust core uses the same folder on Android, so the cache and the login stay.
const dataRoot = () =>
  Platform.OS === "android" ? Paths.document.parentDirectory : Paths.document;

function bridgeSource(): Source | null {
  try {
    // shortcut: a missing native module throws here, so the fixture takes over.
    const bridge =
      require("react-native-garmin-bridge") as typeof import("react-native-garmin-bridge");
    const root = dataRoot();
    const base = root.uri.replace(/^file:\/\//, "").replace(/\/$/, "");
    // No cache means no sync yet, so the core offers sample data. It writes nothing and sends nothing.
    const noCache = !new File(root, "cache.sqlite3").exists;
    sampleData = noCache;
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

// Dev only: EXPO_PUBLIC_JS_FIXTURE=1 reads the JSON fixtures, to check the screens in a build with an old native core.
const bridgeImpl = __DEV__ && process.env.EXPO_PUBLIC_JS_FIXTURE === "1" ? null : bridgeSource();
const unavailable: Source = {
  call: () => Promise.reject(new Error("The Rust bridge is not available in this build.")),
  subscribe: () => () => {},
  version: () => "unavailable",
};
const impl: Source = bridgeImpl ?? fixtureSource ?? unavailable;

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

// Dev only: EXPO_PUBLIC_DEMO_DATE=YYYY-MM-DD shows Today for a day, for a cache that is not current.
const demoDate = __DEV__ ? process.env.EXPO_PUBLIC_DEMO_DATE : undefined;

// Dev only: EXPO_PUBLIC_PERSONA=golfer|gym|health_only|new_user shows a recorded persona, to check the layouts.
const persona = __DEV__
  ? fixtures?.todayPersonas[process.env.EXPO_PUBLIC_PERSONA ?? ""]
  : undefined;

export async function fetchToday(): Promise<Today> {
  const view = persona
    ? (persona as TodayView)
    : await callJson<TodayView>("today", demoDate ? { date: demoDate } : {});
  const today = presentToday(view);
  const sample = sampleData || impl === fixtureSource || persona !== undefined;
  return sample ? { ...today, dateLabel: `${today.dateLabel}, sample data` } : today;
}
