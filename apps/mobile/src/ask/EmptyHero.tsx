import { Platform, Pressable, Text, View } from "react-native";
import { useChatTheme } from "./theme";

// The opening of an empty chat: a title, a line, and outlined question chips.
// A chip sends its question at once, as the Farmer chips do.
export function EmptyHero({
  title = "Ask about your training",
  body,
  suggestions,
  onPick,
  locked = false,
}: {
  title?: string;
  body: string;
  suggestions: string[];
  /** Called with the question of the chip that the person pressed. */
  onPick: (question: string) => void;
  /** Another chat answers, so the chips are off. */
  locked?: boolean;
}) {
  const theme = useChatTheme();
  return (
    <View style={{ gap: 12, paddingTop: 24 }}>
      <Text accessibilityRole="header" style={{ color: theme.text, fontSize: 24, lineHeight: 32 }}>
        {title}
      </Text>
      <Text style={{ color: theme.muted, fontSize: 16, lineHeight: 24 }}>{body}</Text>
      <View style={{ gap: 8, alignItems: "flex-start" }}>
        {suggestions.map((question) => (
          <Pressable
            key={question}
            accessibilityRole="button"
            disabled={locked}
            accessibilityState={{ disabled: locked }}
            onPress={() => onPick(question)}
            android_ripple={{ color: theme.line }}
            style={({ pressed }) => ({
              borderRadius: 18,
              borderWidth: 1,
              borderColor: theme.line,
              paddingHorizontal: 12,
              paddingVertical: 8,
              backgroundColor: Platform.OS === "ios" && pressed ? theme.pressed : undefined,
              opacity: locked ? 0.4 : 1,
            })}
          >
            <Text style={{ color: theme.text, fontSize: 14, lineHeight: 20 }}>{question}</Text>
          </Pressable>
        ))}
      </View>
    </View>
  );
}
