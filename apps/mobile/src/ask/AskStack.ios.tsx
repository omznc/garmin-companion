// The Ask stack on iOS: a plain title for a chat, and the large title for the history.
import { Stack } from "expo-router";
import { useReduceMotion } from "../theme/motion";

export function AskStack() {
  // With reduce motion on, a push fades in place of the slide, as in Farmer.
  const reduceMotion = useReduceMotion();
  return (
    <Stack screenOptions={{ animation: reduceMotion ? "fade" : "default" }}>
      <Stack.Screen name="index" options={{ title: "Ask" }} />
      <Stack.Screen name="history" options={{ title: "Chats", headerLargeTitle: true }} />
    </Stack>
  );
}
