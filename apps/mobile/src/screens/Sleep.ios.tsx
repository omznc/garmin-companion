// Sleep on iOS: grouped sections, a native segmented control, system colours.
// It needs no activity. The core sends every figure and sentence.
import { Host, Picker, Text as SwiftText } from "@expo/ui/swift-ui";
import { pickerStyle, tag } from "@expo/ui/swift-ui/modifiers";
import { Stack } from "expo-router";
import { useState } from "react";
import { PlatformColor, ScrollView, Text, View } from "react-native";
import { FocusHeader } from "../components/FocusChip";
import { ErrorGroup, Group, label, Loading, Note, Row, secondary } from "../components/Grouped";
import { MiniBars } from "../components/MiniBars";
import { SleepExtras } from "../components/SleepExtras";
import { presentSleepExtras, useTiles } from "../data/views";
import {
  type NightView,
  type SleepView,
  type SleepWindow,
  shortDay,
  useSleep,
} from "../data/wellness";

/** A night under seven hours is a short night. */
const SHORT_NIGHT_HOURS = 7;
const FULL_BAR_HOURS = 10;

const STAGE_COLOUR = {
  deep: PlatformColor("systemIndigo"),
  rem: PlatformColor("systemPurple"),
  light: PlatformColor("systemTeal"),
  awake: PlatformColor("systemOrange"),
};

function StageBar({ night }: { night: NightView }) {
  return (
    <View style={{ padding: 16 }}>
      <View
        accessible
        accessibilityLabel={night.stages.map((s) => `${s.label} ${s.text}`).join(", ")}
        style={{ flexDirection: "row", height: 12, borderRadius: 6, overflow: "hidden" }}
      >
        {night.stages.map((s) => (
          <View
            key={s.key}
            style={{ flex: Math.max(s.pct, 1), backgroundColor: STAGE_COLOUR[s.key] }}
          />
        ))}
      </View>
    </View>
  );
}

function LastNight({ night, note }: { night: NightView; note: string | null }) {
  const rows: [string, string][] = [
    ["Duration", night.durationText],
    ...(night.score === null
      ? []
      : [
          [
            night.qualifier ? `Score, ${night.qualifier}` : "Score",
            String(Math.round(night.score)),
          ] as [string, string],
        ]),
    ...(night.start && night.end
      ? ([
          ["Asleep", night.start],
          ["Woke", night.end],
        ] as [string, string][])
      : []),
  ];
  const stageRows = night.stages.map((s): [string, string] => [s.label, s.text]);
  return (
    <>
      <Group title={night.title}>
        {rows.map(([left, right], i) => (
          <Row key={left} left={left} right={right} last={i === rows.length - 1} />
        ))}
      </Group>
      {night.stages.length > 0 ? (
        <Group title="Stages">
          <StageBar night={night} />
          {stageRows.map(([left, right], i) => (
            <Row key={left} left={left} right={right} last={i === stageRows.length - 1} />
          ))}
        </Group>
      ) : null}
      {note ? (
        <Group>
          <Note small>{note}</Note>
        </Group>
      ) : null}
    </>
  );
}

function WindowPicker({ days, onPick }: { days: number; onPick: (days: number) => void }) {
  return (
    <View style={{ marginTop: 24, marginHorizontal: 16 }}>
      <Host matchContents={{ vertical: true }}>
        <Picker
          selection={days}
          onSelectionChange={(d: number) => onPick(d)}
          modifiers={[pickerStyle("segmented")]}
        >
          <SwiftText modifiers={[tag(7)]}>7 nights</SwiftText>
          <SwiftText modifiers={[tag(30)]}>30 nights</SwiftText>
        </Picker>
      </Host>
    </View>
  );
}

function Window({ w }: { w: SleepWindow }) {
  const first = w.bars[0];
  const last = w.bars.at(-1);
  return (
    <Group title={`Last ${w.days} nights`}>
      <View style={{ paddingTop: 16 }}>
        <MiniBars
          values={w.bars.map((b) => b.hours)}
          max={FULL_BAR_HOURS}
          height={96}
          gap={w.days <= 7 ? 8 : 3}
          color={PlatformColor("systemIndigo")}
          trackColor={PlatformColor("separator")}
          colorFor={(h) =>
            h < SHORT_NIGHT_HOURS ? PlatformColor("systemOrange") : PlatformColor("systemIndigo")
          }
          describe={(h, i) => {
            const bar = w.bars[i];
            const day = bar ? shortDay(bar.date) : "";
            return h === null ? `${day}: no sleep recorded` : `${day}: ${h.toFixed(1)} hours`;
          }}
        />
        <View
          style={{
            flexDirection: "row",
            justifyContent: "space-between",
            paddingHorizontal: 16,
            paddingTop: 6,
          }}
        >
          {w.days <= 7 ? (
            w.bars.map((b) => (
              <Text key={b.date} style={{ fontSize: 11, color: secondary }}>
                {b.label}
              </Text>
            ))
          ) : (
            <>
              <Text style={{ fontSize: 11, color: secondary }}>{shortDay(first?.date ?? "")}</Text>
              <Text style={{ fontSize: 11, color: secondary }}>{shortDay(last?.date ?? "")}</Text>
            </>
          )}
        </View>
      </View>
      <Note>{w.summary}</Note>
    </Group>
  );
}

function SleepBody({ view }: { view: SleepView }) {
  const [days, setDays] = useState(7);
  const tiles = useTiles().data;
  const extras = tiles ? presentSleepExtras(tiles) : null;
  return (
    <>
      <Group>
        <Note>{view.lede}</Note>
      </Group>
      {view.empty ? null : (
        <>
          {view.lastNight ? <LastNight night={view.lastNight} note={view.note} /> : null}
          <WindowPicker days={days} onPick={setDays} />
          <Window w={days === 7 ? view.week : view.month} />
          {extras ? <SleepExtras extras={extras} /> : null}
          {view.insights.length > 0 ? (
            <Group title="What your nights say">
              {view.insights.map((i, n) => (
                <View
                  key={i.id}
                  style={{
                    padding: 16,
                    borderBottomWidth: n === view.insights.length - 1 ? 0 : 0.5,
                    borderBottomColor: PlatformColor("separator"),
                  }}
                >
                  <Text style={{ fontSize: 17, color: label }}>{i.claim}</Text>
                  <Text style={{ fontSize: 15, color: secondary, marginTop: 4 }}>{i.detail}</Text>
                </View>
              ))}
            </Group>
          ) : null}
        </>
      )}
    </>
  );
}

/** `root` is the Sleep tab. The iOS stack draws the back button itself, so it is unused. */
export function SleepScreen(_props: { root?: boolean }) {
  const { data, error, refetch } = useSleep();
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      <Stack.Screen options={{ title: "Sleep" }} />
      <FocusHeader />
      {data ? <SleepBody view={data} /> : null}
      {!data && !error ? <Loading /> : null}
      {error ? <ErrorGroup message={String(error)} onRetry={() => refetch()} /> : null}
    </ScrollView>
  );
}
