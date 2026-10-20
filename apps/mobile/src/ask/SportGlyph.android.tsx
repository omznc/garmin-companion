import MaterialIcons from "@expo/vector-icons/MaterialIcons";
import type { ColorValue } from "react-native";
import type { DraftSport } from "../data/chatTypes";

const NAMES = {
  running: "directions-run",
  cycling: "directions-bike",
  cardio: "favorite",
  strength_training: "fitness-center",
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
  return <MaterialIcons name={NAMES[sport]} size={size} color={color as string} />;
}
