// The Ask stack on iOS: a plain title for a chat, and the large title for the history.
import { Stack } from "expo-router";

export function AskStack() {
  return (
    <Stack>
      <Stack.Screen name="index" options={{ title: "Ask" }} />
      <Stack.Screen name="history" options={{ title: "Chats", headerLargeTitle: true }} />
    </Stack>
  );
}
