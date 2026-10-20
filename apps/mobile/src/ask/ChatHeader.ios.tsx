import { Stack, useRouter } from "expo-router";

// The title and the history button in the native header. The system draws the back button,
// except on the first screen of the Ask stack: there the stack has no screen to go back to.
export function ChatHeader({
  title,
  first,
  onHistory,
}: {
  title: string;
  first: boolean;
  onHistory: () => void;
}) {
  const router = useRouter();
  return (
    <>
      <Stack.Screen options={{ title }} />
      {first ? (
        <Stack.Toolbar placement="left">
          <Stack.Toolbar.Button
            icon="chevron.left"
            accessibilityLabel="Back"
            onPress={() => router.back()}
          />
        </Stack.Toolbar>
      ) : null}
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
