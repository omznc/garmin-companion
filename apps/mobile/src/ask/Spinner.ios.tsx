import { ActivityIndicator, type ColorValue } from "react-native";

export function Spinner({
  size = 24,
  color,
  label,
}: {
  size?: number;
  color: ColorValue;
  label: string;
}) {
  return (
    <ActivityIndicator
      size={size > 32 ? "large" : "small"}
      color={color}
      accessibilityLabel={label}
    />
  );
}
