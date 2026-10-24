// Today on iOS: native large title, grouped sections, system colours, Dynamic Type.
// The core sends the sections in order. A section that has no data is not drawn.
import { Stack, useRouter } from "expo-router";
import { Gauge, Host, Text as SwiftText } from "@expo/ui/swift-ui";
import { frame, gaugeStyle, scaleEffect, tint as tintStyle } from "@expo/ui/swift-ui/modifiers";
import { SymbolView } from "expo-symbols";
import { FocusHeader } from "../components/FocusChip";
import type { ReactNode } from "react";
import {
  ActivityIndicator,
  Button,
  PlatformColor,
  Pressable,
  RefreshControl,
  ScrollView,
  Text,
  View,
} from "react-native";
import { metricRoute } from "../data/present";
import type { Hero, Section, Today, WatchSync } from "../data/types";
import { importLabel, importLede, isPulling } from "../data/importLabel";
import { importHistory, type SyncState, useSync } from "../data/useSync";
import { useToday } from "../data/useToday";

const label = PlatformColor("label");
const secondary = PlatformColor("secondaryLabel");
const cell = PlatformColor("secondarySystemGroupedBackground");
const separator = PlatformColor("separator");
const tint = PlatformColor("systemOrange");

function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <View style={{ marginTop: 24 }}>
      {title ? (
        <Text
          accessibilityRole="header"
          style={{
            fontSize: 13,
            color: secondary,
            textTransform: "uppercase",
            marginLeft: 32,
            marginBottom: 6,
          }}
        >
          {title}
        </Text>
      ) : null}
      <View
        style={{
          backgroundColor: cell,
          borderRadius: 26,
          marginHorizontal: 16,
          overflow: "hidden",
        }}
      >
        {children}
      </View>
    </View>
  );
}

// With `onPress` the row is a button with a chevron.
function Row({
  left,
  right,
  accent,
  last,
  onPress,
}: {
  left: string;
  right?: string;
  accent?: boolean;
  last?: boolean;
  onPress?: () => void;
}) {
  const style = {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    gap: 12,
    minHeight: 44,
    paddingHorizontal: 16,
    paddingVertical: 10,
    borderBottomWidth: last ? 0 : 0.5,
    borderBottomColor: separator,
  } as const;
  const content = (
    <>
      <Text style={{ fontSize: 17, color: label, flexGrow: 1, flexShrink: 1 }}>{left}</Text>
      {right ? (
        <Text style={{ fontSize: 17, color: accent ? tint : secondary }}>{right}</Text>
      ) : null}
      {onPress ? (
        <SymbolView name="chevron.right" size={13} tintColor={PlatformColor("tertiaryLabel")} />
      ) : null}
    </>
  );
  // A plain View, not a disabled Pressable: VoiceOver reads a disabled button as "dimmed".
  if (!onPress)
    return (
      <View accessible style={style}>
        {content}
      </View>
    );
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      style={({ pressed }) => ({ ...style, opacity: pressed ? 0.6 : 1 })}
    >
      {content}
    </Pressable>
  );
}

function Note({ children }: { children: string }) {
  return (
    <Group>
      <View style={{ padding: 16 }}>
        <Text style={{ fontSize: 15, color: secondary }}>{children}</Text>
      </View>
    </Group>
  );
}

function SessionGroup({ s }: { s: Extract<Section, { kind: "session" }> }) {
  const router = useRouter();
  const { session } = s;
  const when = [session.when, session.zones ? `${session.zones.aboveZ2Pct}% above Z2` : ""];
  return (
    <Group title={`Last session, ${session.name}`}>
      <Row
        left={session.sport}
        right={when.filter(Boolean).join(", ")}
        onPress={() => router.push(`/today/activity/${session.id}`)}
      />
      {session.stats.map((stat, i) => (
        <Row
          key={stat.label}
          left={stat.label}
          right={stat.value}
          last={i === session.stats.length - 1}
        />
      ))}
    </Group>
  );
}

function WeekGroups({ s }: { s: Extract<Section, { kind: "week" }> }) {
  const router = useRouter();
  return (
    <>
      {s.minutes.some((m) => m > 0) && (
        <Group title="Last seven days">
          {s.labels.map((d, i) => (
            <Row key={d} left={d} right={`${Math.round(s.minutes[i] ?? 0)} min`} />
          ))}
          <Row left="All activities" onPress={() => router.push("/activities")} last />
        </Group>
      )}
      <Note>{s.summary}</Note>
    </>
  );
}

function SectionView({ s, hero }: { s: Section; hero: Hero | null }) {
  const router = useRouter();
  switch (s.kind) {
    case "health": {
      // The hero card shows its metric, so the list skips it.
      const metrics = s.metrics.filter((m) => m.id !== hero?.id);
      if (!metrics.length) return null;
      return (
        <Group title="Health">
          {metrics.map((m, i) => (
            <Row
              key={m.id}
              left={m.label}
              right={m.unit ? `${m.value} ${m.unit}` : m.value}
              accent={m.accent}
              onPress={() => router.push(metricRoute(m.id))}
              last={i === metrics.length - 1}
            />
          ))}
        </Group>
      );
    }
    case "attention":
      return (
        <Group title="Attention">
          {s.notes.map((n, i) => (
            <Row key={n.text} left={n.text} accent={n.accent} last={i === s.notes.length - 1} />
          ))}
        </Group>
      );
    case "text":
      return <Note>{s.text}</Note>;
    case "session":
      return <SessionGroup s={s} />;
    case "week":
      return <WeekGroups s={s} />;
    case "drift":
      return (
        <Group title="Zone drift">
          <Row left={s.summary} onPress={() => router.push("/activities")} />
          {s.runs.map((r, i) => (
            <Row
              // biome-ignore lint/suspicious/noArrayIndexKey: the runs never reorder
              key={i}
              left={r.label}
              right={`${Math.round(r.aboveZ2Pct)}% above Z2`}
              last={i === s.runs.length - 1}
            />
          ))}
        </Group>
      );
  }
}

function AskGroup() {
  const router = useRouter();
  return (
    <Group>
      <Row left="Ask the coach" onPress={() => router.push("/ask")} last />
    </Group>
  );
}

// The headline metric (readiness when Garmin gives one) as a native ring. A tap opens its trend.
function HeroRow({ hero }: { hero: Hero }) {
  const router = useRouter();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${hero.label} ${hero.text}${hero.pct != null ? " out of 100" : ""}`}
      onPress={() => router.push(metricRoute(hero.id))}
      style={({ pressed }) => ({
        flexDirection: "row",
        alignItems: "center",
        gap: 20,
        padding: 16,
        paddingBottom: 0,
        opacity: pressed ? 0.6 : 1,
      })}
    >
      {hero.pct != null ? (
        <Host style={{ width: 88, height: 88 }}>
          <Gauge
            value={hero.pct}
            min={0}
            max={100}
            currentValueLabel={<SwiftText>{hero.text}</SwiftText>}
            modifiers={[
              gaugeStyle("circularCapacity"),
              tintStyle(tint),
              scaleEffect(1.35),
              frame({ width: 88, height: 88 }),
            ]}
          />
        </Host>
      ) : (
        <Text style={{ fontSize: 44, fontWeight: "700", color: tint }}>{hero.text}</Text>
      )}
      <View style={{ flex: 1, gap: 2 }}>
        <Text style={{ fontSize: 22, fontWeight: "700", color: label }}>{hero.label}</Text>
        {hero.pct != null ? (
          <Text style={{ fontSize: 15, color: secondary }}>out of 100</Text>
        ) : null}
      </View>
    </Pressable>
  );
}

function TodayBody({ today, busy, onSync }: { today: Today; busy: boolean; onSync: () => void }) {
  return (
    <>
      <Group>
        {today.hero ? <HeroRow hero={today.hero} /> : null}
        <View style={{ padding: 16 }}>
          <Text style={{ fontSize: 15, color: secondary }}>{today.dateLabel}</Text>
          <Text style={{ fontSize: 17, color: label, marginTop: 6 }}>{today.lede}</Text>
          {today.hero ? null : (
            <Button
              title={busy ? "Syncing" : "Sync now"}
              disabled={busy}
              onPress={onSync}
              color={tint}
            />
          )}
        </View>
      </Group>
      <AskGroup />
      {today.sections.map((s) => (
        <SectionView key={s.kind === "text" ? s.key : s.kind} s={s} hero={today.hero} />
      ))}
    </>
  );
}

// The history import: the first 14 days, then the months. An error offers a retry.
function WatchLine({ watch }: { watch: WatchSync | null }) {
  if (!watch) return null;
  return (
    <Text style={{ fontSize: 13, color: secondary, marginTop: 8, marginHorizontal: 32 }}>
      {watch.hint ? `${watch.line}. ${watch.hint}` : watch.line}
    </Text>
  );
}

function ImportRow({ sync }: { sync: SyncState }) {
  const failed = sync.importError !== null;
  const text = failed ? sync.importError : importLabel(sync);
  if (!text) return null;
  return (
    <Group>
      <View style={{ padding: 16, flexDirection: "row", alignItems: "center", gap: 12 }}>
        {failed ? null : <ActivityIndicator />}
        <Text style={{ flex: 1, fontSize: 15, color: failed ? PlatformColor("systemRed") : label }}>
          {text}
        </Text>
        {failed ? <Button title="Retry" onPress={() => importHistory()} /> : null}
      </View>
    </Group>
  );
}

export function TodayScreen() {
  const router = useRouter();
  const { data, error, refetch } = useToday();
  const sync = useSync();
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      refreshControl={
        <RefreshControl refreshing={isPulling(sync)} onRefresh={() => sync.refresh()} />
      }
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      <FocusHeader>
        <Stack.Toolbar.Button
          icon="bubble.left"
          accessibilityLabel="Ask the coach"
          onPress={() => router.push("/ask")}
        />
      </FocusHeader>
      {sync.importing || sync.importError ? (
        <ImportRow sync={sync} />
      ) : sync.line || sync.error ? (
        <Text style={{ fontSize: 13, color: secondary, marginTop: 8, marginHorizontal: 32 }}>
          {sync.error ?? sync.line}
        </Text>
      ) : null}
      <WatchLine watch={data?.watch ?? null} />
      {data ? (
        <TodayBody
          today={importLede(sync.importing, !!data.hero, data)}
          busy={sync.busy}
          onSync={() => sync.refresh()}
        />
      ) : null}
      {!data && !error ? <ActivityIndicator style={{ margin: 32 }} /> : null}
      {error ? (
        <Group>
          <View style={{ padding: 16 }}>
            <Text style={{ fontSize: 15, color: secondary }}>{String(error)}</Text>
            <Button title="Try again" onPress={() => refetch()} />
          </View>
        </Group>
      ) : null}
    </ScrollView>
  );
}
