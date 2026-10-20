import * as Haptics from "expo-haptics";

// A tap must never fail an action, so each call drops its own error.
const ignore = () => undefined;

/** A tap of success after a change that the person asked for: a saved workout, a finished sync. */
export const committed = () => {
  void Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success).catch(ignore);
};

/** A light tap when the person picks an option. */
export const toggled = () => {
  void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(ignore);
};

/** A firm tap when the person deletes or disconnects something. */
export const removed = () => {
  void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium).catch(ignore);
};
