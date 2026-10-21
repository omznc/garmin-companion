import * as Haptics from "expo-haptics";
import { getThemePrefs } from "../theme/prefs";

// A tap must never fail an action, so each call drops its own error.
const ignore = () => undefined;

// The one gate: the "No vibration" setting stops every tap.
const tap = (play: () => Promise<void>) => {
  if (getThemePrefs().noVibration) return;
  void play().catch(ignore);
};

/** A tap of success after a change that the person asked for: a saved workout, a finished sync. */
export const committed = () => {
  tap(() => Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success));
};

/** A light tap when the person picks an option. */
export const toggled = () => {
  tap(() => Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light));
};

/** A firm tap when the person deletes or disconnects something. */
export const removed = () => {
  tap(() => Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium));
};
