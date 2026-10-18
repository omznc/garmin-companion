// Today on Android: Material 3 Expressive cards in the colours of the scheme.
// The core sends the sections in order. A section that has no data is not drawn.
import { View } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import {
  BarLabels,
  Bars,
  Card,
  Content,
  ErrorCard,
  Loading,
  type Md,
  SectionLabel,
  SplitBar,
  T,
  TileGrid,
} from "../components/MdKit";
import type { LastSession, Section as TodaySection, Today } from "../data/types";
import { useSync } from "../data/useSync";
import { useToday } from "../data/useToday";
import { useMd } from "../theme/md";

function zoneTones(md: Md) {
  return [md.outlineVariant, md.secondary, md.tertiary, md.primary, md.error];
}

function SessionCard({ s }: { s: LastSession }) {
  const md = useMd();
  const tones = zoneTones(md);
  const sub = [s.sport, s.when, s.zones ? `${s.zones.aboveZ2Pct}% above Z2` : null];
  return (
    <Card gap={12}>
      <T v="title">{s.name}</T>
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {sub.filter(Boolean).join(" - ")}
      </T>
      {s.zones ? (
        <SplitBar
          parts={s.zones.percents.map((p, i) => ({ weight: p, color: tones[i] ?? md.primary }))}
        />
      ) : null}
      <View style={{ flexDirection: "row", flexWrap: "wrap", columnGap: 28, rowGap: 12 }}>
        {s.stats.map((stat) => (
          <View key={stat.label}>
            <T v="headlineSmall">{stat.value}</T>
            <T v="labelSmall" color={md.onSurfaceVariant}>
              {stat.label}
            </T>
          </View>
        ))}
      </View>
    </Card>
  );
}

function WeekCard({ s }: { s: Extract<TodaySection, { kind: "week" }> }) {
  const md = useMd();
  const any = s.minutes.some((m) => m > 0);
  return (
    <Card gap={12}>
      <T v="titleMedium">Last seven days</T>
      {any ? (
        <>
          <Bars
            values={s.minutes}
            gap={8}
            describe={(v, i) => `${s.labels[i]}: ${Math.round(v ?? 0)} min`}
          />
          <BarLabels labels={s.labels} />
        </>
      ) : null}
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {s.summary}
      </T>
    </Card>
  );
}

function DriftCard({ s }: { s: Extract<TodaySection, { kind: "drift" }> }) {
  const md = useMd();
  const labels = s.runs.map((r) => r.label);
  return (
    <Card gap={12}>
      <T v="titleMedium">Zone drift</T>
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {s.summary}
      </T>
      <Bars
        values={s.runs.map((r) => r.aboveZ2Pct)}
        max={100}
        gap={8}
        describe={(v, i) => `${labels[i]}: ${Math.round(v ?? 0)} %`}
      />
      <BarLabels labels={labels} />
    </Card>
  );
}

function SectionView({ s }: { s: TodaySection }) {
  const md = useMd();
  switch (s.kind) {
    case "health":
      return (
        <TileGrid
          tiles={s.metrics.map((m) => ({
            label: m.label,
            value: m.unit ? `${m.value} ${m.unit}` : m.value,
            accent: m.accent,
          }))}
        />
      );
    case "attention":
      return (
        <>
          <SectionLabel>Attention</SectionLabel>
          <Card tone="tertiary" gap={12}>
            {s.notes.map((n) => (
              <T key={n.text} v={n.accent ? "titleMedium" : "body"} color={md.onTertiaryContainer}>
                {n.text}
              </T>
            ))}
          </Card>
        </>
      );
    case "text":
      return (
        <View style={{ paddingHorizontal: 4 }}>
          <T v="body" color={md.onSurfaceVariant}>
            {s.text}
          </T>
        </View>
      );
    case "session":
      return (
        <>
          <SectionLabel>Last session</SectionLabel>
          <SessionCard s={s.session} />
        </>
      );
    case "week":
      return <WeekCard s={s} />;
    case "drift":
      return <DriftCard s={s} />;
  }
}

function TodayBody({ today }: { today: Today }) {
  const md = useMd();
  return (
    <Content>
      <Card tone="primary" gap={12} style={{ borderRadius: 32 }}>
        <T v="label" color={md.onPrimaryContainer}>
          {today.dateLabel}
        </T>
        <T v="body" color={md.onPrimaryContainer}>
          {today.lede}
        </T>
      </Card>
      {today.sections.map((s) => (
        <SectionView key={s.kind === "text" ? s.key : s.kind} s={s} />
      ))}
    </Content>
  );
}

export function TodayScreen() {
  const { data, error, refetch } = useToday();
  const md = useMd();
  const sync = useSync();
  return (
    <LargeAppBarScreen
      title={data?.greeting ?? "Today"}
      refreshing={sync.busy}
      onRefresh={() => sync.refresh()}
    >
      {sync.line || sync.error ? (
        <View style={{ paddingHorizontal: 20 }}>
          <T v="labelSmall" color={sync.error ? md.error : md.onSurfaceVariant}>
            {sync.error ?? sync.line ?? ""}
          </T>
        </View>
      ) : null}
      {data ? <TodayBody today={data} /> : null}
      {!data && !error ? <Loading /> : null}
      {error ? <ErrorCard message={String(error)} onRetry={refetch} /> : null}
    </LargeAppBarScreen>
  );
}
