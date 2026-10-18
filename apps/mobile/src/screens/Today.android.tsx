// Today on Android: Material 3 components from @expo/ui (Jetpack Compose).
import { Host } from "@expo/ui";
import { Box, Card, Column, FlowRow, Row, Text } from "@expo/ui/jetpack-compose";
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
import type { Metric, Today } from "../data/types";
import { useToday } from "../data/useToday";
import { HOST_SEED, useMd } from "../theme/md";

type Md = ReturnType<typeof useMd>;

const GAP = 12;

function Section({ children }: { children: ReactNode }) {
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

function MetricTile({ colors, m }: { colors: Md; m: Metric }) {
  return (
    <Card colors={{ containerColor: colors.surfaceContainerHigh }} modifiers={[weight(1)]}>
      <Column modifiers={[fillMaxWidth(), paddingAll(16)]}>
        <Text
          style={{ typography: "labelMedium" }}
          color={m.accent ? colors.primary : colors.onSurfaceVariant}
        >
          {m.label}
        </Text>
        <Text style={{ typography: "headlineMedium" }} color={colors.onSurface}>
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

function MetricGrid({ md, metrics }: { md: Md; metrics: Metric[] }) {
  return (
    <>
      {pairs(metrics).map((row) => (
        <Row
          key={row[0]?.id}
          horizontalArrangement={{ spacedBy: GAP }}
          modifiers={[fillMaxWidth()]}
        >
          {row.map((m) => (
            <MetricTile key={m.id} colors={md} m={m} />
          ))}
        </Row>
      ))}
    </>
  );
}

function ZoneStrip({ md, zones }: { md: Md; zones: number[] }) {
  const tones = [md.surfaceVariant, md.secondary, md.tertiary, md.primary, md.error];
  return (
    <Row modifiers={[fillMaxWidth(), height(12)]}>
      {zones.map((pct, i) => (
        <Box
          key={tones[i]}
          modifiers={[weight(Math.max(pct, 1)), height(12), background(tones[i] ?? md.primary)]}
        />
      ))}
    </Row>
  );
}

function LastSessionCard({ md, today }: { md: Md; today: Today }) {
  const s = today.lastSession;
  if (!s) return null;
  return (
    <Card colors={{ containerColor: md.surfaceContainerLow }}>
      <Column verticalArrangement={{ spacedBy: 8 }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
        <Text style={{ typography: "titleLarge" }} color={md.onSurface}>
          {s.name}
        </Text>
        <Text style={{ typography: "bodyMedium" }} color={md.onSurfaceVariant}>
          {[s.sport, s.when, s.aboveZ2Pct == null ? null : `${s.aboveZ2Pct}% above Z2`]
            .filter(Boolean)
            .join(" - ")}
        </Text>
        <ZoneStrip md={md} zones={s.zones} />
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

function AttentionCard({ md, today }: { md: Md; today: Today }) {
  if (!today.attention.length) return null;
  return (
    <Card colors={{ containerColor: md.surfaceContainerLow }}>
      <Column verticalArrangement={{ spacedBy: 12 }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
        {today.attention.map((n) => (
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
  max = Math.max(...values, 1),
}: {
  md: Md;
  values: number[];
  labels: string[];
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
          key={labels[i]}
          accessible
          accessibilityLabel={`${labels[i]}: ${v}`}
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
        {labels.map((l) => (
          <Box key={l} modifiers={[weight(1)]} contentAlignment="center">
            <Text style={{ typography: "labelSmall" }} color={md.onSurfaceVariant}>
              {l}
            </Text>
          </Box>
        ))}
      </Row>
    </Host>
  );
}

function HealthSection({ md, today }: { md: Md; today: Today }) {
  if (!today.metrics.length && !today.fuelLine) return null;
  return (
    <Section>
      <MetricGrid md={md} metrics={today.metrics} />
      {today.fuelLine ? (
        <Text style={{ typography: "bodyMedium" }} color={md.onSurfaceVariant}>
          {today.fuelLine}
        </Text>
      ) : null}
    </Section>
  );
}

function AttentionSection({ md, today }: { md: Md; today: Today }) {
  if (!today.attention.length) return null;
  return (
    <Section>
      <Heading md={md}>Attention</Heading>
      <AttentionCard md={md} today={today} />
    </Section>
  );
}

function LastSessionSection({ md, today }: { md: Md; today: Today }) {
  if (!today.lastSession) return null;
  return (
    <Section>
      <Heading md={md}>Last session</Heading>
      <LastSessionCard md={md} today={today} />
    </Section>
  );
}

function DriftSection({ md, today }: { md: Md; today: Today }) {
  const drift = today.zoneDrift;
  if (!drift) return null;
  const labels = drift.runs.map((r) => r.label);
  return (
    <>
      <Section>
        <Heading md={md}>Zone drift</Heading>
        <Text style={{ typography: "bodyMedium" }} color={md.onSurfaceVariant}>
          {drift.summary}
        </Text>
      </Section>
      <Bars md={md} values={drift.runs.map((r) => r.aboveZ2Pct)} labels={labels} max={100} />
      <BarLabels md={md} labels={labels} />
    </>
  );
}

function WeekSection({ md, today }: { md: Md; today: Today }) {
  const week = today.week;
  if (!week) return null;
  return (
    <>
      <Section>
        <Heading md={md}>Last seven days</Heading>
      </Section>
      {week.km.some((km) => km > 0) ? (
        <>
          <Bars md={md} values={week.km} labels={week.labels} />
          <BarLabels md={md} labels={week.labels} />
        </>
      ) : null}
      <Section>
        <Text style={{ typography: "bodyMedium" }} color={md.onSurfaceVariant}>
          {week.summary}
        </Text>
      </Section>
    </>
  );
}

// Each section hides when it has no data. Health comes first.
function TodayBody({ today }: { today: Today }) {
  const md = useMd();
  return (
    <>
      <Section>
        <Text style={{ typography: "labelLarge" }} color={md.onSurfaceVariant}>
          {today.dateLabel}
        </Text>
        <Text style={{ typography: "bodyLarge" }} color={md.onSurface}>
          {today.lede}
        </Text>
      </Section>
      <HealthSection md={md} today={today} />
      <AttentionSection md={md} today={today} />
      <LastSessionSection md={md} today={today} />
      <DriftSection md={md} today={today} />
      <WeekSection md={md} today={today} />
    </>
  );
}

export function TodayScreen() {
  const { data } = useToday();
  return (
    <LargeAppBarScreen title={data?.greeting ?? "Today"}>
      {data ? <TodayBody today={data} /> : <ActivityIndicator style={{ margin: 32 }} />}
    </LargeAppBarScreen>
  );
}
