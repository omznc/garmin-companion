// One stack per tab. The screen draws its own top app bar, so the header is off.
import { Stack } from "expo-router";
import type { ReactNode } from "react";
import { useReduceMotion } from "../ask/motion";
import { useMd } from "../theme/md";

export function TabStack({ title, children }: { title: string; children?: ReactNode }) {
  const md = useMd();
  const reduceMotion = useReduceMotion();
  // The scene colour stops a white frame behind a screen as it slides in.
  // With "Remove animations" on, a push fades in place of the slide.
  return (
    <Stack
      screenOptions={{
        headerShown: false,
        contentStyle: { backgroundColor: md.surface },
        animation: reduceMotion ? "fade" : "default",
      }}
    >
      <Stack.Screen name="index" options={{ title }} />
      {children}
    </Stack>
  );
}
