// Ask on Android: Material 3 colours, a plain title bar, the native text input.
import { Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { AskView } from "../components/AskView";
import { useMd } from "../theme/md";

const BAR = 72;

export function Ask() {
  const md = useMd();
  const insets = useSafeAreaInsets();
  return (
    <View style={{ flex: 1, backgroundColor: md.surface }}>
      <View
        style={{
          paddingTop: insets.top,
          height: insets.top + BAR,
          justifyContent: "flex-end",
          paddingHorizontal: 16,
          paddingBottom: 12,
          backgroundColor: md.surface,
        }}
      >
        <Text
          accessibilityRole="header"
          style={{ color: md.onSurface, fontSize: 28, lineHeight: 36, fontWeight: "400" }}
        >
          Ask
        </Text>
      </View>
      <AskView
        top={0}
        theme={{
          bg: md.surface,
          text: md.onSurface,
          muted: md.onSurfaceVariant,
          faint: md.outline,
          accent: md.primary,
          onAccent: md.onPrimary,
          bubble: md.surfaceContainerHigh,
          field: md.surfaceContainerHighest,
          line: md.outlineVariant,
          warn: md.error,
        }}
      />
    </View>
  );
}
