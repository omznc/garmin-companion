// A small bar trend for Weight and Food. It scales from the lowest value, so a change shows.
// The screen passes the colour and a spoken label that names both ends.
import { View } from "react-native";

const HEIGHT = 96;
const MAX_BARS = 48;

// shortcut: keep every nth value above 48 values, the chart is a shape and not a reading.
function thin(values: number[]): number[] {
  if (values.length <= MAX_BARS) return values;
  const step = values.length / MAX_BARS;
  return Array.from({ length: MAX_BARS }, (_, i) => values[Math.floor(i * step)] ?? 0);
}

export function TrendBars({
  values,
  color,
  label,
}: {
  values: number[];
  color: string;
  label: string;
}) {
  const bars = thin(values);
  const low = Math.min(...bars);
  const span = Math.max(Math.max(...bars) - low, 1e-6);
  return (
    <View
      accessible
      accessibilityLabel={label}
      style={{ flexDirection: "row", alignItems: "flex-end", height: HEIGHT, gap: 2 }}
    >
      {bars.map((v, i) => (
        <View
          // biome-ignore lint/suspicious/noArrayIndexKey: the bars never reorder
          key={i}
          style={{
            flex: 1,
            height: 12 + ((v - low) / span) * (HEIGHT - 12),
            borderRadius: 2,
            backgroundColor: color,
          }}
        />
      ))}
    </View>
  );
}
