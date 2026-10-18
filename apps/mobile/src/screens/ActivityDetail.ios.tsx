// Activity detail on iOS: a pushed screen with an inline title, grouped sections, system colours.
// The core decides which blocks exist. A block with no data is not drawn.
import { Stack, useLocalSearchParams } from "expo-router";
import type { ReactNode } from "react";
import { ActivityIndicator, PlatformColor, ScrollView, Text, View } from "react-native";
import { type ActivityDetail, barHeights, useActivityDetail } from "../data/activities";

const label = PlatformColor("label");
const secondary = PlatformColor("secondaryLabel");
const cell = PlatformColor("secondarySystemGroupedBackground");
const separator = PlatformColor("separator");
const tint = PlatformColor("systemOrange");

function Group({ title, children }: { title?: string; children: ReactNode }) {
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
    </View>
  );
}

function Row({
  left,
  right,
  sub,
  last,
}: {
  left: string;
  right?: string;
  sub?: string;
  last: boolean;
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
      {right ? <Text style={{ fontSize: 17, color: secondary }}>{right}</Text> : null}
    </View>
  );
}

function Note({ children }: { children: string }) {
  return (
    <View style={{ marginHorizontal: 32, marginTop: 8 }}>
      <Text style={{ fontSize: 15, color: secondary }}>{children}</Text>
    </View>
  );
}

function Zones({ z }: { z: NonNullable<ActivityDetail["zones"]> }) {
  const fills = [separator, secondary, tint, tint, tint];
  const opacity = [1, 1, 0.55, 0.78, 1];
  return (
    <Group title="Heart rate zones">
      <View
        style={{
          flexDirection: "row",
          height: 10,
          margin: 16,
          borderRadius: 2,
          overflow: "hidden",
        }}
      >
        {z.rows
          .filter((r) => r.percent > 0)
          .map((r) => (
            <View
              key={r.zone}
              style={{
                flex: Math.max(r.percent, 1),
                backgroundColor: fills[r.zone - 1],
                opacity: opacity[r.zone - 1],
              }}
            />
          ))}
      </View>
      {z.rows.map((r, i) => (
        <Row key={r.zone} left={`Zone ${r.zone}`} right={r.time} last={i === z.rows.length - 1} />
      ))}
    </Group>
  );
}

function Bars({ values, name }: { values: (number | null)[]; name: string }) {
  const heights = barHeights(values);
  if (heights.length === 0) return null;
  return (
    <View
      accessible
      accessibilityLabel={name}
      style={{ flexDirection: "row", alignItems: "flex-end", height: 80, gap: 1, padding: 16 }}
    >
      {heights.map((h, i) => (
        <View
          // biome-ignore lint/suspicious/noArrayIndexKey: the samples never reorder
          key={i}
          style={{
            flex: 1,
            height: Math.max(h * 48, 1),
            backgroundColor: tint,
            opacity: h > 0 ? 1 : 0,
          }}
        />
      ))}
    </View>
  );
}

function Detail({ d }: { d: ActivityDetail }) {
  return (
    <>
      <Note>{`${d.sportLabel}, ${d.when}`}</Note>
      <Group>
        {d.stats.map((s, i) => (
          <Row key={s.label} left={s.label} right={s.value} last={i === d.stats.length - 1} />
        ))}
      </Group>
      {d.zones ? (
        <>
          <Zones z={d.zones} />
          <Note>{d.zones.sentence}</Note>
        </>
      ) : null}
      {d.chart ? (
        <>
          <Group title="Heart rate">
            <Bars values={d.chart.hr} name="Heart rate over the session" />
          </Group>
          {d.chart.elevation.length > 0 ? (
            <Group title="Elevation">
              <Bars values={d.chart.elevation} name="Elevation over the session" />
            </Group>
          ) : null}
          <Note>{`Over ${d.chart.span}.`}</Note>
        </>
      ) : null}
      {d.laps.length > 0 ? (
        <Group title="Laps">
          {d.laps.map((l, i) => (
            <Row
              key={l.label}
              left={l.label}
              sub={l.pace ?? undefined}
              right={[l.distance, l.time].filter(Boolean).join(", ")}
              last={i === d.laps.length - 1}
            />
          ))}
        </Group>
      ) : null}
      {d.strength ? (
        <>
          <Group title="Sets">
            {d.strength.sets.map((s, i) => (
              <Row
                key={s.label}
                left={s.label}
                sub={[s.exercise, s.rest ? `rest ${s.rest}` : null].filter(Boolean).join(", ")}
                right={s.reps ? `${s.reps} reps` : undefined}
                last={i === (d.strength?.sets.length ?? 0) - 1}
              />
            ))}
          </Group>
          <Note>{d.strength.sentence}</Note>
        </>
      ) : null}
    </>
  );
}

export function ActivityDetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data, error, isPending } = useActivityDetail(Number(id));
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      <Stack.Screen
        options={{
          title: data?.name ?? "Activity",
          headerLargeTitle: false,
          headerRight: () => null,
        }}
      />
      {isPending ? <ActivityIndicator style={{ margin: 32 }} /> : null}
      {error ? <Note>{`The session could not be read. ${String(error)}`}</Note> : null}
      {data === null ? <Note>This session is not in the cache.</Note> : null}
      {data ? <Detail d={data} /> : null}
    </ScrollView>
  );
}
