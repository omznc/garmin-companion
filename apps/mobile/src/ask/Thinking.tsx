import { Text, View } from "react-native";
import { Spinner } from "./Spinner";
import { useChatTheme } from "./theme";

// The typing indicator: a spinner and the word "Thinking".
export function Thinking() {
  const theme = useChatTheme();
  return (
    <View
      accessible
      accessibilityLabel="Thinking"
      style={{ flexDirection: "row", gap: 8, alignItems: "center" }}
    >
      <Spinner size={24} color={theme.muted} label="Thinking" />
      <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 20 }}>Thinking</Text>
    </View>
  );
}
