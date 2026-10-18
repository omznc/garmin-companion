// The Material 3 pieces that the Health and Sleep screens share.
import { Host } from "@expo/ui";
import { Box, Button, Card, Column, Row, Text } from "@expo/ui/jetpack-compose";
import { fillMaxWidth, paddingAll, weight } from "@expo/ui/jetpack-compose/modifiers";
import type { ReactNode } from "react";
import { ActivityIndicator } from "react-native";
import { HOST_SEED, useMd } from "../theme/md";

export type Md = ReturnType<typeof useMd>;

export const GAP = 12;

export function Block({ children }: { children: ReactNode }) {
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column verticalArrangement={{ spacedBy: GAP }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
        {children}
      </Column>
    </Host>
  );
}

export function Heading({ md, children }: { md: Md; children: string }) {
  return (
    <Text style={{ typography: "titleMedium" }} color={md.onSurface}>
      {children}
    </Text>
  );
}

export function Body({ md, children }: { md: Md; children: string }) {
  return (
    <Text style={{ typography: "bodyMedium" }} color={md.onSurfaceVariant}>
      {children}
    </Text>
  );
}

export function Caption({ md, children }: { md: Md; children: string }) {
  return (
    <Text style={{ typography: "labelMedium" }} color={md.onSurfaceVariant}>
      {children}
    </Text>
  );
}

export function Tile({ md, label, value }: { md: Md; label: string; value: string }) {
  return (
    <Card colors={{ containerColor: md.surfaceContainerHigh }} modifiers={[weight(1)]}>
      <Column modifiers={[fillMaxWidth(), paddingAll(16)]}>
        <Text style={{ typography: "labelMedium" }} color={md.onSurfaceVariant}>
          {label}
        </Text>
        <Text style={{ typography: "headlineMedium" }} color={md.onSurface}>
          {value}
        </Text>
      </Column>
    </Card>
  );
}

/** Tiles in rows of two. A lone tile keeps the width of one. */
export function TileGrid({ md, tiles }: { md: Md; tiles: { label: string; value: string }[] }) {
  const rows: (typeof tiles)[] = [];
  for (let i = 0; i < tiles.length; i += 2) rows.push(tiles.slice(i, i + 2));
  return (
    <Block>
      {rows.map((row) => (
        <Row
          key={row[0]?.label}
          horizontalArrangement={{ spacedBy: GAP }}
          modifiers={[fillMaxWidth()]}
        >
          {row.map((t) => (
            <Tile key={t.label} md={md} label={t.label} value={t.value} />
          ))}
          {row.length === 1 ? <Box modifiers={[weight(1)]} /> : null}
        </Row>
      ))}
    </Block>
  );
}

/** One label under each bar, or one at each end of a long window. */
export function BarLabels({ md, labels }: { md: Md; labels: string[] }) {
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Row
        horizontalArrangement={labels.length === 2 ? "spaceBetween" : undefined}
        modifiers={[fillMaxWidth(), paddingAll(16)]}
      >
        {labels.map((l, i) =>
          labels.length === 2 ? (
            <Caption key={l} md={md}>
              {l}
            </Caption>
          ) : (
            // biome-ignore lint/suspicious/noArrayIndexKey: the labels never reorder
            <Box key={i} modifiers={[weight(1)]} contentAlignment="center">
              <Text style={{ typography: "labelSmall" }} color={md.onSurfaceVariant}>
                {l}
              </Text>
            </Box>
          ),
        )}
      </Row>
    </Host>
  );
}

export function Loading() {
  return <ActivityIndicator style={{ margin: 32 }} />;
}

export function ErrorBlock({ message, onRetry }: { message: string; onRetry: () => void }) {
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
