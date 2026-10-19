import { SymbolView } from "expo-symbols";
import type { ColorValue } from "react-native";

export type GlyphName = "send" | "stop" | "check" | "close" | "tool" | "expand" | "collapse";

const NAMES = {
  send: "arrow.up",
  stop: "stop.fill",
  check: "checkmark",
  close: "xmark",
  tool: "wrench.and.screwdriver",
  expand: "plus",
  collapse: "minus",
} as const;

export function Glyph({ name, size, color }: { name: GlyphName; size: number; color: ColorValue }) {
  return <SymbolView name={NAMES[name]} size={size} tintColor={color} />;
}
