// Today on Android: Material 3 components from @expo/ui (Jetpack Compose).
// The core sends the sections in order. A section that has no data is not drawn.
import { Host } from "@expo/ui";
import { Box, Button, Card, Column, FlowRow, Row, Text } from "@expo/ui/jetpack-compose";
import {
  background,
  fillMaxWidth,
  height,
  paddingAll,
  weight,
} from "@expo/ui/jetpack-compose/modifiers";
import type { ReactNode } from "react";
import { ActivityIndicator, View } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import type { LastSession, Metric, Section as TodaySection, Today } from "../data/types";
import { useToday } from "../data/useToday";
import { HOST_SEED, useMd } from "../theme/md";

type Md = ReturnType<typeof useMd>;

const GAP = 12;

function Block({ children }: { children: ReactNode }) {
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column verticalArrangement={{ spacedBy: GAP }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
        {children}
      </Column>
    </Host>
  );
}

function Heading({ md, children }: { md: Md; children: string }) {
  return (
    <Text style={{ typography: "titleMedium" }} color={md.onSurface}>
      {children}
    </Text>
  );
}

function Body({ md, children }: { md: Md; children: string }) {
  return (
    <Text style={{ typography: "bodyMedium" }} color={md.onSurfaceVariant}>
      {children}
    </Text>
  );
}

function MetricTile({ md, m }: { md: Md; m: Metric }) {
  return (
    <Card colors={{ containerColor: md.surfaceContainerHigh }} modifiers={[weight(1)]}>
      <Column modifiers={[fillMaxWidth(), paddingAll(16)]}>
        <Text
          style={{ typography: "labelMedium" }}
          color={m.accent ? md.primary : md.onSurfaceVariant}
        >
          {m.label}
        </Text>
        <Text style={{ typography: "headlineMedium" }} color={md.onSurface}>
          {m.unit ? `${m.value} ${m.unit}` : m.value}
        </Text>
      </Column>
    </Card>
  );
}

function pairs<T>(items: T[]): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += 2) out.push(items.slice(i, i + 2));
  return out;
}

function HealthBlock({ md, metrics }: { md: Md; metrics: Metric[] }) {
  return (
    <Block>
      {pairs(metrics).map((row) => (
        <Row
          key={row[0]?.id}
          horizontalArrangement={{ spacedBy: GAP }}
          modifiers={[fillMaxWidth()]}
        >
          {row.map((m) => (
            <MetricTile key={m.id} md={md} m={m} />
          ))}
        </Row>
      ))}
    </Block>
  );
}

function ZoneStrip({ md, percents }: { md: Md; percents: number[] }) {
  const tones = [md.surfaceVariant, md.secondary, md.tertiary, md.primary, md.error];
  return (
    <Row modifiers={[fillMaxWidth(), height(12)]}>
      {percents.map((pct, i) => (
        <Box
          key={`zone-${i + 1}`}
          modifiers={[weight(Math.max(pct, 1)), height(12), background(tones[i] ?? md.primary)]}
        />
      ))}
    </Row>
  );
}

function SessionCard({ md, s }: { md: Md; s: LastSession }) {
  const sub = [s.sport, s.when, s.zones ? `${s.zones.aboveZ2Pct}% above Z2` : null];
  return (
    <Card colors={{ containerColor: md.surfaceContainerLow }}>
      <Column verticalArrangement={{ spacedBy: 8 }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
        <Text style={{ typography: "titleLarge" }} color={md.onSurface}>
          {s.name}
        </Text>
        <Body md={md}>{sub.filter(Boolean).join(" - ")}</Body>
        {s.zones ? <ZoneStrip md={md} percents={s.zones.percents} /> : null}
        <FlowRow
          horizontalArrangement={{ spacedBy: 24 }}
          verticalArrangement={{ spacedBy: 12 }}
          modifiers={[fillMaxWidth()]}
        >
          {s.stats.map((stat) => (
            <Column key={stat.label}>
              <Text style={{ typography: "titleMedium" }} color={md.onSurface}>
                {stat.value}
              </Text>
              <Text style={{ typography: "labelSmall" }} color={md.onSurfaceVariant}>
                {stat.label}
              </Text>
            </Column>
          ))}
        </FlowRow>
      </Column>
    </Card>
  );
}

function AttentionCard({ md, notes }: { md: Md; notes: { text: string; accent: boolean }[] }) {
  return (
    <Card colors={{ containerColor: md.surfaceContainerLow }}>
      <Column verticalArrangement={{ spacedBy: 12 }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
        {notes.map((n) => (
          <Text
            key={n.text}
            style={{ typography: "bodyLarge" }}
            color={n.accent ? md.primary : md.onSurface}
          >
            {n.text}
          </Text>
        ))}
      </Column>
    </Card>
  );
}

function Bars({
  md,
  values,
  labels,
  unit,
  max = Math.max(...values, 1),
}: {
  md: Md;
  values: number[];
  labels: string[];
  unit: string;
  max?: number;
}) {
  return (
    <View
      style={{
        flexDirection: "row",
        alignItems: "flex-end",
        height: 96,
        gap: 8,
        paddingHorizontal: 16,
      }}
    >
      {values.map((v, i) => (
        <View
          // biome-ignore lint/suspicious/noArrayIndexKey: the bars never reorder
          key={i}
          accessible
          accessibilityLabel={`${labels[i]}: ${Math.round(v)} ${unit}`}
          style={{
            flex: 1,
            height: Math.max((v / max) * 96, 4),
            borderRadius: 8,
            backgroundColor: v > 0 ? md.primary : md.surfaceVariant,
          }}
        />
      ))}
    </View>
  );
}

function BarLabels({ md, labels }: { md: Md; labels: string[] }) {
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Row modifiers={[fillMaxWidth(), paddingAll(16)]}>
        {labels.map((l, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: the labels never reorder
          <Box key={i} modifiers={[weight(1)]} contentAlignment="center">
            <Text style={{ typography: "labelSmall" }} color={md.onSurfaceVariant}>
              {l}
            </Text>
          </Box>
        ))}
      </Row>
    </Host>
  );
}

function WeekBlock({ md, s }: { md: Md; s: Extract<TodaySection, { kind: "week" }> }) {
  return (
    <>
      <Block>
        <Heading md={md}>Last seven days</Heading>
      </Block>
      {s.minutes.some((m) => m > 0) ? (
        <>
          <Bars md={md} values={s.minutes} labels={s.labels} unit="min" />
          <BarLabels md={md} labels={s.labels} />
        </>
      ) : null}
      <Block>
        <Body md={md}>{s.summary}</Body>
      </Block>
    </>
  );
}

function DriftBlock({ md, s }: { md: Md; s: Extract<TodaySection, { kind: "drift" }> }) {
  const labels = s.runs.map((r) => r.label);
  return (
    <>
      <Block>
        <Heading md={md}>Zone drift</Heading>
        <Body md={md}>{s.summary}</Body>
      </Block>
      <Bars md={md} values={s.runs.map((r) => r.aboveZ2Pct)} labels={labels} unit="%" max={100} />
      <BarLabels md={md} labels={labels} />
    </>
  );
}

function SectionView({ md, s }: { md: Md; s: TodaySection }) {
  switch (s.kind) {
    case "health":
      return <HealthBlock md={md} metrics={s.metrics} />;
    case "attention":
      return (
        <Block>
          <Heading md={md}>Attention</Heading>
          <AttentionCard md={md} notes={s.notes} />
        </Block>
      );
    case "text":
      return (
        <Block>
          <Body md={md}>{s.text}</Body>
        </Block>
      );
    case "session":
      return (
        <Block>
          <Heading md={md}>Last session</Heading>
          <SessionCard md={md} s={s.session} />
        </Block>
      );
    case "week":
      return <WeekBlock md={md} s={s} />;
    case "drift":
      return <DriftBlock md={md} s={s} />;
  }
}

function TodayBody({ today }: { today: Today }) {
  const md = useMd();
  return (
    <>
      <Block>
        <Text style={{ typography: "labelLarge" }} color={md.onSurfaceVariant}>
          {today.dateLabel}
        </Text>
        <Text style={{ typography: "bodyLarge" }} color={md.onSurface}>
          {today.lede}
        </Text>
      </Block>
      {today.sections.map((s) => (
        <SectionView key={s.kind === "text" ? s.key : s.kind} md={md} s={s} />
      ))}
    </>
  );
}

export function TodayScreen() {
  const { data, error, refetch } = useToday();
  return (
    <LargeAppBarScreen title={data?.greeting ?? "Today"}>
      {data ? <TodayBody today={data} /> : null}
      {!data && !error ? <ActivityIndicator style={{ margin: 32 }} /> : null}
      {error ? <ErrorBlock message={String(error)} onRetry={refetch} /> : null}
    </LargeAppBarScreen>
  );
}

function ErrorBlock({ message, onRetry }: { message: string; onRetry: () => void }) {
  const md = useMd();
  return (
    <Block>
      <Text style={{ typography: "bodyLarge" }} color={md.error}>
        {message}
      </Text>
      <Button onClick={onRetry}>
        <Text>Try again</Text>
      </Button>
    </Block>
  );
}
