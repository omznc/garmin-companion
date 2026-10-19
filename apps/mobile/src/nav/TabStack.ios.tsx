// One stack per tab. It gives the native large title.
// The header uses the default look, so it follows light and dark mode.
import { Stack } from "expo-router";
import type { ReactNode } from "react";
import { useReduceMotion } from "../theme/motion";

export function TabStack({ title, children }: { title: string; children?: ReactNode }) {
  // With reduce motion on, a push fades in place of the slide.
  const reduceMotion = useReduceMotion();
  return (
    <Stack screenOptions={{ headerLargeTitle: true, animation: reduceMotion ? "fade" : "default" }}>
      <Stack.Screen name="index" options={{ title }} />
      {children}
    </Stack>
  );
}
