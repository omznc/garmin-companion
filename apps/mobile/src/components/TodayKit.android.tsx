// Material 3 Expressive pieces of Today: a cookie shape for the key figure,
// vital tiles with an icon, and the sport icon of the last session.
import { useFocusEffect, useRouter } from "expo-router";
import { useCallback } from "react";
import { Pressable, Text, View } from "react-native";
import Animated, {
  cancelAnimation,
  Easing,
  useAnimatedStyle,
  useSharedValue,
  withRepeat,
  withTiming,
} from "react-native-reanimated";
import { metricRoute } from "../data/present";
import type { Metric } from "../data/types";
import { useMd } from "../theme/md";
import { useReduceMotion } from "../theme/motion";
import { GAP, type IconName, IconTile, type Md, T, toneColors } from "./MdKit";

/** A figure that stays on one line. A long value shrinks, it does not wrap. */
export function FitText({
  size,
  color,
  children,
}: {
  size: 24 | 32;
  color: string;
  children: string;
}) {
  return (
    <Text
      numberOfLines={1}
      adjustsFontSizeToFit
      style={{
        color,
        fontSize: size,
        lineHeight: size + 8,
        fontWeight: "600",
        fontVariant: ["tabular-nums"],
        flexShrink: 1,
      }}
    >
      {children}
    </Text>
  );
}

/**
 * A 12-point cookie, from three rounded squares turned by 30 degrees each.
 * It turns slowly while Today is on screen. The children stay upright.
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
  const reduceMotion = useReduceMotion();
  const turn = useSharedValue(0);
  // A tab stays mounted when another is open, and a turn left running there
  // still sends a prop update every frame. It goes on from where it stopped.
  useFocusEffect(
    useCallback(() => {
      if (reduceMotion) return;
      const from = turn.value % 360;
      turn.value = from;
      turn.value = withRepeat(
        withTiming(from + 360, { duration: 48000, easing: Easing.linear }),
        -1,
      );
      return () => cancelAnimation(turn);
    }, [turn, reduceMotion]),
  );
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
  const router = useRouter();
  const c = toneColors(md, m.accent ? "tertiary" : "high");
  // The big corner leans one way and then the other, so the grid does not look stamped.
  const big = 36;
  const small = 14;
  // A view does not clip its own ripple, so the parent View clips it to the corners.
  return (
    <View
      style={{
        flex: 1,
        overflow: "hidden",
        backgroundColor: c.bg,
        borderTopLeftRadius: lean === "left" ? big : small,
        borderBottomRightRadius: lean === "left" ? big : small,
        borderTopRightRadius: lean === "left" ? small : big,
        borderBottomLeftRadius: lean === "left" ? small : big,
      }}
    >
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={`${m.label}, ${m.unit ? `${m.value} ${m.unit}` : m.value}`}
        onPress={() => router.push(metricRoute(m.id))}
        android_ripple={{ color: `${c.fg}22` }}
        style={{
          flexGrow: 1,
          minHeight: 132,
          padding: 16,
          gap: 14,
          justifyContent: "space-between",
        }}
      >
        <IconTile name={VITAL_ICONS[m.id] ?? "insights"} tone={m.accent ? "tertiary" : "primary"} />
        <View>
          <View style={{ flexDirection: "row", alignItems: "baseline", gap: 4 }}>
            <FitText size={32} color={c.fg}>
              {m.value}
            </FitText>
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
      </Pressable>
    </View>
  );
}

/** The tones of heart rate zones 1 to 5. Z1 uses outline, so it shows on a tonal card. */
export function zoneTones(md: Md) {
  return [md.outline, md.secondary, md.tertiary, md.primary, md.error];
}

/** The icon of a sport family from the core. */
export const FAMILY_ICON: Record<string, IconName> = {
  endurance: "directions-run",
  gym: "fitness-center",
  golf: "golf-course",
  water: "pool",
  mindBody: "self-improvement",
};

const SPORT_ICONS: [RegExp, IconName][] = [
  // Before the cycling match, as in the core (sport.rs).
  [/motorcycl/, "two-wheeler"],
  [/run/, "directions-run"],
  [/cycl|bik|ride/, "directions-bike"],
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
