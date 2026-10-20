import { useEffect, useState } from "react";
import { AccessibilityInfo, Animated, Easing, Text } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useReduceMotion } from "../theme/motion";
import type { Notice } from "./snackbar-store";

const SHOW_MS = 4000;
const ENTER_MS = 200;
const EXIT_MS = 150;
// The bar sits above the tab bar.
const LIFT = 88;
const RISE = 16;

export type SnackbarColors = { background: string; ink: string };

// A short message at the bottom of the screen. A screen reader reads it out.
// With reduced motion the bar fades and does not move.
export function Snackbar({
  notice,
  onHide,
  colors,
  radius,
}: {
  notice: Notice | null;
  onHide: () => void;
  colors: SnackbarColors;
  radius: number;
}) {
  const reduceMotion = useReduceMotion();
  const { bottom } = useSafeAreaInsets();
  const [progress] = useState(() => new Animated.Value(0));
  // The notice on screen. It stays while the bar exits, after `notice` is null.
  const [shown, setShown] = useState<Notice | null>(notice);

  useEffect(() => {
    if (!notice) return;
    AccessibilityInfo.announceForAccessibility(notice.message);
    const timer = setTimeout(onHide, SHOW_MS);
    return () => clearTimeout(timer);
  }, [notice, onHide]);

  useEffect(() => {
    if (notice) setShown(notice);
    const animation = Animated.timing(progress, {
      toValue: notice ? 1 : 0,
      duration: notice ? ENTER_MS : EXIT_MS,
      easing: Easing.out(Easing.cubic),
      useNativeDriver: true,
    });
    animation.start(({ finished }) => {
      if (finished && !notice) setShown(null);
    });
    return () => animation.stop();
  }, [notice, progress]);

  if (!shown) return null;
  return (
    <Animated.View
      accessibilityLiveRegion="polite"
      pointerEvents="none"
      style={{
        position: "absolute",
        left: 16,
        right: 16,
        bottom: bottom + LIFT,
        minHeight: 48,
        justifyContent: "center",
        paddingHorizontal: 16,
        paddingVertical: 12,
        borderRadius: radius,
        backgroundColor: colors.background,
        elevation: 3,
        opacity: progress,
        transform: [
          {
            translateY: progress.interpolate({
              inputRange: [0, 1],
              outputRange: [reduceMotion ? 0 : RISE, 0],
            }),
          },
        ],
      }}
    >
      <Text style={{ color: colors.ink, fontSize: 14, lineHeight: 20 }}>{shown.message}</Text>
    </Animated.View>
  );
}
