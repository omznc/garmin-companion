// Activity detail on Android: a large app bar with a back button, then Material 3 cards.
// The core decides which blocks exist. A block with no data is not drawn.
import { Host } from "@expo/ui";
import { Box, Card, Column, FlowRow, Row, Text } from "@expo/ui/jetpack-compose";
import {
  background,
  fillMaxWidth,
  height,
  paddingAll,
  weight,
} from "@expo/ui/jetpack-compose/modifiers";
import { useLocalSearchParams } from "expo-router";
import type { ReactNode } from "react";
import { ActivityIndicator, View } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import { type ActivityDetail, barHeights, useActivityDetail } from "../data/activities";
import { HOST_SEED, useMd } from "../theme/md";

type Md = ReturnType<typeof useMd>;

function Block({ children }: { children: ReactNode }) {
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column verticalArrangement={{ spacedBy: 12 }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
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

function Stats({ md, d }: { md: Md; d: ActivityDetail }) {
  return (
    <Block>
      <Card colors={{ containerColor: md.surfaceContainerLow }}>
        <FlowRow
          horizontalArrangement={{ spacedBy: 24 }}
          verticalArrangement={{ spacedBy: 12 }}
          modifiers={[fillMaxWidth(), paddingAll(16)]}
        >
          {d.stats.map((s) => (
            <Column key={s.label}>
              <Text style={{ typography: "titleMedium" }} color={md.onSurface}>
                {s.value}
              </Text>
              <Text style={{ typography: "labelSmall" }} color={md.onSurfaceVariant}>
                {s.label}
              </Text>
            </Column>
          ))}
        </FlowRow>
      </Card>
    </Block>
  );
}

function Zones({ md, z }: { md: Md; z: NonNullable<ActivityDetail["zones"]> }) {
  const tones = [md.surfaceVariant, md.secondary, md.tertiary, md.primary, md.error];
  return (
    <Block>
      <Heading md={md}>Heart rate zones</Heading>
      <Row modifiers={[fillMaxWidth(), height(12)]}>
        {z.rows
          .filter((r) => r.percent > 0)
          .map((r) => (
            <Box
              key={r.zone}
              modifiers={[
                weight(Math.max(r.percent, 1)),
                height(12),
                background(tones[r.zone - 1] ?? md.primary),
              ]}
            />
          ))}
      </Row>
      <Body md={md}>{z.rows.map((r) => `Z${r.zone} ${r.time}`).join(", ")}</Body>
      <Body md={md}>{z.sentence}</Body>
    </Block>
  );
}

function Bars({ md, values, label }: { md: Md; values: (number | null)[]; label: string }) {
  const heights = barHeights(values);
  if (heights.length === 0) return null;
  return (
    <View
      accessible
      accessibilityLabel={label}
      style={{
        flexDirection: "row",
        alignItems: "flex-end",
        height: 80,
        gap: 1,
        marginHorizontal: 16,
      }}
    >
      {heights.map((h, i) => (
        <View
          // biome-ignore lint/suspicious/noArrayIndexKey: the samples never reorder
          key={i}
          style={{
            flex: 1,
            height: Math.max(h * 80, 1),
            backgroundColor: md.primary,
            opacity: h > 0 ? 1 : 0,
          }}
        />
      ))}
    </View>
  );
}

function ChartBlock({ md, c }: { md: Md; c: NonNullable<ActivityDetail["chart"]> }) {
  return (
    <>
      <Block>
        <Heading md={md}>Heart rate</Heading>
      </Block>
      <Bars md={md} values={c.hr} label="Heart rate over the session" />
      {c.elevation.length > 0 ? (
        <>
          <Block>
            <Heading md={md}>Elevation</Heading>
          </Block>
          <Bars md={md} values={c.elevation} label="Elevation over the session" />
        </>
      ) : null}
      <Block>
        <Body md={md}>{`Over ${c.span}.`}</Body>
      </Block>
    </>
  );
}

function Table({
  md,
  title,
  note,
  rows,
}: {
  md: Md;
  title: string;
  note?: string;
  rows: string[][];
}) {
  return (
    <Block>
      <Heading md={md}>{title}</Heading>
      {note ? <Body md={md}>{note}</Body> : null}
      <Card colors={{ containerColor: md.surfaceContainerLow }}>
        <Column verticalArrangement={{ spacedBy: 8 }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
          {rows.map((r) => (
            <Text key={r[0]} style={{ typography: "bodyMedium" }} color={md.onSurface}>
              {r.join("  ")}
            </Text>
          ))}
        </Column>
      </Card>
    </Block>
  );
}

function DetailBody({ md, d }: { md: Md; d: ActivityDetail }) {
  return (
    <>
      <Block>
        <Body md={md}>{`${d.sportLabel} - ${d.when}`}</Body>
      </Block>
      <Stats md={md} d={d} />
      {d.zones ? <Zones md={md} z={d.zones} /> : null}
      {d.chart ? <ChartBlock md={md} c={d.chart} /> : null}
      {d.laps.length > 0 ? (
        <Table
          md={md}
          title="Laps"
          rows={d.laps.map((l) => [l.label, l.distance ?? "-", l.time ?? "-", l.pace ?? "-"])}
        />
      ) : null}
      {d.strength ? (
        <Table
          md={md}
          title="Sets"
          note={d.strength.sentence}
          rows={d.strength.sets.map((s) => [
            s.label,
            `${s.reps ?? "-"} reps`,
            s.rest ? `rest ${s.rest}` : "",
            s.exercise ?? "",
          ])}
        />
      ) : null}
    </>
  );
}

export function ActivityDetailScreen() {
  const md = useMd();
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data, error, isPending } = useActivityDetail(Number(id));
  return (
    <LargeAppBarScreen title={data?.name ?? "Activity"} back>
      {isPending ? <ActivityIndicator style={{ margin: 32 }} /> : null}
      {error ? (
        <Block>
          <Body md={md}>{`The session could not be read. ${String(error)}`}</Body>
        </Block>
      ) : null}
      {data === null ? (
        <Block>
          <Body md={md}>This session is not in the cache.</Body>
        </Block>
      ) : null}
      {data ? <DetailBody md={md} d={data} /> : null}
    </LargeAppBarScreen>
  );
}
