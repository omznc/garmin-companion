// One stack per tab. The screen draws its own top app bar, so the header is off.
import { Stack } from "expo-router";

export function TabStack({ title }: { title: string }) {
  return (
    <Stack screenOptions={{ headerShown: false }}>
      <Stack.Screen name="index" options={{ title }} />
    </Stack>
  );
}
