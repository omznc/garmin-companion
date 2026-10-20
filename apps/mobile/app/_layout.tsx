import { focusManager, QueryClientProvider } from "@tanstack/react-query";
import { DarkTheme, DefaultTheme, Stack, ThemeProvider } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect } from "react";
import { AppState, useColorScheme } from "react-native";
import { getStatus, historyPending } from "../src/data/account";
import { queryClient } from "../src/data/queryClient";
import { importHistory } from "../src/data/useSync";
import "../src/theme/prefs";
import { useReduceMotion } from "../src/theme/motion";
import { useRootColor } from "../src/theme/useRootColor";
import { UpdateBanner } from "../src/update/UpdateBanner";

export default function RootLayout() {
  const root = useRootColor();
  const dark = useColorScheme() === "dark";
  const reduceMotion = useReduceMotion();
  // Refetch Today when the app returns to the foreground.
  useEffect(() => {
    const sub = AppState.addEventListener("change", (status) =>
      focusManager.setFocused(status === "active"),
    );
    return () => sub.remove();
  }, []);

  // The app stopped during the history import: take it up again.
  useEffect(() => {
    if (!historyPending()) return;
    getStatus().then(
      (s) => {
        if (s.connected) importHistory(false);
      },
      () => {},
    );
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      {/* The native header takes its text colours from this theme. */}
      <ThemeProvider value={dark ? DarkTheme : DefaultTheme}>
        <StatusBar style="auto" />
        <Stack
          screenOptions={{
            headerShown: false,
            contentStyle: { backgroundColor: root },
            animation: reduceMotion ? "fade" : "default",
          }}
        >
          <Stack.Screen name="(tabs)" />
          <Stack.Screen name="ask" />
          <Stack.Screen name="setup" options={{ gestureEnabled: false }} />
          <Stack.Screen name="sign-in" options={{ headerShown: true, title: "Sign in" }} />
        </Stack>
        <UpdateBanner />
      </ThemeProvider>
    </QueryClientProvider>
  );
}
