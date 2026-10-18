// One stack per tab. It gives the native large title.
// The header uses the default look, so it follows light and dark mode.
import { Stack } from "expo-router";
import type { ReactNode } from "react";

export function TabStack({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <Stack screenOptions={{ headerLargeTitle: true }}>
      <Stack.Screen name="index" options={{ title }} />
      {children}
    </Stack>
  );
}
