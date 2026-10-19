import { Text, View } from "react-native";
import { useChatTheme } from "./theme";

// One thing the person said: a right-aligned bubble.
export function UserMessage({ text }: { text: string }) {
  const theme = useChatTheme();
  return (
    <View style={{ alignItems: "flex-end" }}>
      <View
        style={{
          maxWidth: "85%",
          borderRadius: 20,
          paddingHorizontal: 16,
          paddingVertical: 12,
          backgroundColor: theme.bubble,
        }}
      >
        <Text selectable style={{ color: theme.onBubble, fontSize: 16, lineHeight: 24 }}>
          {text}
        </Text>
      </View>
    </View>
  );
}
