// Weight, Food and Strength on Android: Material 3 from @expo/ui.
// The core sends the text. This file draws the blocks of `data/body.ts`.
import { Host } from "@expo/ui";
import { Button, Card, Column, FlowRow, Row, Text } from "@expo/ui/jetpack-compose";
import { fillMaxWidth, paddingAll, weight } from "@expo/ui/jetpack-compose/modifiers";
import type { ReactNode } from "react";
import { ActivityIndicator, View } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import { TrendBars } from "../components/TrendBars";
import { type Block, type BodyKind, useBody } from "../data/body";
import { HOST_SEED, useMd } from "../theme/md";

type Md = ReturnType<typeof useMd>;

function Pad({ children }: { children: ReactNode }) {
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column verticalArrangement={{ spacedBy: 12 }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
        {children}
      </Column>
    </Host>
  );
}

function Tiles({ md, tiles }: { md: Md; tiles: Extract<Block, { kind: "tiles" }>["tiles"] }) {
  return (
    <Pad>
      <FlowRow
        horizontalArrangement={{ spacedBy: 12 }}
        verticalArrangement={{ spacedBy: 12 }}
        modifiers={[fillMaxWidth()]}
      >
        {tiles.map((t) => (
          <Card key={t.label} colors={{ containerColor: md.surfaceContainerHigh }}>
            <Column modifiers={[paddingAll(16)]}>
              <Text
                style={{ typography: "labelMedium" }}
                color={t.accent ? md.primary : md.onSurfaceVariant}
              >
                {t.label}
              </Text>
              <Text style={{ typography: "headlineSmall" }} color={md.onSurface}>
                {t.value}
              </Text>
            </Column>
          </Card>
        ))}
      </FlowRow>
    </Pad>
  );
}

function Rows({ md, b }: { md: Md; b: Extract<Block, { kind: "rows" }> }) {
  return (
    <Pad>
      <Text style={{ typography: "titleMedium" }} color={md.onSurface}>
        {b.title}
      </Text>
      {b.rows.map((r, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: a date can repeat in a list
        <Row key={`${r.key}-${i}`} modifiers={[fillMaxWidth()]}>
          <Column modifiers={[weight(1)]}>
            <Text style={{ typography: "bodyLarge" }} color={md.onSurface}>
              {r.key}
            </Text>
            {r.sub ? (
              <Text style={{ typography: "bodySmall" }} color={md.onSurfaceVariant}>
                {r.sub}
              </Text>
            ) : null}
          </Column>
          <Text style={{ typography: "bodyLarge" }} color={md.onSurfaceVariant}>
            {r.value}
          </Text>
        </Row>
      ))}
    </Pad>
  );
}

function ChartBlock({ md, b }: { md: Md; b: Extract<Block, { kind: "chart" }> }) {
  return (
    <>
      <Pad>
        <Text style={{ typography: "titleMedium" }} color={md.onSurface}>
          {b.title}
        </Text>
      </Pad>
      <View style={{ paddingHorizontal: 16 }}>
        <TrendBars
          values={b.values}
          color={md.primary}
          label={`${b.title} from ${b.from} to ${b.to}`}
        />
      </View>
      <Pad>
        <Text style={{ typography: "labelSmall" }} color={md.onSurfaceVariant}>
          {`${b.from} to ${b.to}`}
        </Text>
      </Pad>
    </>
  );
}

function TextBlock({ md, b }: { md: Md; b: Extract<Block, { kind: "text" }> }) {
  const typography =
    b.tone === "heading" ? "titleMedium" : b.tone === "note" ? "bodySmall" : "bodyLarge";
  const color = b.tone === "lede" || b.tone === "heading" ? md.onSurface : md.onSurfaceVariant;
  return (
    <Pad>
      <Text style={{ typography }} color={color}>
        {b.text}
      </Text>
    </Pad>
  );
}

function BlockView({ md, b }: { md: Md; b: Block }) {
  switch (b.kind) {
    case "text":
      return <TextBlock md={md} b={b} />;
    case "tiles":
      return <Tiles md={md} tiles={b.tiles} />;
    case "chart":
      return <ChartBlock md={md} b={b} />;
    case "rows":
      return <Rows md={md} b={b} />;
    case "empty":
      return (
        <Pad>
          <Text style={{ typography: "titleMedium" }} color={md.onSurface}>
            {b.title}
          </Text>
          <Text style={{ typography: "bodyMedium" }} color={md.onSurfaceVariant}>
            {b.body}
          </Text>
        </Pad>
      );
  }
}

export function BodyScreen({ kind, title }: { kind: BodyKind; title: string }) {
  const md = useMd();
  const { data, error, refetch } = useBody(kind);
  return (
    <LargeAppBarScreen title={title} settings={false}>
      {data?.blocks.map((b, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: the blocks come in a fixed order
        <BlockView key={i} md={md} b={b} />
      ))}
      {!data && !error ? <ActivityIndicator style={{ margin: 32 }} /> : null}
      {error ? (
        <Pad>
          <Text style={{ typography: "bodyLarge" }} color={md.error}>
            {String(error)}
          </Text>
          <Button onClick={() => refetch()}>
            <Text>Try again</Text>
          </Button>
        </Pad>
      ) : null}
    </LargeAppBarScreen>
  );
}
