// Weight, Food and Strength on Android: Material 3 Expressive cards.
// The core sends the text. This file draws the blocks of `data/body.ts`.
import type { UseQueryResult } from "@tanstack/react-query";
import { View } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import {
  BarLabels,
  Bars,
  Card,
  Content,
  ErrorCard,
  Group,
  ListRow,
  Loading,
  SectionLabel,
  T,
  TileGrid,
} from "../components/MdKit";
import { RouteTrace, WearBar } from "../components/RouteTrace";
import { type Block, type BodyKind, type BodyScreenData, useBody } from "../data/body";
import { useMd } from "../theme/md";

const MAX_BARS = 48;

// shortcut: keep every nth value above 48 values, the chart is a shape and not a reading.
function thin(values: number[]): number[] {
  if (values.length <= MAX_BARS) return values;
  const step = values.length / MAX_BARS;
  return Array.from({ length: MAX_BARS }, (_, i) => values[Math.floor(i * step)] ?? 0);
}

/** The chart scales from the lowest value, so a change shows. */
function ChartBlock({ b }: { b: Extract<Block, { kind: "chart" }> }) {
  const bars = thin(b.values);
  const low = Math.min(...bars);
  const span = Math.max(Math.max(...bars) - low, 1e-6);
  return (
    <Card gap={12}>
      <T v="titleMedium">{b.title}</T>
      <Bars
        values={bars.map((v) => 0.12 + ((v - low) / span) * 0.88)}
        max={1}
        height={112}
        gap={2}
        describe={(_, i) => (i === 0 ? `${b.title} from ${b.from} to ${b.to}` : "")}
      />
      <BarLabels labels={[b.from, b.to]} />
    </Card>
  );
}

function TextBlock({ b }: { b: Extract<Block, { kind: "text" }> }) {
  const md = useMd();
  if (b.tone === "heading") return <SectionLabel>{b.text}</SectionLabel>;
  if (b.tone === "lede") {
    return (
      <Card tone="primary" style={{ borderRadius: 32 }}>
        <T v="body" color={md.onPrimaryContainer}>
          {b.text}
        </T>
      </Card>
    );
  }
  return (
    <View style={{ paddingHorizontal: 4 }}>
      <T v={b.tone === "note" ? "bodyMedium" : "body"} color={md.onSurfaceVariant}>
        {b.text}
      </T>
    </View>
  );
}

function RowsBlock({ b }: { b: Extract<Block, { kind: "rows" }> }) {
  const md = useMd();
  return (
    <>
      <SectionLabel>{b.title}</SectionLabel>
      <Group>
        {b.rows.map((r, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: a date can repeat in a list
          <View key={`${r.key}-${i}`}>
            <ListRow
              title={r.key}
              supporting={r.sub}
              trailing={
                <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
                  {r.trace?.length ? <RouteTrace points={r.trace} color={md.primary} /> : null}
                  <T v="titleMedium" color={md.onSurfaceVariant}>
                    {r.value}
                  </T>
                </View>
              }
            />
            {r.bar ? (
              <WearBar
                fraction={r.bar.fraction}
                color={r.bar.accent ? md.tertiary : md.primary}
                track={md.surfaceContainerHighest}
              />
            ) : null}
          </View>
        ))}
      </Group>
    </>
  );
}

function BlockView({ b }: { b: Block }) {
  const md = useMd();
  switch (b.kind) {
    case "text":
      return <TextBlock b={b} />;
    case "tiles":
      return <TileGrid tiles={b.tiles} />;
    case "chart":
      return <ChartBlock b={b} />;
    case "rows":
      return <RowsBlock b={b} />;
    case "empty":
      return (
        <Card tone="secondary">
          <T v="titleMedium" color={md.onSecondaryContainer}>
            {b.title}
          </T>
          <T v="bodyMedium" color={md.onSecondaryContainer}>
            {b.body}
          </T>
        </Card>
      );
  }
}

type Query = Pick<UseQueryResult<BodyScreenData>, "data" | "error" | "refetch">;

/** Any screen that is a list of blocks. */
export function BlocksScreen({ title, query }: { title: string; query: Query }) {
  const { data, error, refetch } = query;
  return (
    <LargeAppBarScreen title={title} back>
      {data ? (
        <Content>
          {data.blocks.map((b, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: the blocks come in a fixed order
            <BlockView key={i} b={b} />
          ))}
        </Content>
      ) : null}
      {!data && !error ? <Loading /> : null}
      {error ? <ErrorCard message={String(error)} onRetry={() => refetch()} /> : null}
    </LargeAppBarScreen>
  );
}

export function BodyScreen({ kind, title }: { kind: BodyKind; title: string }) {
  return <BlocksScreen title={title} query={useBody(kind)} />;
}
