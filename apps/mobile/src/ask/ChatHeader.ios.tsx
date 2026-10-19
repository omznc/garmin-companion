import { Stack } from "expo-router";
import { useChatTheme } from "./theme";

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
  const { text } = useChatTheme();
  return (
    <>
      {/* The system label colour follows light and dark, so the title stays readable. */}
      <Stack.Screen options={{ title, headerTintColor: text, headerTitleStyle: { color: text } }} />
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
