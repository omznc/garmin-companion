// One stack per tab. The screen draws its own top app bar, so the header is off.
import { Stack } from "expo-router";
import type { ReactNode } from "react";

export function TabStack({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <Stack screenOptions={{ headerShown: false }}>
      <Stack.Screen name="index" options={{ title }} />
      {children}
    </Stack>
  );
}
