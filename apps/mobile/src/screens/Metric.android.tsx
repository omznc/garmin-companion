// One health metric on Android: the latest figure, a large chart of the window and the trend sentence.
// It reads the `key` route param. The core sends the figures and sentences. The screen computes the window dates.
import { useLocalSearchParams } from "expo-router";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import { BarLabels, Bars, Card, Content, ErrorCard, Loading, T } from "../components/MdKit";
import { type HealthView, type MetricTrend, scaled, useHealth, windowEnds } from "../data/wellness";
import { useMd } from "../theme/md";

function MetricBody({ view, m }: { view: HealthView; m: MetricTrend }) {
  const md = useMd();
  return (
    <Content>
      <Card gap={16}>
        <T v="display">{m.unit ? `${m.valueText} ${m.unit}` : m.valueText}</T>
        <Bars
          values={scaled(m.points)}
          max={1}
          height={160}
          gap={m.points.length > 14 ? 3 : 6}
          describe={(v, i) =>
            v === null ? `Day ${i + 1}: no reading` : `Day ${i + 1} of ${m.points.length}`
          }
        />
        <BarLabels labels={windowEnds(view, m)} />
      </Card>
      <T v="body">{m.sentence}</T>
      <T v="labelSmall" color={md.onSurfaceVariant}>
        {m.caption}
      </T>
    </Content>
  );
}

function MetricContent() {
  const { key } = useLocalSearchParams<{ key: string }>();
  const { data, error, refetch } = useHealth();
  const m = data?.metrics.find((x) => x.key === key);
  if (error) return <ErrorCard message={String(error)} onRetry={refetch} />;
  if (!data) return <Loading />;
  if (!m) {
    return (
      <Content>
        <T v="body">{`No reading in the last ${data.days} days.`}</T>
      </Content>
    );
  }
  return <MetricBody view={data} m={m} />;
}

export function MetricScreen() {
  const { key } = useLocalSearchParams<{ key: string }>();
  const { data } = useHealth();
  const title = data?.metrics.find((x) => x.key === key)?.label ?? "Metric";
  return (
    <LargeAppBarScreen title={title} back>
      <MetricContent />
    </LargeAppBarScreen>
  );
}
