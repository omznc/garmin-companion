// Sleep on Android: last night, the stage bar, a 7 or 30 night window and what the nights say.
// It needs no activity. The core sends every figure and sentence.
import { Host } from "@expo/ui";
import {
  Column,
  SegmentedButton,
  SingleChoiceSegmentedButtonRow,
  Text,
} from "@expo/ui/jetpack-compose";
import { fillMaxWidth } from "@expo/ui/jetpack-compose/modifiers";
import { useState } from "react";
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
import {
  type NightView,
  type SleepInsight,
  type SleepView,
  type SleepWindow,
  shortDay,
  useSleep,
} from "../data/wellness";
import { HOST_SEED, useMd } from "../theme/md";

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

function LastNight({ night, note }: { night: NightView; note: string | null }) {
  const md = useMd();
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
      <TileGrid tiles={tiles} />
      {night.stages.length > 0 ? (
        <Card gap={12}>
          <SplitBar
            parts={night.stages.map((s) => ({ weight: s.pct, color: stageTone(md, s.key) }))}
          />
          <View style={{ flexDirection: "row", flexWrap: "wrap", columnGap: 16, rowGap: 4 }}>
            {night.stages.map((s) => (
              <T key={s.key} v="label" color={stageTone(md, s.key)}>
                {`${s.label} ${s.text}`}
              </T>
            ))}
          </View>
        </Card>
      ) : null}
      {note ? (
        <T v="labelSmall" color={md.onSurfaceVariant}>
          {note}
        </T>
      ) : null}
    </>
  );
}

function WindowPicker({ days, onPick }: { days: number; onPick: (days: number) => void }) {
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column modifiers={[fillMaxWidth()]}>
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

function Window({ w }: { w: SleepWindow }) {
  const md = useMd();
  const labels =
    w.days <= 7
      ? w.bars.map((b) => b.label)
      : [shortDay(w.bars[0]?.date ?? ""), shortDay(w.bars.at(-1)?.date ?? "")];
  return (
    <Card gap={12}>
      <Bars
        values={w.bars.map((b) => b.hours)}
        max={FULL_BAR_HOURS}
        height={112}
        gap={w.days <= 7 ? 8 : 3}
        colorFor={(h) => (h < SHORT_NIGHT_HOURS ? md.tertiary : md.primary)}
        describe={(h, i) => {
          const bar = w.bars[i];
          const day = bar ? shortDay(bar.date) : "";
          return h === null ? `${day}: no sleep recorded` : `${day}: ${h.toFixed(1)} hours`;
        }}
      />
      <BarLabels labels={labels} />
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {w.summary}
      </T>
    </Card>
  );
}

function Insights({ items }: { items: SleepInsight[] }) {
  const md = useMd();
  return (
    <>
      <SectionLabel>What your nights say</SectionLabel>
      {items.map((i) => (
        <Card key={i.id} tone={i.tone === "watch" ? "tertiary" : "surface"} gap={6}>
          <T v="titleMedium" color={i.tone === "watch" ? md.onTertiaryContainer : md.onSurface}>
            {i.claim}
          </T>
          <T
            v="bodyMedium"
            color={i.tone === "watch" ? md.onTertiaryContainer : md.onSurfaceVariant}
          >
            {i.detail}
          </T>
        </Card>
      ))}
    </>
  );
}

function SleepBody({ view }: { view: SleepView }) {
  const md = useMd();
  const [days, setDays] = useState(7);
  return (
    <Content>
      <Card tone="primary" gap={12} style={{ borderRadius: 32 }}>
        {view.lastNight ? (
          <T v="label" color={md.onPrimaryContainer}>
            {view.lastNight.title}
          </T>
        ) : null}
        <T v="body" color={md.onPrimaryContainer}>
          {view.lede}
        </T>
      </Card>
      {view.empty ? null : (
        <>
          {view.lastNight ? <LastNight night={view.lastNight} note={view.note} /> : null}
          <WindowPicker days={days} onPick={setDays} />
          <Window w={days === 7 ? view.week : view.month} />
          {view.insights.length > 0 ? <Insights items={view.insights} /> : null}
        </>
      )}
    </Content>
  );
}

export function SleepScreen() {
  const { data, error, refetch } = useSleep();
  return (
    <LargeAppBarScreen title="Sleep" back>
      {data ? <SleepBody view={data} /> : null}
      {!data && !error ? <Loading /> : null}
      {error ? <ErrorCard message={String(error)} onRetry={refetch} /> : null}
    </LargeAppBarScreen>
  );
}
