// Activity detail on Android: a large app bar with a back button, then tonal cards.
// The core decides which blocks exist. A block with no data is not drawn.
import { useLocalSearchParams } from "expo-router";
import { View } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import {
  Bars,
  Card,
  Content,
  Loading,
  type Md,
  SectionLabel,
  SplitBar,
  T,
} from "../components/MdKit";
import { type ActivityDetail, barHeights, useActivityDetail } from "../data/activities";
import { useMd } from "../theme/md";

function Stats({ d }: { d: ActivityDetail }) {
  const md = useMd();
  return (
    <Card tone="primary" gap={16} style={{ borderRadius: 32 }}>
      <T v="label" color={md.onPrimaryContainer}>{`${d.sportLabel} - ${d.when}`}</T>
      <View style={{ flexDirection: "row", flexWrap: "wrap", columnGap: 28, rowGap: 16 }}>
        {d.stats.map((s) => (
          <View key={s.label}>
            <T v="headlineSmall" color={md.onPrimaryContainer}>
              {s.value}
            </T>
            <T v="labelSmall" color={md.onPrimaryContainer}>
              {s.label}
            </T>
          </View>
        ))}
      </View>
    </Card>
  );
}

function Zones({ z }: { z: NonNullable<ActivityDetail["zones"]> }) {
  const md = useMd();
  const tones = [md.outlineVariant, md.secondary, md.tertiary, md.primary, md.error];
  return (
    <Card gap={12}>
      <T v="titleMedium">Heart rate zones</T>
      <SplitBar
        parts={z.rows
          .filter((r) => r.percent > 0)
          .map((r) => ({ weight: r.percent, color: tones[r.zone - 1] ?? md.primary }))}
      />
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {z.rows.map((r) => `Z${r.zone} ${r.time}`).join(", ")}
      </T>
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {z.sentence}
      </T>
    </Card>
  );
}

function Trace({
  title,
  values,
  label,
}: {
  title: string;
  values: (number | null)[];
  label: string;
}) {
  const heights = barHeights(values);
  if (heights.length === 0) return null;
  return (
    <>
      <T v="titleMedium">{title}</T>
      <Bars
        values={heights}
        max={1}
        height={80}
        gap={1}
        describe={(_, i) => (i === 0 ? label : "")}
      />
    </>
  );
}

function ChartBlock({ md, c }: { md: Md; c: NonNullable<ActivityDetail["chart"]> }) {
  return (
    <Card gap={12}>
      <Trace title="Heart rate" values={c.hr} label="Heart rate over the session" />
      {c.elevation.length > 0 ? (
        <Trace title="Elevation" values={c.elevation} label="Elevation over the session" />
      ) : null}
      <T v="labelSmall" color={md.onSurfaceVariant}>{`Over ${c.span}.`}</T>
    </Card>
  );
}

function Table({ title, note, rows }: { title: string; note?: string; rows: string[][] }) {
  const md = useMd();
  return (
    <>
      <SectionLabel>{title}</SectionLabel>
      {note ? (
        <T v="bodyMedium" color={md.onSurfaceVariant}>
          {note}
        </T>
      ) : null}
      <Card gap={10}>
        {rows.map((r) => (
          <T key={r[0]} v="bodyMedium">
            {r.join("  ")}
          </T>
        ))}
      </Card>
    </>
  );
}

function DetailBody({ md, d }: { md: Md; d: ActivityDetail }) {
  return (
    <Content>
      <Stats d={d} />
      {d.zones ? <Zones z={d.zones} /> : null}
      {d.chart ? <ChartBlock md={md} c={d.chart} /> : null}
      {d.laps.length > 0 ? (
        <Table
          title="Laps"
          rows={d.laps.map((l) => [l.label, l.distance ?? "-", l.time ?? "-", l.pace ?? "-"])}
        />
      ) : null}
      {d.strength ? (
        <Table
          title="Sets"
          note={d.strength.sentence}
          rows={d.strength.sets.map((s) => [
            s.label,
            s.reps ?? "-",
            s.rest ? `rest ${s.rest}` : "",
            s.exercise ?? "",
          ])}
        />
      ) : null}
    </Content>
  );
}

export function ActivityDetailScreen() {
  const md = useMd();
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data, error, isPending } = useActivityDetail(Number(id));
  return (
    <LargeAppBarScreen title={data?.name ?? "Activity"} back>
      {isPending ? <Loading /> : null}
      {error ? (
        <Content>
          <Card tone="error">
            <T color={md.onErrorContainer}>{`The session could not be read. ${String(error)}`}</T>
          </Card>
        </Content>
      ) : null}
      {data === null ? (
        <Content>
          <Card>
            <T color={md.onSurfaceVariant}>This session is not in the cache.</T>
          </Card>
        </Content>
      ) : null}
      {data ? <DetailBody md={md} d={data} /> : null}
    </LargeAppBarScreen>
  );
}
