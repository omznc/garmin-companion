// Sleep on Android: last night, the stage bar, a 7 or 30 night window and what the nights say.
// It needs no activity. The core sends every figure and sentence.
import { Host } from "@expo/ui";
import {
  Box,
  Card,
  Column,
  FlowRow,
  Row,
  SegmentedButton,
  SingleChoiceSegmentedButtonRow,
  Text,
} from "@expo/ui/jetpack-compose";
import {
  background,
  fillMaxWidth,
  height,
  paddingAll,
  weight,
} from "@expo/ui/jetpack-compose/modifiers";
import { useState } from "react";
import { View } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import {
  BarLabels,
  Block,
  Body,
  Caption,
  ErrorBlock,
  Heading,
  Loading,
  TileGrid,
} from "../components/MdBlocks";
import { MiniBars } from "../components/MiniBars";
import {
  type NightView,
  type SleepInsight,
  type SleepView,
  type SleepWindow,
  shortDay,
  useSleep,
} from "../data/wellness";
import { HOST_SEED, useMd } from "../theme/md";

type Md = ReturnType<typeof useMd>;

/** A night under seven hours is a short night. */
const SHORT_NIGHT_HOURS = 7;
const FULL_BAR_HOURS = 10;

function stageTone(md: Md, key: string): string {
  const tones: Record<string, string> = {
    deep: md.primary,
    rem: md.tertiary,
    light: md.secondary,
    awake: md.error,
  };
  return tones[key] ?? md.primary;
}

function LastNight({ md, night, note }: { md: Md; night: NightView; note: string | null }) {
  const tiles = [
    night.score === null
      ? null
      : {
          label: night.qualifier ? `Score, ${night.qualifier}` : "Score",
          value: String(Math.round(night.score)),
        },
    { label: "Duration", value: night.durationText },
    night.start && night.end ? { label: "Asleep", value: night.start } : null,
    night.start && night.end ? { label: "Woke", value: night.end } : null,
  ].filter((t): t is { label: string; value: string } => t !== null);
  return (
    <>
      <TileGrid md={md} tiles={tiles} />
      {night.stages.length > 0 ? (
        <Block>
          <Row modifiers={[fillMaxWidth(), height(12)]}>
            {night.stages.map((s) => (
              <Box
                key={s.key}
                modifiers={[
                  weight(Math.max(s.pct, 1)),
                  height(12),
                  background(stageTone(md, s.key)),
                ]}
              />
            ))}
          </Row>
          <FlowRow
            horizontalArrangement={{ spacedBy: 16 }}
            verticalArrangement={{ spacedBy: 4 }}
            modifiers={[fillMaxWidth()]}
          >
            {night.stages.map((s) => (
              <Text key={s.key} style={{ typography: "labelMedium" }} color={stageTone(md, s.key)}>
                {`${s.label} ${s.text}`}
              </Text>
            ))}
          </FlowRow>
        </Block>
      ) : null}
      {note ? (
        <Block>
          <Caption md={md}>{note}</Caption>
        </Block>
      ) : null}
    </>
  );
}

function WindowPicker({ days, onPick }: { days: number; onPick: (days: number) => void }) {
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column modifiers={[fillMaxWidth(), paddingAll(16)]}>
        <SingleChoiceSegmentedButtonRow modifiers={[fillMaxWidth()]}>
          {[7, 30].map((d) => (
            <SegmentedButton key={d} selected={days === d} onClick={() => onPick(d)}>
              <SegmentedButton.Label>
                <Text>{`${d} nights`}</Text>
              </SegmentedButton.Label>
            </SegmentedButton>
          ))}
        </SingleChoiceSegmentedButtonRow>
      </Column>
    </Host>
  );
}

function Window({ md, w }: { md: Md; w: SleepWindow }) {
  const labels =
    w.days <= 7
      ? w.bars.map((b) => b.label)
      : [shortDay(w.bars[0]?.date ?? ""), shortDay(w.bars.at(-1)?.date ?? "")];
  return (
    <>
      <MiniBars
        values={w.bars.map((b) => b.hours)}
        max={FULL_BAR_HOURS}
        height={96}
        gap={w.days <= 7 ? 8 : 3}
        color={md.primary}
        trackColor={md.surfaceVariant}
        colorFor={(h) => (h < SHORT_NIGHT_HOURS ? md.tertiary : md.primary)}
        describe={(h, i) => {
          const bar = w.bars[i];
          const day = bar ? shortDay(bar.date) : "";
          return h === null ? `${day}: no sleep recorded` : `${day}: ${h.toFixed(1)} hours`;
        }}
      />
      <BarLabels md={md} labels={labels} />
      <Block>
        <Body md={md}>{w.summary}</Body>
      </Block>
    </>
  );
}

function Insights({ md, items }: { md: Md; items: SleepInsight[] }) {
  return (
    <Block>
      <Heading md={md}>What your nights say</Heading>
      {items.map((i) => (
        <Card key={i.id} colors={{ containerColor: md.surfaceContainerLow }}>
          <Column
            verticalArrangement={{ spacedBy: 6 }}
            modifiers={[fillMaxWidth(), paddingAll(16)]}
          >
            <Text
              style={{ typography: "bodyLarge" }}
              color={i.tone === "watch" ? md.primary : md.onSurface}
            >
              {i.claim}
            </Text>
            <Text style={{ typography: "bodyMedium" }} color={md.onSurfaceVariant}>
              {i.detail}
            </Text>
          </Column>
        </Card>
      ))}
    </Block>
  );
}

function SleepBody({ view }: { view: SleepView }) {
  const md = useMd();
  const [days, setDays] = useState(7);
  return (
    <>
      <Block>
        {view.lastNight ? (
          <Text style={{ typography: "labelLarge" }} color={md.onSurfaceVariant}>
            {view.lastNight.title}
          </Text>
        ) : null}
        <Text style={{ typography: "bodyLarge" }} color={md.onSurface}>
          {view.lede}
        </Text>
      </Block>
      {view.empty ? null : (
        <>
          {view.lastNight ? <LastNight md={md} night={view.lastNight} note={view.note} /> : null}
          <WindowPicker days={days} onPick={setDays} />
          <Window md={md} w={days === 7 ? view.week : view.month} />
          {view.insights.length > 0 ? <Insights md={md} items={view.insights} /> : null}
        </>
      )}
      <View style={{ height: 32 }} />
    </>
  );
}

export function SleepScreen() {
  const { data, error, refetch } = useSleep();
  return (
    <LargeAppBarScreen title="Sleep" settings={false}>
      {data ? <SleepBody view={data} /> : null}
      {!data && !error ? <Loading /> : null}
      {error ? <ErrorBlock message={String(error)} onRetry={refetch} /> : null}
    </LargeAppBarScreen>
  );
}
