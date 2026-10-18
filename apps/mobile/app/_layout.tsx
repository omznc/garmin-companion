import { focusManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Stack } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect } from "react";
import { AppState, Platform } from "react-native";

const queryClient = new QueryClient();

export default function RootLayout() {
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
      <Stack screenOptions={{ headerShown: false }}>
        <Stack.Screen name="(tabs)" />
        <Stack.Screen
          name="settings"
          options={{
            presentation: Platform.OS === "ios" ? "modal" : "card",
            headerShown: Platform.OS === "ios",
            title: "Settings",
          }}
        />
      </Stack>
    </QueryClientProvider>
  );
}
