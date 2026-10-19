// One stack per tab. The screen draws its own top app bar, so the header is off.
import { Stack } from "expo-router";
import type { ReactNode } from "react";
import { useMd } from "../theme/md";

export function TabStack({ title, children }: { title: string; children?: ReactNode }) {
  const md = useMd();
  // The scene colour stops a white frame behind a screen as it slides in.
  return (
    <Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: md.surface } }}>
      <Stack.Screen name="index" options={{ title }} />
      {children}
    </Stack>
  );
}
