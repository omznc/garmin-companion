// One stack per tab. On iOS it gives the native large title and the Settings bar button.
// On Android the screen draws its own top app bar, so the header is off.
import { Stack, useRouter } from "expo-router";
import { Platform, PlatformColor, Pressable } from "react-native";
import { SymbolView } from "expo-symbols";

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
    <Stack
      screenOptions={{
        headerShown: Platform.OS === "ios",
        headerLargeTitle: true,
        headerLargeTitleShadowVisible: false,
        headerTransparent: true,
        headerBlurEffect: "systemMaterial",
        headerRight: SettingsBarButton,
      }}
    >
      <Stack.Screen name="index" options={{ title }} />
    </Stack>
  );
}
