// Ask on Android: Material 3 colours, a plain title bar, the native text input.
import { Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { AskView } from "../components/AskView";
import { useMd } from "../theme/md";

const BAR = 88;

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
          paddingHorizontal: 20,
          paddingBottom: 12,
          backgroundColor: md.surface,
        }}
      >
        <Text
          accessibilityRole="header"
          style={{ color: md.onSurface, fontSize: 34, lineHeight: 42, fontWeight: "500" }}
        >
          Ask
        </Text>
      </View>
      <AskView
        top={0}
        theme={{
          chip: md.secondaryContainer,
          bg: md.surface,
          text: md.onSurface,
          muted: md.onSurfaceVariant,
          faint: md.outline,
          accent: md.primary,
          onAccent: md.onPrimary,
          bubble: md.secondaryContainer,
          field: md.surfaceContainerHighest,
          line: md.outlineVariant,
          warn: md.error,
        }}
      />
    </View>
  );
}
