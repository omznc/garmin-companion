import { Host } from "@expo/ui";
import { LoadingIndicator } from "@expo/ui/jetpack-compose";
import { size as sizeModifier } from "@expo/ui/jetpack-compose/modifiers";
import type { ColorValue } from "react-native";
import { View } from "react-native";
import { useHostTheme } from "../theme/md";

// The Material 3 Expressive loading indicator: a shape that morphs. It turns
// on its own, even with reduced motion, because it is the information.
export function Spinner({
  size = 24,
  color,
  label,
}: {
  size?: number;
  color: ColorValue;
  label: string;
}) {
  const host = useHostTheme();
  return (
    <View
      accessible
      accessibilityRole="progressbar"
      accessibilityLabel={label}
      style={{ width: size, height: size }}
    >
      <Host style={{ width: size, height: size }} {...host}>
        <LoadingIndicator color={color} modifiers={[sizeModifier(size, size)]} />
      </Host>
    </View>
  );
}
