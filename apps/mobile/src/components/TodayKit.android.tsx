// Material 3 Expressive pieces of Today: a cookie shape for the key figure,
// vital tiles with an icon, and the sport icon of the last session.
import { useEffect } from "react";
import { View } from "react-native";
import Animated, {
  cancelAnimation,
  Easing,
  useAnimatedStyle,
  useSharedValue,
  withRepeat,
  withTiming,
} from "react-native-reanimated";
import type { Metric } from "../data/types";
import { useMd } from "../theme/md";
import { GAP, type IconName, IconTile, T, toneColors } from "./MdKit";

/**
 * A 12-point cookie, from three rounded squares turned by 30 degrees each.
 * It turns slowly. The children stay upright.
 */
export function Cookie({
  size,
  color,
  children,
}: {
  size: number;
  color: string;
  children: React.ReactNode;
}) {
  const turn = useSharedValue(0);
  useEffect(() => {
    turn.value = withRepeat(withTiming(360, { duration: 48000, easing: Easing.linear }), -1);
    return () => cancelAnimation(turn);
  }, [turn]);
  const spin = useAnimatedStyle(() => ({ transform: [{ rotate: `${turn.value}deg` }] }));
  const side = size * 0.74;
  const square = (deg: number) => (
    <View
      key={deg}
      style={{
        position: "absolute",
        width: side,
        height: side,
        borderRadius: side * 0.3,
        backgroundColor: color,
        transform: [{ rotate: `${deg}deg` }],
      }}
    />
  );
  return (
    <View style={{ width: size, height: size, alignItems: "center", justifyContent: "center" }}>
      <Animated.View
        style={[
          { position: "absolute", width: size, height: size },
          { alignItems: "center", justifyContent: "center" },
          spin,
        ]}
      >
        {[0, 30, 60].map(square)}
      </Animated.View>
      {children}
    </View>
  );
}

const VITAL_ICONS: Record<string, IconName> = {
  sleep: "bedtime",
  hrv: "monitor-heart",
  rhr: "favorite",
  battery: "battery-charging-full",
  readiness: "bolt",
  steps: "directions-walk",
  stress: "spa",
};

/** Vitals in rows of two. Each tile has an icon and an emphasized figure. A lone tile is wide. */
export function VitalGrid({ metrics }: { metrics: Metric[] }) {
  const rows: Metric[][] = [];
  for (let i = 0; i < metrics.length; i += 2) rows.push(metrics.slice(i, i + 2));
  return (
    <View style={{ gap: GAP }}>
      {rows.map((row, r) => (
        <View key={row[0]?.id} style={{ flexDirection: "row", gap: GAP }}>
          {row.map((m, c) => (
            <Vital key={m.id} m={m} lean={(r + c) % 2 === 0 ? "left" : "right"} />
          ))}
        </View>
      ))}
    </View>
  );
}

function Vital({ m, lean }: { m: Metric; lean: "left" | "right" }) {
  const md = useMd();
  const c = toneColors(md, m.accent ? "tertiary" : "high");
  // The big corner leans one way and then the other, so the grid does not look stamped.
  const big = 36;
  const small = 14;
  return (
    <View
      style={{
        flex: 1,
        minHeight: 132,
        padding: 16,
        gap: 14,
        justifyContent: "space-between",
        backgroundColor: c.bg,
        borderTopLeftRadius: lean === "left" ? big : small,
        borderBottomRightRadius: lean === "left" ? big : small,
        borderTopRightRadius: lean === "left" ? small : big,
        borderBottomLeftRadius: lean === "left" ? small : big,
      }}
    >
      <IconTile name={VITAL_ICONS[m.id] ?? "insights"} tone={m.accent ? "tertiary" : "primary"} />
      <View>
        <View style={{ flexDirection: "row", alignItems: "baseline", gap: 4 }}>
          <T v="headline" color={c.fg}>
            {m.value}
          </T>
          {m.unit ? (
            <T v="labelSmall" color={c.quiet}>
              {m.unit}
            </T>
          ) : null}
        </View>
        <T v="labelSmall" color={c.quiet}>
          {m.label}
        </T>
      </View>
    </View>
  );
}

const SPORT_ICONS: [RegExp, IconName][] = [
  [/run/, "directions-run"],
  [/cycl|bik/, "directions-bike"],
  [/hik/, "hiking"],
  [/walk/, "directions-walk"],
  [/swim/, "pool"],
  [/strength|gym/, "fitness-center"],
  [/golf/, "golf-course"],
];

/** The icon of a Garmin activity type. Unknown types get a general sport icon. */
export function sportIcon(typeKey: string | null): IconName {
  const k = (typeKey ?? "").toLowerCase();
  return SPORT_ICONS.find(([re]) => re.test(k))?.[1] ?? "sports";
}
