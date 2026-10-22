// The state of the Notifications page. The master switch asks for the system permission when the
// person turns it on, and not before. The page reads the permission when it opens and when the app
// returns to the foreground, so a block in the phone settings shows at once.
import * as Notifications from "expo-notifications";
import { useEffect, useState } from "react";
import { AppState } from "react-native";
import { isSampleData } from "../data/source";
import { turnOnNotifications } from "./notifications";
import { setNotifyPrefs, useNotifyPrefs } from "./store";

export function useNotifySettings() {
  const prefs = useNotifyPrefs();
  const [denied, setDenied] = useState(false);
  const sample = isSampleData();

  useEffect(() => {
    const read = () =>
      Notifications.getPermissionsAsync().then(
        (p) => setDenied(!p.granted && (prefs.enabled || p.status === "denied")),
        // The page keeps its last state when the system does not answer.
        () => {},
      );
    void read();
    const sub = AppState.addEventListener("change", (s) => s === "active" && void read());
    return () => sub.remove();
  }, [prefs.enabled]);

  const toggleMaster = async () => {
    if (prefs.enabled) return setNotifyPrefs({ enabled: false });
    // Sample mode has no account, so there is nothing to notify about.
    if (sample) return;
    const granted = await turnOnNotifications().catch((e) => {
      console.warn("notifications not turned on", e);
      return null;
    });
    // An error is not a refusal. Only a refusal shows the blocked note.
    if (granted !== null) setDenied(!granted);
  };
  return { prefs, denied, sample, toggleMaster, setKind: setNotifyPrefs };
}

export const DENIED_NOTE =
  "The phone blocks notifications for this app. Allow them in the phone settings.";
export const SAMPLE_NOTE = "Notifications need an account. Sign in to turn them on.";
export const LOCAL_NOTE =
  "Notifications are made on this phone. The app sends no data to a push server.";
