// Weight, Food and Strength on iOS: grouped lists, system colours, Dynamic Type.
// The core sends the text. This file draws the blocks of `data/body.ts`.
import { Stack } from "expo-router";
import { type ReactNode, useState } from "react";
import {
  ActivityIndicator,
  Button,
  PlatformColor,
  Pressable,
  ScrollView,
  Text,
  View,
} from "react-native";
import { RouteTrace, WearBar } from "../components/RouteTrace";
import { TrendBars } from "../components/TrendBars";
import { type Block, type BlockRow, type ScreenKind, useBody } from "../data/body";

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
  trace,
  bar,
}: {
  left: string;
  right?: string;
  sub?: string;
  accent?: boolean;
  last?: boolean;
  trace?: BlockRow["trace"];
  bar?: BlockRow["bar"];
}) {
  return (
    <View
      style={{
        paddingHorizontal: 16,
        paddingVertical: 10,
        borderBottomWidth: last ? 0 : 0.5,
        borderBottomColor: separator,
      }}
    >
      <View
        style={{
          flexDirection: "row",
          justifyContent: "space-between",
          alignItems: "center",
          gap: 12,
          minHeight: 44,
        }}
      >
        <View style={{ flexShrink: 1 }}>
          <Text style={{ fontSize: 17, color: label }}>{left}</Text>
          {sub ? <Text style={{ fontSize: 13, color: secondary }}>{sub}</Text> : null}
        </View>
        <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
          {trace?.length ? <RouteTrace points={trace} color={tint} /> : null}
          {right ? (
            <Text style={{ fontSize: 17, color: accent ? tint : secondary }}>{right}</Text>
          ) : null}
        </View>
      </View>
      {bar ? (
        <WearBar fraction={bar.fraction} color={bar.accent ? tint : secondary} track={separator} />
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

function BlockView({ b, onAction }: { b: Block; onAction: (id: string) => void }) {
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
              trace={r.trace}
              bar={r.bar}
              last={i === b.rows.length - 1}
            />
          ))}
        </Group>
      );
    case "actions":
      return (
        <Group>
          {b.actions.map((a, i) => (
            <Pressable
              key={a.id}
              accessibilityRole="button"
              onPress={() => onAction(a.id)}
              style={{
                minHeight: 44,
                justifyContent: "center",
                paddingHorizontal: 16,
                borderBottomWidth: i === b.actions.length - 1 ? 0 : 0.5,
                borderBottomColor: separator,
              }}
            >
              <Text style={{ fontSize: 17, color: tint }}>{a.label}</Text>
            </Pressable>
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

export function BodyScreen({ kind, title }: { kind: ScreenKind; title: string }) {
  // The weekly report pages through its weeks. Offset 0 is the newest week.
  const [offset, setOffset] = useState(0);
  const { data, error, refetch } = useBody(kind, offset);
  const onAction = (id: string) =>
    setOffset((o) => (id === "earlier" ? o + 1 : Math.max(o - 1, 0)));
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      <Stack.Screen options={{ title }} />
      {data?.blocks.map((b, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: the blocks come in a fixed order
        <BlockView key={i} b={b} onAction={onAction} />
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
