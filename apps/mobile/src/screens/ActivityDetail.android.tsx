// Activity detail on Android: a large app bar with a back button, then tonal cards.
// The core decides which blocks exist. A block with no data is not drawn.
import { useLocalSearchParams } from "expo-router";
import { type ColorValue, View } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import {
  BarLabels,
  Bars,
  Card,
  Content,
  DataTable,
  IconTile,
  Loading,
  SectionLabel,
  SplitBar,
  T,
  TileGrid,
} from "../components/MdKit";
import { FAMILY_ICON, sportIcon } from "../components/TodayKit";
import { type ActivityDetail, barHeights, useActivityDetail } from "../data/activities";
import { useMd } from "../theme/md";

function Stats({ d }: { d: ActivityDetail }) {
  const md = useMd();
  const tiles = d.stats.map((s, i) => ({ ...s, accent: i < 2 }));
  return (
    <>
      <Card tone="primary" style={{ borderRadius: 32 }}>
        <View style={{ flexDirection: "row", alignItems: "center", gap: 16 }}>
          <IconTile
            name={FAMILY_ICON[d.family] ?? sportIcon(d.sportLabel)}
            tone="tertiary"
            size={56}
          />
          <View style={{ flex: 1 }}>
            <T v="title" color={md.onPrimaryContainer}>
              {d.sportLabel}
            </T>
            <T v="bodyMedium" color={md.onPrimaryContainer}>
              {d.when}
            </T>
          </View>
        </View>
      </Card>
      {tiles.length > 0 ? <TileGrid tiles={tiles} /> : null}
    </>
  );
}

function ZoneRow({ zone, percent, time, color }: ZoneRowProps) {
  const md = useMd();
  return (
    <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
      <View style={{ width: 28 }}>
        <T v="label">{`Z${zone}`}</T>
      </View>
      <View
        style={{
          flex: 1,
          height: 12,
          borderRadius: 6,
          backgroundColor: md.surfaceContainerHighest,
        }}
      >
        <View
          style={{
            width: `${Math.min(percent, 100)}%`,
            minWidth: percent > 0 ? 6 : 0,
            height: 12,
            borderRadius: 6,
            backgroundColor: color,
          }}
        />
      </View>
      <View style={{ width: 84 }}>
        <T v="bodyMedium" align="right">{`${time}, ${Math.round(percent)}%`}</T>
      </View>
    </View>
  );
}

type ZoneRowProps = { zone: number; percent: number; time: string; color: ColorValue };

function Zones({ z }: { z: NonNullable<ActivityDetail["zones"]> }) {
  const md = useMd();
  const tones = [md.outline, md.secondary, md.tertiary, md.primary, md.error];
  const color = (zone: number) => tones[zone - 1] ?? md.primary;
  return (
    <Card gap={12}>
      <T v="titleMedium">Heart rate zones</T>
      <SplitBar
        parts={z.rows
          .filter((r) => r.percent > 0)
          .map((r) => ({ weight: r.percent, color: color(r.zone) }))}
      />
      {z.rows.map((r) => (
        <ZoneRow key={r.zone} {...r} color={color(r.zone)} />
      ))}
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {z.sentence}
      </T>
    </Card>
  );
}

// The lowest and highest real sample, as one caption. A null sample is ignored.
function range(values: (number | null)[], unit: string): string[] {
  const real = values.filter((v): v is number => v !== null);
  if (real.length === 0) return [];
  return [
    `Min ${Math.round(Math.min(...real))} ${unit}`,
    `Max ${Math.round(Math.max(...real))} ${unit}`,
  ];
}

function Trace({
  title,
  values,
  unit,
  label,
  span,
}: {
  title: string;
  values: (number | null)[];
  unit: string;
  label: string;
  span?: string;
}) {
  const md = useMd();
  const heights = barHeights(values);
  if (heights.length === 0) return null;
  return (
    <Card gap={12}>
      <T v="titleMedium">{title}</T>
      <Bars
        values={heights}
        max={1}
        height={128}
        gap={1}
        describe={(_, i) => (i === 0 ? label : "")}
      />
      <BarLabels labels={range(values, unit)} />
      {span ? <T v="labelSmall" color={md.onSurfaceVariant}>{`Over ${span}.`}</T> : null}
    </Card>
  );
}

function ChartBlock({ c }: { c: NonNullable<ActivityDetail["chart"]> }) {
  const hasElevation = c.elevation.some((v) => v !== null);
  return (
    <>
      <Trace
        title="Heart rate"
        values={c.hr}
        unit="bpm"
        label="Heart rate over the session"
        span={hasElevation ? undefined : c.span}
      />
      {hasElevation ? (
        <Trace
          title="Elevation"
          values={c.elevation}
          unit="m"
          label="Elevation over the session"
          span={c.span}
        />
      ) : null}
    </>
  );
}

// Keep a column only when at least one row has a value for it.
function columns<R>(rows: R[], cols: [string, (r: R) => string | null][]) {
  const used = cols.filter(([, get]) => rows.some((r) => get(r) !== null));
  return {
    head: used.map(([h]) => h),
    rows: rows.map((r) => used.map(([, get]) => get(r) ?? "-")),
  };
}

function Table<R>({
  title,
  note,
  rows,
  cols,
}: {
  title: string;
  note?: string;
  rows: R[];
  cols: [string, (r: R) => string | null][];
}) {
  const md = useMd();
  const t = columns(rows, cols);
  return (
    <>
      <SectionLabel>{title}</SectionLabel>
      {note ? (
        <T v="bodyMedium" color={md.onSurfaceVariant}>
          {note}
        </T>
      ) : null}
      <DataTable head={t.head} rows={t.rows} />
    </>
  );
}

type Lap = ActivityDetail["laps"][number];
type StrengthSet = NonNullable<ActivityDetail["strength"]>["sets"][number];

const LAP_COLS: [string, (l: Lap) => string | null][] = [
  ["Lap", (l) => l.label],
  ["Distance", (l) => l.distance],
  ["Time", (l) => l.time],
  ["Pace", (l) => l.pace],
  ["HR", (l) => l.avgHr],
];

const SET_COLS: [string, (s: StrengthSet) => string | null][] = [
  ["Set", (s) => s.label],
  ["Exercise", (s) => s.exercise],
  ["Reps", (s) => s.reps],
  ["Work", (s) => s.work],
  ["Rest", (s) => s.rest],
];

function DetailBody({ d }: { d: ActivityDetail }) {
  return (
    <Content>
      <Stats d={d} />
      {d.zones ? <Zones z={d.zones} /> : null}
      {d.chart ? <ChartBlock c={d.chart} /> : null}
      {d.laps.length > 0 ? <Table title="Laps" rows={d.laps} cols={LAP_COLS} /> : null}
      {d.strength ? (
        <Table title="Sets" note={d.strength.sentence} rows={d.strength.sets} cols={SET_COLS} />
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
      {data ? <DetailBody d={data} /> : null}
    </LargeAppBarScreen>
  );
}
