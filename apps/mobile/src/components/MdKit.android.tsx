// The Material 3 Expressive pieces of the Android screens: tonal cards, grouped
// list rows, a type scale and small charts. They draw from the colour scheme
// (the wallpaper on Android 12 and later), so no screen has a fixed colour.
import MaterialIcons from "@expo/vector-icons/MaterialIcons";
import { Children, type ComponentProps, type ReactNode } from "react";
import {
  ActivityIndicator,
  type ColorValue,
  Pressable,
  Text,
  type TextStyle,
  View,
  type ViewStyle,
} from "react-native";
import { useMd } from "../theme/md";

export type Md = ReturnType<typeof useMd>;
export type IconName = ComponentProps<typeof MaterialIcons>["name"];

export const GAP = 12;
export const MARGIN = 16;
const OUTER = 28;
const INNER = 6;

export type Tone = "surface" | "high" | "primary" | "secondary" | "tertiary" | "error";

/** The container colour, the ink on it and the quiet ink on it. */
export function toneColors(md: Md, tone: Tone) {
  switch (tone) {
    case "primary":
      return { bg: md.primaryContainer, fg: md.onPrimaryContainer, quiet: md.onPrimaryContainer };
    case "secondary":
      return {
        bg: md.secondaryContainer,
        fg: md.onSecondaryContainer,
        quiet: md.onSecondaryContainer,
      };
    case "tertiary":
      return {
        bg: md.tertiaryContainer,
        fg: md.onTertiaryContainer,
        quiet: md.onTertiaryContainer,
      };
    case "error":
      return { bg: md.errorContainer, fg: md.onErrorContainer, quiet: md.onErrorContainer };
    case "high":
      return { bg: md.surfaceContainerHigh, fg: md.onSurface, quiet: md.onSurfaceVariant };
    default:
      return { bg: md.surfaceContainer, fg: md.onSurface, quiet: md.onSurfaceVariant };
  }
}

// The Material 3 type scale. The emphasized variants are heavier, not larger.
const TYPE = {
  display: { fontSize: 44, lineHeight: 52, fontWeight: "600" },
  headline: { fontSize: 32, lineHeight: 40, fontWeight: "600" },
  headlineSmall: { fontSize: 24, lineHeight: 32, fontWeight: "600" },
  title: { fontSize: 22, lineHeight: 28, fontWeight: "500" },
  titleMedium: { fontSize: 16, lineHeight: 24, fontWeight: "600" },
  body: { fontSize: 16, lineHeight: 24 },
  bodyMedium: { fontSize: 14, lineHeight: 20 },
  label: { fontSize: 14, lineHeight: 20, fontWeight: "600" },
  labelSmall: { fontSize: 12, lineHeight: 16, fontWeight: "500" },
} as const satisfies Record<string, TextStyle>;

export type Variant = keyof typeof TYPE;

export function T({
  v = "body",
  color,
  align,
  lines,
  children,
}: {
  v?: Variant;
  color?: ColorValue;
  align?: TextStyle["textAlign"];
  lines?: number;
  children: ReactNode;
}) {
  const md = useMd();
  return (
    <Text
      numberOfLines={lines}
      style={[
        TYPE[v],
        { color: color ?? md.onSurface, textAlign: align, fontVariant: ["tabular-nums"] },
      ]}
    >
      {children}
    </Text>
  );
}

/** The padded column that every screen puts its cards in. */
export function Content({ children }: { children: ReactNode }) {
  return <View style={{ padding: MARGIN, gap: GAP }}>{children}</View>;
}

/** A section label above a card or a group. */
export function SectionLabel({ children }: { children: string }) {
  const md = useMd();
  return (
    <View style={{ paddingHorizontal: MARGIN, paddingTop: 12 }}>
      <T v="label" color={md.primary}>
        {children}
      </T>
    </View>
  );
}

/** A tonal card with a large corner. It can open a screen. */
export function Card({
  tone = "surface",
  onPress,
  gap = 8,
  style,
  children,
}: {
  tone?: Tone;
  onPress?: () => void;
  gap?: number;
  style?: ViewStyle;
  children: ReactNode;
}) {
  const md = useMd();
  const box: ViewStyle = {
    backgroundColor: toneColors(md, tone).bg,
    borderRadius: OUTER,
    overflow: "hidden",
    ...style,
  };
  const inner = <View style={{ padding: 20, gap }}>{children}</View>;
  if (!onPress) return <View style={box}>{inner}</View>;
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      android_ripple={{ color: md.onSurfaceVariant + "22" }}
      style={box}
    >
      {inner}
    </Pressable>
  );
}

/** A round container for an icon, in a tone of the scheme. */
export function IconTile({
  name,
  tone = "secondary",
  size = 40,
}: {
  name: IconName;
  tone?: Tone;
  size?: number;
}) {
  const md = useMd();
  const c = toneColors(md, tone);
  return (
    <View
      style={{
        width: size,
        height: size,
        borderRadius: size / 2,
        backgroundColor: c.bg,
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <MaterialIcons
        name={name}
        size={size * 0.55}
        color={c.fg}
        accessible={false}
        importantForAccessibility="no-hide-descendants"
      />
    </View>
  );
}

/**
 * A group of rows in one container. The outer corners are large and the
 * corners between two rows are small, as in the Android 16 settings.
 */
export function Group({ children }: { children: ReactNode }) {
  const md = useMd();
  const rows = Children.toArray(children).filter(Boolean);
  return (
    <View style={{ gap: 2 }}>
      {rows.map((row, i) => {
        const first = i === 0;
        const last = i === rows.length - 1;
        const top = first ? OUTER : INNER;
        const bottom = last ? OUTER : INNER;
        return (
          <View
            // biome-ignore lint/suspicious/noArrayIndexKey: the rows never reorder
            key={i}
            style={{
              backgroundColor: md.surfaceContainer,
              borderTopLeftRadius: top,
              borderTopRightRadius: top,
              borderBottomLeftRadius: bottom,
              borderBottomRightRadius: bottom,
              overflow: "hidden",
            }}
          >
            {row}
          </View>
        );
      })}
    </View>
  );
}

/** One row of a group: a tonal icon, a title, a supporting line and a trailing part. */
export function ListRow({
  icon,
  tone = "secondary",
  title,
  supporting,
  trailing,
  chevron,
  danger,
  action,
  disabled,
  onPress,
}: {
  icon?: IconName;
  tone?: Tone;
  title: string;
  supporting?: string;
  trailing?: ReactNode;
  chevron?: boolean;
  danger?: boolean;
  action?: boolean;
  disabled?: boolean;
  onPress?: () => void;
}) {
  const md = useMd();
  const ink = disabled
    ? md.onSurfaceVariant
    : danger
      ? md.error
      : action
        ? md.primary
        : md.onSurface;
  return (
    <Pressable
      accessibilityRole={onPress ? "button" : undefined}
      disabled={!onPress || disabled}
      onPress={onPress}
      android_ripple={{ color: md.onSurfaceVariant + "22" }}
      style={{
        minHeight: icon ? 72 : 56,
        paddingHorizontal: 20,
        paddingVertical: 12,
        flexDirection: "row",
        alignItems: "center",
        gap: 16,
      }}
    >
      {icon ? <IconTile name={icon} tone={tone} /> : null}
      <View style={{ flex: 1 }}>
        <T v="titleMedium" color={ink}>
          {title}
        </T>
        {supporting ? (
          <T v="bodyMedium" color={md.onSurfaceVariant} lines={2}>
            {supporting}
          </T>
        ) : null}
      </View>
      {trailing}
      {chevron ? (
        <MaterialIcons name="chevron-right" size={24} color={md.onSurfaceVariant} />
      ) : null}
    </Pressable>
  );
}

/** A figure with its label, in a tonal tile. Tiles of a row share the width. */
export function Tile({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  const md = useMd();
  const c = toneColors(md, accent ? "tertiary" : "secondary");
  return (
    <View
      style={{
        flex: 1,
        backgroundColor: c.bg,
        borderRadius: 24,
        padding: 16,
        gap: 4,
        justifyContent: "space-between",
        minHeight: 96,
      }}
    >
      <T v="label" color={c.quiet}>
        {label}
      </T>
      <T v="headlineSmall" color={c.fg}>
        {value}
      </T>
    </View>
  );
}

/** Tiles in rows of two, in one tone. A tile with a signal takes the tertiary tone. A lone tile is full width. */
export function TileGrid({
  tiles,
}: {
  tiles: { label: string; value: string; accent?: boolean }[];
}) {
  const rows: (typeof tiles)[] = [];
  for (let i = 0; i < tiles.length; i += 2) rows.push(tiles.slice(i, i + 2));
  return (
    <View style={{ gap: GAP }}>
      {rows.map((row) => (
        <View key={row[0]?.label} style={{ flexDirection: "row", gap: GAP }}>
          {row.map((t) => (
            <Tile key={t.label} {...t} />
          ))}
        </View>
      ))}
    </View>
  );
}

/** Rounded bars. A null value is a hairline. The bars take the scheme's primary. */
export function Bars({
  values,
  height = 96,
  max,
  gap = 6,
  color,
  track,
  colorFor,
  describe,
}: {
  values: (number | null)[];
  height?: number;
  max?: number;
  gap?: number;
  color?: ColorValue;
  track?: ColorValue;
  colorFor?: (value: number, index: number) => ColorValue;
  describe?: (value: number | null, index: number) => string;
}) {
  const md = useMd();
  const top = max ?? Math.max(...values.map((v) => v ?? 0), 1);
  const radius = Math.min(8, gap + 2);
  return (
    <View style={{ flexDirection: "row", alignItems: "flex-end", height, gap }}>
      {values.map((v, i) => (
        <View
          // biome-ignore lint/suspicious/noArrayIndexKey: the bars never reorder
          key={i}
          accessible={describe !== undefined}
          accessibilityLabel={describe?.(v, i)}
          style={{
            flex: 1,
            height: v === null ? 2 : Math.max((v / top) * height, 4),
            borderRadius: radius,
            backgroundColor:
              v === null || v <= 0
                ? (track ?? md.outlineVariant)
                : (colorFor?.(v, i) ?? color ?? md.primary),
          }}
        />
      ))}
    </View>
  );
}

/** One label under each bar, or one at each end of a long window. */
export function BarLabels({ labels }: { labels: string[] }) {
  const md = useMd();
  const ends = labels.length === 2;
  return (
    <View style={{ flexDirection: "row", justifyContent: ends ? "space-between" : undefined }}>
      {labels.map((l, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: the labels never reorder
        <View key={i} style={{ flex: ends ? undefined : 1, alignItems: "center" }}>
          <T v="labelSmall" color={md.onSurfaceVariant}>
            {l}
          </T>
        </View>
      ))}
    </View>
  );
}

/** A bar split in parts that share its width, with rounded ends. */
export function SplitBar({ parts }: { parts: { weight: number; color: ColorValue }[] }) {
  return (
    <View style={{ flexDirection: "row", height: 12, gap: 2 }}>
      {parts.map((p, i) => (
        <View
          // biome-ignore lint/suspicious/noArrayIndexKey: the parts never reorder
          key={i}
          style={{
            flex: Math.max(p.weight, 1),
            borderRadius: 6,
            backgroundColor: p.color,
          }}
        />
      ))}
    </View>
  );
}

export function Loading() {
  return <ActivityIndicator style={{ margin: 32 }} />;
}

/** The error message of a screen and a retry button. */
export function ErrorCard({ message, onRetry }: { message: string; onRetry: () => void }) {
  const md = useMd();
  return (
    <Content>
      <Card tone="error">
        <T v="body" color={md.onErrorContainer}>
          {message}
        </T>
        <Pressable
          accessibilityRole="button"
          onPress={onRetry}
          style={{
            alignSelf: "flex-start",
            minHeight: 48,
            borderRadius: 24,
            paddingHorizontal: 24,
            justifyContent: "center",
            backgroundColor: md.error,
          }}
        >
          <T v="label" color={md.onError}>
            Try again
          </T>
        </Pressable>
      </Card>
    </Content>
  );
}
