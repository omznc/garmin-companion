import { focusManager, QueryClientProvider } from "@tanstack/react-query";
import { DarkTheme, DefaultTheme, Stack, ThemeProvider } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect } from "react";
import { AppState, useColorScheme } from "react-native";
import { queryClient } from "../src/data/queryClient";
import "../src/theme/prefs";
import { useRootColor } from "../src/theme/useRootColor";
import { UpdateBanner } from "../src/update/UpdateBanner";

export default function RootLayout() {
  const root = useRootColor();
  const dark = useColorScheme() === "dark";
  // Refetch Today when the app returns to the foreground.
  useEffect(() => {
    const sub = AppState.addEventListener("change", (status) =>
      focusManager.setFocused(status === "active"),
    );
    return () => sub.remove();
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      {/* The native header takes its text colours from this theme. */}
      <ThemeProvider value={dark ? DarkTheme : DefaultTheme}>
        <StatusBar style="auto" />
        <Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: root } }}>
          <Stack.Screen name="(tabs)" />
          <Stack.Screen name="setup" options={{ gestureEnabled: false }} />
          <Stack.Screen name="sign-in" options={{ headerShown: true, title: "Sign in" }} />
        </Stack>
        <UpdateBanner />
      </ThemeProvider>
    </QueryClientProvider>
  );
}
