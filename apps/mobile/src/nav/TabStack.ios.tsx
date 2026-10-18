// One stack per tab. It gives the native large title and the Settings bar button.
// The header uses the default look, so it follows light and dark mode.
import { Stack, useRouter } from "expo-router";
import { SymbolView } from "expo-symbols";
import { PlatformColor, Pressable } from "react-native";

function SettingsBarButton() {
  const router = useRouter();
  return (
    <Pressable accessibilityLabel="Settings" hitSlop={8} onPress={() => router.push("/settings")}>
      <SymbolView name="person.crop.circle" size={26} tintColor={PlatformColor("label")} />
    </Pressable>
  );
}

export function TabStack({ title }: { title: string }) {
  return (
    <Stack screenOptions={{ headerLargeTitle: true, headerRight: SettingsBarButton }}>
      <Stack.Screen name="index" options={{ title }} />
    </Stack>
  );
}
