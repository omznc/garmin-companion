import { focusManager, QueryClientProvider } from "@tanstack/react-query";
import { DarkTheme, DefaultTheme, Stack, ThemeProvider } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect } from "react";
import { AppState, useColorScheme } from "react-native";
import { getStatus, historyPending } from "../src/data/account";
import { queryClient } from "../src/data/queryClient";
import { importHistory, resumeCutOff, syncIfStale } from "../src/data/useSync";
import { syncTabPrefs } from "../src/nav/tabPrefs";
import { useSystemBack } from "../src/nav/useSystemBack";
import { registerBackgroundSync } from "../src/notify/backgroundSync";
import { configureNotifications } from "../src/notify/notifications";
import { useNotificationTap } from "../src/notify/useNotificationTap";
import "../src/theme/prefs";
import { useReduceMotion } from "../src/theme/motion";
import { useRootColor } from "../src/theme/useRootColor";
import { DialogHost } from "../src/ui/DialogHost";
import { SnackbarHost } from "../src/ui/SnackbarHost";
import { UpdateBanner } from "../src/update/UpdateBanner";

configureNotifications();

export default function RootLayout() {
  const root = useRootColor();
  const dark = useColorScheme() === "dark";
  const reduceMotion = useReduceMotion();
  useSystemBack();
  useNotificationTap();
  useEffect(() => {
    registerBackgroundSync().catch((e) => console.warn("background sync not registered", e));
  }, []);
  // Refetch stale data when the app returns to the foreground. A quick sync runs if the last one is old.
  useEffect(() => {
    void syncIfStale();
    const sub = AppState.addEventListener("change", (status) => {
      focusManager.setFocused(status === "active");
      if (status !== "active") return;
      resumeCutOff();
      void syncIfStale();
    });
    return () => sub.remove();
  }, []);

  // The core keeps the tab order. Take its value if the local copy differs.
  useEffect(() => {
    void syncTabPrefs();
  }, []);

  // The app stopped during the history import: take it up again.
  useEffect(() => {
    if (!historyPending()) return;
    getStatus().then(
      (s) => {
        if (s.connected) importHistory();
      },
      // The marker stays, so the next start tries again.
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
        <SnackbarHost />
        <DialogHost />
      </ThemeProvider>
    </QueryClientProvider>
  );
}
