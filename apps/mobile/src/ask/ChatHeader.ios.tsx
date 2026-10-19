import { Stack } from "expo-router";

// The title and the history button in the native header. The system draws the back button.
export function ChatHeader({
  title,
  onHistory,
}: {
  title: string;
  /** The system draws the back button, so this stays unused. */
  back?: boolean;
  onHistory: () => void;
}) {
  return (
    <>
      <Stack.Screen options={{ title }} />
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Button
          icon="clock.arrow.circlepath"
          accessibilityLabel="Chat history"
          onPress={onHistory}
        />
      </Stack.Toolbar>
    </>
  );
}
