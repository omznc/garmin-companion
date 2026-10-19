// One health metric on iOS: grouped sections, system colours. The stack gives the title.
// It reads the `key` route param. The core sends every figure and sentence.
import { Stack, useLocalSearchParams } from "expo-router";
import { PlatformColor, ScrollView, Text, View } from "react-native";
import { ErrorGroup, Group, Loading, Note, Row, secondary } from "../components/Grouped";
import { MiniBars } from "../components/MiniBars";
import { type HealthView, type MetricTrend, scaled, useHealth, windowEnds } from "../data/wellness";

function MetricBody({ view, m }: { view: HealthView; m: MetricTrend }) {
  const [first, last] = windowEnds(view, m);
  return (
    <>
      <Group>
        <Row left="Latest" right={m.unit ? `${m.valueText} ${m.unit}` : m.valueText} last />
      </Group>
      <Group title={`${first} to ${last}`}>
        <View style={{ paddingVertical: 16 }}>
          <MiniBars
            values={scaled(m.points)}
            max={1}
            height={120}
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
    </>
  );
}

function MetricContent() {
  const { key } = useLocalSearchParams<{ key: string }>();
  const { data, error, refetch } = useHealth();
  const m = data?.metrics.find((x) => x.key === key);
  if (error) return <ErrorGroup message={String(error)} onRetry={() => refetch()} />;
  if (!data) return <Loading />;
  if (!m) {
    return (
      <Group>
        <Note>No data for this metric.</Note>
      </Group>
    );
  }
  return <MetricBody view={data} m={m} />;
}

export function MetricScreen() {
  const { key } = useLocalSearchParams<{ key: string }>();
  const { data } = useHealth();
  const title = data?.metrics.find((x) => x.key === key)?.label ?? "Metric";
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      <Stack.Screen options={{ title }} />
      <MetricContent />
    </ScrollView>
  );
}
