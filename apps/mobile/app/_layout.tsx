import { focusManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Stack } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect } from "react";
import { AppState } from "react-native";
import "../src/theme/prefs";
import { useRootColor } from "../src/theme/useRootColor";
import { UpdateBanner } from "../src/update/UpdateBanner";

const queryClient = new QueryClient();

export default function RootLayout() {
  const root = useRootColor();
  // Refetch Today when the app returns to the foreground.
  useEffect(() => {
    const sub = AppState.addEventListener("change", (status) =>
      focusManager.setFocused(status === "active"),
    );
    return () => sub.remove();
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <StatusBar style="auto" />
      <Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: root } }}>
        <Stack.Screen name="(tabs)" />
        <Stack.Screen name="setup" options={{ gestureEnabled: false }} />
        <Stack.Screen name="sign-in" options={{ headerShown: true, title: "Sign in" }} />
      </Stack>
      <UpdateBanner />
    </QueryClientProvider>
  );
}
