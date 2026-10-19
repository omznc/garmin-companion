import { useSyncExternalStore } from "react";
import { AccessibilityInfo } from "react-native";

// The phone's "Reduce motion" (iOS) or "Remove animations" (Android).
// Read once when the module loads, then kept current by the system event.
let systemReduceMotion = false;
const listeners = new Set<() => void>();

const setSystem = (value: boolean) => {
  if (value === systemReduceMotion) return;
  systemReduceMotion = value;
  for (const listener of listeners) listener();
};

AccessibilityInfo.isReduceMotionEnabled()
  .then(setSystem)
  .catch(() => {
    // No answer from the phone: motion stays on.
  });
AccessibilityInfo.addEventListener("reduceMotionChanged", setSystem);

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};

/** True when nothing on screen may move. A spinner keeps turning, because it
 *  is information. A change of colour or opacity is still fine. */
export function useReduceMotion(): boolean {
  return useSyncExternalStore(subscribe, () => systemReduceMotion);
}
