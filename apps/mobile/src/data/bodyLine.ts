import type { BodyKind, FoodView, StrengthView, WeightView } from "./body";

export const DASH = "—";
export const whole = (v: number | null) =>
  v == null ? DASH : Math.round(v).toLocaleString("en-US");
export const one = (v: number | null) => (v == null ? DASH : v.toFixed(1));

/** One short value for a list row: "80.9 kg, down 0.7 kg", "1,800 kcal a day", "6 sessions". */
export function bodyLine(kind: BodyKind, raw: WeightView | FoodView | StrengthView): string | null {
  if (raw.empty) return raw.empty.title;
  if (kind === "weight") {
    const v = raw as WeightView;
    if (v.trendKg == null) return null;
    const change =
      v.changeKg == null || v.changeKg === 0
        ? ""
        : `, ${v.changeKg > 0 ? "up" : "down"} ${Math.abs(v.changeKg).toFixed(1)} kg`;
    return `${one(v.trendKg)} kg${change}`;
  }
  if (kind === "food") {
    const v = raw as FoodView;
    return v.avgEatenKcal == null ? null : `${whole(v.avgEatenKcal)} kcal a day`;
  }
  return (raw as StrengthView).sessionsText;
}
