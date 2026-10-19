// The shape of a route and a wear bar, drawn with plain views, so no drawing library is needed.
// The core sends the shape as points in a unit square, north up, with true proportions.
import { type ColorValue, View } from "react-native";

const BOX = 56;
const PAD = 4;
// shortcut: thin the points to 48 and draw each segment as a rotated view, upgrade to SVG for a full map.
const MAX_POINTS = 48;

function thin(points: [number, number][]): [number, number][] {
  if (points.length <= MAX_POINTS) return points;
  const step = (points.length - 1) / (MAX_POINTS - 1);
  return Array.from(
    { length: MAX_POINTS },
    (_, i) => points[Math.round(i * step)] as [number, number],
  );
}

export function RouteTrace({ points, color }: { points: [number, number][]; color: ColorValue }) {
  const inner = BOX - PAD * 2;
  const at = ([x, y]: [number, number]) => [PAD + x * inner, PAD + y * inner] as const;
  const shape = thin(points).map(at);
  return (
    <View accessible accessibilityLabel="Shape of the route" style={{ width: BOX, height: BOX }}>
      {shape.slice(1).map(([x, y], i) => {
        const [px, py] = shape[i] as readonly [number, number];
        const length = Math.hypot(x - px, y - py);
        return (
          <View
            // biome-ignore lint/suspicious/noArrayIndexKey: the segments never reorder
            key={i}
            style={{
              position: "absolute",
              left: (x + px) / 2 - length / 2,
              top: (y + py) / 2 - 1,
              width: length + 1,
              height: 2,
              borderRadius: 1,
              backgroundColor: color,
              transform: [{ rotate: `${Math.atan2(y - py, x - px)}rad` }],
            }}
          />
        );
      })}
    </View>
  );
}

/** Wear against a limit. `fraction` runs from 0 to 1. */
export function WearBar({
  fraction,
  color,
  track,
}: {
  fraction: number;
  color: ColorValue;
  track: ColorValue;
}) {
  return (
    <View
      accessible
      accessibilityLabel={`${Math.round(fraction * 100)} percent of the limit`}
      style={{
        height: 4,
        borderRadius: 2,
        backgroundColor: track,
        marginHorizontal: 20,
        marginBottom: 14,
      }}
    >
      <View
        style={{
          width: `${Math.round(Math.min(Math.max(fraction, 0), 1) * 100)}%`,
          height: 4,
          borderRadius: 2,
          backgroundColor: color,
        }}
      />
    </View>
  );
}
