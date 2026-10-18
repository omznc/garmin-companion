// Today on iOS: native large title, grouped sections, system colours, Dynamic Type.
// UNVERIFIED until it runs on the iOS Simulator.
import { ActivityIndicator, PlatformColor, ScrollView, Text, View } from "react-native";
import type { ReactNode } from "react";
import type { Today } from "../data/types";
import { useToday } from "../data/useToday";

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
  accent,
  last,
}: {
  left: string;
  right?: string;
  accent?: boolean;
  last?: boolean;
}) {
  return (
    <View
      style={{
        flexDirection: "row",
        justifyContent: "space-between",
        alignItems: "center",
        minHeight: 44,
        paddingHorizontal: 16,
        paddingVertical: 10,
        borderBottomWidth: last ? 0 : 0.5,
        borderBottomColor: separator,
      }}
    >
      <Text style={{ fontSize: 17, color: label, flexShrink: 1 }}>{left}</Text>
      {right ? (
        <Text style={{ fontSize: 17, color: accent ? tint : secondary }}>{right}</Text>
      ) : null}
    </View>
  );
}

function TodayBody({ today }: { today: Today }) {
  const s = today.lastSession;
  const week = today.week;
  return (
    <>
      <Group>
        <View style={{ padding: 16 }}>
          <Text style={{ fontSize: 15, color: secondary }}>{today.dateLabel}</Text>
          <Text style={{ fontSize: 17, color: label, marginTop: 6 }}>{today.lede}</Text>
        </View>
      </Group>
      {today.metrics.length > 0 && (
        <Group title="Health">
          {today.metrics.map((m, i) => (
            <Row
              key={m.id}
              left={m.label}
              right={m.unit ? `${m.value} ${m.unit}` : m.value}
              accent={m.accent}
              last={i === today.metrics.length - 1}
            />
          ))}
        </Group>
      )}
      {today.fuelLine !== "" && (
        <Group>
          <View style={{ padding: 16 }}>
            <Text style={{ fontSize: 15, color: secondary }}>{today.fuelLine}</Text>
          </View>
        </Group>
      )}
      {today.attention.length > 0 && (
        <Group title="Attention">
          {today.attention.map((n, i) => (
            <Row
              key={n.text}
              left={n.text}
              accent={n.accent}
              last={i === today.attention.length - 1}
            />
          ))}
        </Group>
      )}
      {s && (
        <Group title={`Last session, ${s.name}`}>
          {s.stats.map((stat, i) => (
            <Row
              key={stat.label}
              left={stat.label}
              right={stat.value}
              last={i === s.stats.length - 1}
            />
          ))}
        </Group>
      )}
      {week && (
        <>
          <Group title="Last seven days">
            {week.labels.map((d, i) => (
              <Row key={d} left={d} right={`${week.km[i]?.toFixed(1)} km`} last={i === 6} />
            ))}
          </Group>
          <Text style={{ fontSize: 13, color: secondary, margin: 32 }}>{week.summary}</Text>
        </>
      )}
    </>
  );
}

export function TodayScreen() {
  const { data } = useToday();
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
    >
      {data ? <TodayBody today={data} /> : <ActivityIndicator style={{ margin: 32 }} />}
    </ScrollView>
  );
}
