// Weight, Food and Strength on iOS: grouped lists, system colours, Dynamic Type.
// The core sends the text. This file draws the blocks of `data/body.ts`.
import { Stack } from "expo-router";
import type { ReactNode } from "react";
import { ActivityIndicator, Button, PlatformColor, ScrollView, Text, View } from "react-native";
import { TrendBars } from "../components/TrendBars";
import { type Block, type BodyKind, useBody } from "../data/body";

const label = PlatformColor("label");
const secondary = PlatformColor("secondaryLabel");
const cell = PlatformColor("secondarySystemGroupedBackground");
const separator = PlatformColor("separator");
const tint = PlatformColor("systemOrange");

function Group({ title, children }: { title?: string; children?: ReactNode }) {
  return (
    <View style={{ marginTop: 24 }}>
      {title ? (
        <Text
          accessibilityRole="header"
          style={{
            fontSize: 13,
            color: secondary,
            textTransform: "uppercase",
            marginLeft: 32,
            marginBottom: 6,
          }}
        >
          {title}
        </Text>
      ) : null}
      {children ? (
        <View
          style={{
            backgroundColor: cell,
            borderRadius: 26,
            marginHorizontal: 16,
            overflow: "hidden",
          }}
        >
          {children}
        </View>
      ) : null}
    </View>
  );
}

function Row({
  left,
  right,
  sub,
  accent,
  last,
}: {
  left: string;
  right?: string;
  sub?: string;
  accent?: boolean;
  last?: boolean;
}) {
  return (
    <View
      style={{
        flexDirection: "row",
        justifyContent: "space-between",
        alignItems: "center",
        gap: 12,
        minHeight: 44,
        paddingHorizontal: 16,
        paddingVertical: 10,
        borderBottomWidth: last ? 0 : 0.5,
        borderBottomColor: separator,
      }}
    >
      <View style={{ flexShrink: 1 }}>
        <Text style={{ fontSize: 17, color: label }}>{left}</Text>
        {sub ? <Text style={{ fontSize: 13, color: secondary }}>{sub}</Text> : null}
      </View>
      {right ? (
        <Text style={{ fontSize: 17, color: accent ? tint : secondary }}>{right}</Text>
      ) : null}
    </View>
  );
}

function Note({ children, strong }: { children: string; strong?: boolean }) {
  return (
    <Text
      style={{
        fontSize: strong ? 17 : 13,
        color: strong ? label : secondary,
        marginHorizontal: 32,
        marginTop: 12,
      }}
    >
      {children}
    </Text>
  );
}

function BlockView({ b }: { b: Block }) {
  switch (b.kind) {
    case "text":
      if (b.tone === "heading") return <Group title={b.text} />;
      return <Note strong={b.tone !== "note"}>{b.text}</Note>;
    case "tiles":
      return (
        <Group>
          {b.tiles.map((t, i) => (
            <Row
              key={t.label}
              left={t.label}
              right={t.value}
              accent={t.accent}
              last={i === b.tiles.length - 1}
            />
          ))}
        </Group>
      );
    case "chart":
      return (
        <Group title={b.title}>
          <View style={{ padding: 16 }}>
            <TrendBars
              values={b.values}
              color="#ff9500"
              label={`${b.title} from ${b.from} to ${b.to}`}
            />
            <Text style={{ fontSize: 13, color: secondary, marginTop: 8 }}>
              {`${b.from} to ${b.to}`}
            </Text>
          </View>
        </Group>
      );
    case "rows":
      return (
        <Group title={b.title}>
          {b.rows.map((r, i) => (
            <Row
              // biome-ignore lint/suspicious/noArrayIndexKey: a date can repeat in a list
              key={`${r.key}-${i}`}
              left={r.key}
              right={r.value}
              sub={r.sub}
              last={i === b.rows.length - 1}
            />
          ))}
        </Group>
      );
    case "empty":
      return (
        <Group>
          <View style={{ padding: 16, gap: 8 }}>
            <Text style={{ fontSize: 17, fontWeight: "600", color: label }}>{b.title}</Text>
            <Text style={{ fontSize: 15, color: secondary }}>{b.body}</Text>
          </View>
        </Group>
      );
  }
}

export function BodyScreen({ kind, title }: { kind: BodyKind; title: string }) {
  const { data, error, refetch } = useBody(kind);
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      <Stack.Screen options={{ title }} />
      {data?.blocks.map((b, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: the blocks come in a fixed order
        <BlockView key={i} b={b} />
      ))}
      {!data && !error ? <ActivityIndicator style={{ margin: 32 }} /> : null}
      {error ? (
        <Group>
          <View style={{ padding: 16 }}>
            <Text style={{ fontSize: 17, color: label }}>{String(error)}</Text>
            <Button title="Try again" onPress={() => refetch()} />
          </View>
        </Group>
      ) : null}
    </ScrollView>
  );
}
