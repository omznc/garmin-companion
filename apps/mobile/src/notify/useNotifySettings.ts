// The state of the Notifications page. The master switch asks for the system permission when the
// person turns it on, and not before.
import { useState } from "react";
import { turnOnNotifications } from "./notifications";
import { setNotifyPrefs, useNotifyPrefs } from "./store";

export function useNotifySettings() {
  const prefs = useNotifyPrefs();
  const [denied, setDenied] = useState(false);
  const toggleMaster = async () => {
    if (prefs.enabled) return setNotifyPrefs({ enabled: false });
    const granted = await turnOnNotifications().catch(() => false);
    setDenied(!granted);
  };
  return { prefs, denied, toggleMaster, setKind: setNotifyPrefs };
}

export const DENIED_NOTE =
  "The phone blocks notifications for this app. Allow them in the phone settings.";
export const LOCAL_NOTE =
  "Notifications are made on this phone. The app sends no data to a push server.";
