// Health on Android: a Sleep card that opens the Sleep detail, the body rows, then one card for each metric.
// The core sends the metrics that the watch records, with a trend sentence for each.
import { useRouter } from "expo-router";
import { View } from "react-native";
import { BodyLinks } from "../components/BodyLinks";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import { Bars, Card, Content, ErrorCard, IconTile, Loading, T } from "../components/MdKit";
import { type MetricTrend, scaled, useHealth, useSleep } from "../data/wellness";
import { useMd } from "../theme/md";

function SleepCard() {
  const router = useRouter();
  const md = useMd();
  const { data } = useSleep();
  return (
    <Card
      tone="primary"
      gap={12}
      style={{ borderRadius: 32 }}
      onPress={() => router.push("/health/sleep")}
    >
      <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
        <IconTile name="bedtime" tone="high" />
        <T v="headlineSmall" color={md.onPrimaryContainer}>
          Sleep
        </T>
      </View>
      <T v="body" color={md.onPrimaryContainer}>
        {data?.lede ?? "Last night, the stages and the last 30 nights."}
      </T>
    </Card>
  );
}

function MetricCard({ m }: { m: MetricTrend }) {
  const md = useMd();
  const router = useRouter();
  const value = m.unit ? `${m.valueText} ${m.unit}` : m.valueText;
  return (
    <Card gap={12} onPress={() => router.push(`/health/metric/${m.key}`)}>
      <View
        style={{ flexDirection: "row", alignItems: "baseline", justifyContent: "space-between" }}
      >
        <T v="titleMedium">{m.label}</T>
        <T v="headlineSmall">{value}</T>
      </View>
      <Bars
        values={scaled(m.points)}
        max={1}
        height={48}
        gap={3}
        describe={(v, i) =>
          v === null ? `Day ${i + 1}: no reading` : `Day ${i + 1} of ${m.points.length}`
        }
      />
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {m.sentence}
      </T>
      <T v="labelSmall" color={md.onSurfaceVariant}>
        {m.caption}
      </T>
    </Card>
  );
}

function HealthBody() {
  const md = useMd();
  const { data, error, refetch } = useHealth();
  if (error) return <ErrorCard message={String(error)} onRetry={refetch} />;
  if (!data) return <Loading />;
  return (
    <Content>
      <View style={{ paddingHorizontal: 4 }}>
        <T v="bodyMedium" color={md.onSurfaceVariant}>
          {data.note}
        </T>
      </View>
      <SleepCard />
      <BodyLinks />
      {data.metrics.map((m) => (
        <MetricCard key={m.key} m={m} />
      ))}
    </Content>
  );
}

export function HealthScreen() {
  return (
    <LargeAppBarScreen title="Health">
      <HealthBody />
    </LargeAppBarScreen>
  );
}
