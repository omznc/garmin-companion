// One stack per tab. It gives the native large title.
// The header uses the default look, so it follows light and dark mode.
import { Stack } from "expo-router";

export function TabStack({ title }: { title: string }) {
  return (
    <Stack screenOptions={{ headerLargeTitle: true }}>
      <Stack.Screen name="index" options={{ title }} />
    </Stack>
  );
}
