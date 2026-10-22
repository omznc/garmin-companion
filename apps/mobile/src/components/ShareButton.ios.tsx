// The share button of a header on iOS: the system share symbol, in the tint colour.
import { SymbolView } from "expo-symbols";
import { PlatformColor, Pressable } from "react-native";

export function ShareButton({ label, onPress }: { label: string; onPress: () => void }) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      hitSlop={10}
      onPress={onPress}
      style={({ pressed }) => ({ opacity: pressed ? 0.5 : 1 })}
    >
      <SymbolView name="square.and.arrow.up" size={22} tintColor={PlatformColor("link")} />
    </Pressable>
  );
}
