// A row of thin bars. It draws a sparkline and the nights of a sleep window.
// A day with no reading (null) is a hairline, not a zero.
import { type ColorValue, View } from "react-native";

type Props = {
  values: (number | null)[];
  height: number;
  color: ColorValue;
  trackColor: ColorValue;
  // The value that fills a bar. The default is the largest value.
  max?: number;
  gap?: number;
  // A bar can take another colour, for example a short night.
  colorFor?: (value: number, index: number) => ColorValue;
  // A spoken label for a bar. The bars are not otherwise reachable by touch.
  describe?: (value: number | null, index: number) => string;
};

export function MiniBars({
  values,
  height,
  color,
  trackColor,
  max,
  gap = 2,
  colorFor,
  describe,
}: Props) {
  const top = max ?? Math.max(...values.map((v) => v ?? 0), 1);
  return (
    <View
      style={{
        flexDirection: "row",
        alignItems: "flex-end",
        height,
        gap,
        paddingHorizontal: 16,
      }}
    >
      {values.map((v, i) => (
        <View
          // biome-ignore lint/suspicious/noArrayIndexKey: the bars never reorder
          key={i}
          accessible={describe !== undefined}
          accessibilityLabel={describe?.(v, i)}
          style={{
            flex: 1,
            height: v === null ? 1 : Math.max((v / top) * height, 3),
            borderRadius: 2,
            backgroundColor: v === null ? trackColor : (colorFor?.(v, i) ?? color),
          }}
        />
      ))}
    </View>
  );
}
