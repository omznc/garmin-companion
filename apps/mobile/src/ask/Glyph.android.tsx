import MaterialIcons from "@expo/vector-icons/MaterialIcons";
import type { ColorValue } from "react-native";

export type GlyphName = "send" | "stop" | "check" | "close" | "tool" | "expand" | "collapse";

const NAMES = {
  send: "arrow-upward",
  stop: "stop",
  check: "check",
  close: "close",
  tool: "build",
  expand: "add",
  collapse: "remove",
} as const;

export function Glyph({ name, size, color }: { name: GlyphName; size: number; color: ColorValue }) {
  return <MaterialIcons name={NAMES[name]} size={size} color={color as string} />;
}
