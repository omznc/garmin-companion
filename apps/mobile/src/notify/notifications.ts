// The native side: the permission, the Android channels, the delivery and the tap.
// Notifications are local. The app sends nothing to a push server.
import * as Notifications from "expo-notifications";
import { Platform } from "react-native";
import { call, isSampleData } from "../data/source";
import { getStatus } from "../data/account";
import type { NotifyDeps } from "./run";
import { readPrefs, readState, setNotifyPrefs, writeState } from "./store";
import type { Planned } from "./plan";

const CHANNELS = {
  activities: {
    name: "New activities",
    description: "A summary after a sync that finds new activities.",
  },
  nudges: {
    name: "Notes for today",
    description: "The notes that the Today screen marks as worth acting on.",
  },
} as const;

async function ensureChannels() {
  if (Platform.OS !== "android") return;
  for (const [id, c] of Object.entries(CHANNELS)) {
    await Notifications.setNotificationChannelAsync(id, {
      name: c.name,
      description: c.description,
      importance: Notifications.AndroidImportance.DEFAULT,
    });
  }
}

// Show a banner also when the app is open.
export function configureNotifications() {
  Notifications.setNotificationHandler({
    handleNotification: async () => ({
      shouldShowBanner: true,
      shouldShowList: true,
      shouldPlaySound: false,
      shouldSetBadge: false,
    }),
  });
}

const allowed = async () => (await Notifications.getPermissionsAsync()).granted;

// Ask for the permission only when the person turns the master switch on.
// Returns false when the system refuses, and then the switch stays off.
export async function turnOnNotifications(): Promise<boolean> {
  await ensureChannels();
  const granted = (await allowed()) || (await Notifications.requestPermissionsAsync()).granted;
  setNotifyPrefs({ enabled: granted });
  return granted;
}

const deps: NotifyDeps = {
  prefs: readPrefs,
  state: readState,
  saveState: writeState,
  signedIn: async () => (await getStatus()).connected,
  sample: isSampleData,
  facts: async () => {
    const [today, recent] = await Promise.all([
      call("today"),
      call("cached_activities", { limit: 20 }),
    ]);
    return {
      date: today.date,
      attention: today.attention,
      recent: recent.map((a) => ({ id: a.activityId, name: a.name })),
    };
  },
  permitted: allowed,
  deliver: async (p: Planned) => {
    await ensureChannels();
    await Notifications.scheduleNotificationAsync({
      identifier: p.id,
      content: { title: p.title, body: p.body, data: { href: p.href } },
      trigger: Platform.OS === "android" ? { channelId: p.channel } : null,
    });
  },
};

export { deps as notifyDeps };
