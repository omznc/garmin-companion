// The one data source of the app. Every screen reads through `call`.
// The Rust bridge is the source when the native module is in the build.
// Expo Go and the web have no native module, so they read the fixture.
import { File, Paths } from "expo-file-system";
import fixture from "../../../../crates/garmin-api/fixtures/today.sample.json";
import { presentToday } from "./present";
import type { BridgeEvent, Today, TodayView } from "./types";

type Source = {
  call: (cmd: string, args?: unknown) => Promise<string>;
  subscribe: (listener: (event: BridgeEvent) => void) => () => void;
  version: () => string;
};

const fixtureSource: Source = {
  async call(cmd) {
    if (cmd === "today") return JSON.stringify(fixture);
    throw new Error(`no fixture for command: ${cmd}`);
  },
  subscribe: () => () => {},
  version: () => "fixture",
};

function bridgeSource(): Source | null {
  try {
    // shortcut: a missing native module throws here, so the fixture takes over.
    const bridge =
      require("react-native-garmin-bridge") as typeof import("react-native-garmin-bridge");
    const base = Paths.document.uri.replace(/^file:\/\//, "");
    // Fixture mode while the app has no cache file of its own.
    bridge.init(base, !new File(Paths.document, "cache.sqlite3").exists);
    return {
      version: () => bridge.version(),
      call: (cmd, args = {}) => bridge.call(cmd, JSON.stringify(args)),
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
const impl: Source = bridgeImpl ?? fixtureSource;

export const sourceKind = bridgeImpl ? "bridge" : "fixture";

export async function callJson<T>(cmd: string, args: unknown = {}): Promise<T> {
  return JSON.parse(await impl.call(cmd, args)) as T;
}

export const subscribe = impl.subscribe;
export const bridgeVersion = impl.version;

// Demo only: EXPO_PUBLIC_DEMO_DATE=YYYY-MM-DD shows Today for a day, for a cache that is not current.
const demoDate = process.env.EXPO_PUBLIC_DEMO_DATE;

export async function fetchToday(): Promise<Today> {
  return presentToday(await callJson<TodayView>("today", demoDate ? { date: demoDate } : {}));
}
