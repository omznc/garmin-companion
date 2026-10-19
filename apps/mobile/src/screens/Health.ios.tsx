// Health on iOS: native large title, grouped sections, system colours, Dynamic Type.
// The core sends the metrics that the watch records, with a trend sentence for each.
import { useRouter } from "expo-router";
import { PlatformColor, Pressable, ScrollView, Text, View } from "react-native";
import { BodyLinks } from "../components/BodyLinks";
import { ErrorGroup, Group, Loading, Note, Row, secondary } from "../components/Grouped";
import { MiniBars } from "../components/MiniBars";
import { type MetricTrend, scaled, useHealth, useSleep } from "../data/wellness";

function SleepGroup() {
  const router = useRouter();
  const { data } = useSleep();
  return (
    <Group title="Sleep">
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="Open Sleep"
        onPress={() => router.push("/health/sleep")}
      >
        <Note>{data?.lede ?? "Last night, the stages and the last 30 nights."}</Note>
      </Pressable>
    </Group>
  );
}

function MetricGroup({ m }: { m: MetricTrend }) {
  const router = useRouter();
  return (
    <Group title={m.label}>
      <Row
        left="Latest"
        right={m.unit ? `${m.valueText} ${m.unit}` : m.valueText}
        onPress={() => router.push(`/health/metric/${m.key}`)}
      />
      <View style={{ paddingVertical: 12 }}>
        <MiniBars
          values={scaled(m.points)}
          max={1}
          height={40}
          color={PlatformColor("systemOrange")}
          trackColor={PlatformColor("separator")}
          describe={(v, i) =>
            v === null ? `Day ${i + 1}: no reading` : `Day ${i + 1} of ${m.points.length}`
          }
        />
      </View>
      <Note>{m.sentence}</Note>
      <View style={{ paddingHorizontal: 16, paddingBottom: 12 }}>
        <Text style={{ fontSize: 13, color: secondary }}>{m.caption}</Text>
      </View>
    </Group>
  );
}

export function HealthScreen() {
  const { data, error, refetch } = useHealth();
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      {data ? (
        <>
          <Group>
            <Note>{data.note}</Note>
          </Group>
          <SleepGroup />
          <BodyLinks />
          {data.metrics.map((m) => (
            <MetricGroup key={m.key} m={m} />
          ))}
        </>
      ) : null}
      {!data && !error ? <Loading /> : null}
      {error ? <ErrorGroup message={String(error)} onRetry={() => refetch()} /> : null}
    </ScrollView>
  );
}
