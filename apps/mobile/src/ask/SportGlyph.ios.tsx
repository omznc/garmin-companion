import { SymbolView } from "expo-symbols";
import type { ColorValue } from "react-native";
import type { DraftSport } from "../data/chatTypes";

const NAMES = {
  running: "figure.run",
  cycling: "figure.outdoor.cycle",
  cardio: "heart.fill",
  strength_training: "dumbbell.fill",
} as const;

export function SportGlyph({
  sport,
  size,
  color,
}: {
  sport: DraftSport;
  size: number;
  color: ColorValue;
}) {
  return <SymbolView name={NAMES[sport]} size={size} tintColor={color} />;
}
