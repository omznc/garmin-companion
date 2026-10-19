import { useEffect, useState } from "react";
import { Animated } from "react-native";
import { useReduceMotion } from "./motion";
import { useChatTheme } from "./theme";

// A block that pulses while content loads. With less motion it stays still.
export function Skeleton({
  height,
  width = "100%",
  radius = 12,
}: {
  height: number;
  width?: number | `${number}%`;
  radius?: number;
}) {
  const theme = useChatTheme();
  const reduceMotion = useReduceMotion();
  const [opacity] = useState(() => new Animated.Value(0.5));
  useEffect(() => {
    if (reduceMotion) {
      opacity.setValue(0.75);
      return;
    }
    const loop = Animated.loop(
      Animated.sequence([
        Animated.timing(opacity, { toValue: 1, duration: 700, useNativeDriver: true }),
        Animated.timing(opacity, { toValue: 0.5, duration: 700, useNativeDriver: true }),
      ]),
    );
    loop.start();
    return () => loop.stop();
  }, [opacity, reduceMotion]);
  return (
    <Animated.View
      accessibilityLabel="Loading"
      style={{ height, width, borderRadius: radius, backgroundColor: theme.field, opacity }}
    />
  );
}
