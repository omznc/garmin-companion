import { focusManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Stack } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect } from "react";
import { AppState } from "react-native";
import { applyTheme, getSettings } from "../src/data/account";

const queryClient = new QueryClient();

export default function RootLayout() {
  // Refetch Today when the app returns to the foreground.
  useEffect(() => {
    const sub = AppState.addEventListener("change", (status) =>
      focusManager.setFocused(status === "active"),
    );
    return () => sub.remove();
  }, []);

  // Apply the saved theme at start. A failed read keeps the phone setting.
  useEffect(() => {
    getSettings()
      .then((s) => applyTheme(s.theme))
      .catch(() => {});
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <StatusBar style="auto" />
      <Stack screenOptions={{ headerShown: false }}>
        <Stack.Screen name="(tabs)" />
        <Stack.Screen name="setup" options={{ gestureEnabled: false }} />
        <Stack.Screen name="sign-in" options={{ headerShown: true, title: "Sign in" }} />
      </Stack>
    </QueryClientProvider>
  );
}
