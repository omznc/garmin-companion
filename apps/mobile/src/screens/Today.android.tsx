// Today on Android: Material 3 Expressive cards in the colours of the scheme.
// The core sends the sections in order. A section that has no data is not drawn.
import MaterialIcons from "@expo/vector-icons/MaterialIcons";
import { useRouter } from "expo-router";
import { Pressable, View } from "react-native";
import { AskFab, FAB_CLEARANCE, useFabExtended } from "../components/AskFab";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import {
  BarLabels,
  Bars,
  Card,
  Content,
  ErrorCard,
  IconTile,
  Loading,
  SectionLabel,
  SplitBar,
  T,
} from "../components/MdKit";
import { Cookie, FitText, sportIcon, VitalGrid, zoneTones } from "../components/TodayKit";
import { metricRoute } from "../data/present";
import { useGreeting } from "../data/useGreeting";
import type { Hero, LastSession, Section as TodaySection, Today } from "../data/types";
import { useSync } from "../data/useSync";
import { useToday } from "../data/useToday";
import { useMd } from "../theme/md";

// A fixed grid: equal columns, so a long value cannot push the next one away.
function StatGrid({ stats }: { stats: LastSession["stats"] }) {
  const md = useMd();
  const width = stats.length <= 4 ? "50%" : "33.33%";
  return (
    <View style={{ flexDirection: "row", flexWrap: "wrap", rowGap: 12 }}>
      {stats.map((stat) => (
        <View key={stat.label} style={{ width, paddingRight: 8 }}>
          <FitText size={24} color={md.onSurface}>
            {stat.value}
          </FitText>
          <T v="labelSmall" color={md.onSurfaceVariant}>
            {stat.label}
          </T>
        </View>
      ))}
    </View>
  );
}

function SessionCard({ s }: { s: LastSession }) {
  const md = useMd();
  const tones = zoneTones(md);
  const router = useRouter();
  const sub = [s.sport, s.when, s.zones ? `${s.zones.aboveZ2Pct}% above Z2` : null];
  return (
    <Card gap={12} onPress={() => router.push(`/today/activity/${s.id}`)}>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 14 }}>
        <IconTile name={sportIcon(s.typeKey)} tone="primary" size={56} />
        <View style={{ flex: 1 }}>
          <T v="title" lines={2}>
            {s.name}
          </T>
          <T v="bodyMedium" color={md.onSurfaceVariant}>
            {sub.filter(Boolean).join(" - ")}
          </T>
        </View>
      </View>
      {s.zones ? (
        <SplitBar
          parts={s.zones.percents.map((p, i) => ({ weight: p, color: tones[i] ?? md.primary }))}
        />
      ) : null}
      <StatGrid stats={s.stats} />
    </Card>
  );
}

function WeekCard({ s }: { s: Extract<TodaySection, { kind: "week" }> }) {
  const md = useMd();
  const router = useRouter();
  const any = s.minutes.some((m) => m > 0);
  return (
    <Card gap={12} onPress={() => router.push("/activities")}>
      <T v="titleMedium">Last seven days</T>
      {any ? (
        <>
          <Bars
            values={s.minutes}
            gap={8}
            color={md.secondary}
            colorFor={(_, i) => (i === s.minutes.length - 1 ? md.primary : md.secondary)}
            describe={(v, i) => `${s.labels[i]}: ${Math.round(v ?? 0)} min`}
          />
          <BarLabels labels={s.labels} />
        </>
      ) : null}
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {s.summary}
      </T>
    </Card>
  );
}

function DriftCard({ s }: { s: Extract<TodaySection, { kind: "drift" }> }) {
  const md = useMd();
  const router = useRouter();
  const labels = s.runs.map((r) => r.label);
  return (
    <Card gap={12} onPress={() => router.push("/activities")}>
      <T v="titleMedium">Zone drift</T>
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {s.summary}
      </T>
      <Bars
        values={s.runs.map((r) => r.aboveZ2Pct)}
        max={100}
        gap={8}
        describe={(v, i) => `${labels[i]}: ${Math.round(v ?? 0)} %`}
      />
      <BarLabels labels={labels} />
    </Card>
  );
}

function SectionView({ s, hero }: { s: TodaySection; hero: Hero | null }) {
  const md = useMd();
  switch (s.kind) {
    case "health":
      return <VitalGrid metrics={s.metrics.filter((m) => m.id !== hero?.id)} />;
    case "attention":
      return (
        <>
          <SectionLabel>Attention</SectionLabel>
          <Card tone="tertiary" gap={16}>
            {s.notes.map((n) => (
              <View
                key={n.text}
                style={{ flexDirection: "row", gap: 12, alignItems: "flex-start" }}
              >
                <MaterialIcons name="priority-high" size={22} color={md.onTertiaryContainer} />
                <View style={{ flex: 1 }}>
                  <T v={n.accent ? "titleMedium" : "body"} color={md.onTertiaryContainer}>
                    {n.text}
                  </T>
                </View>
              </View>
            ))}
          </Card>
        </>
      );
    case "text":
      return (
        <View style={{ paddingHorizontal: 4 }}>
          <T v="body" color={md.onSurfaceVariant}>
            {s.text}
          </T>
        </View>
      );
    case "session":
      return (
        <>
          <SectionLabel>Last session</SectionLabel>
          <SessionCard s={s.session} />
        </>
      );
    case "week":
      return <WeekCard s={s} />;
    case "drift":
      return <DriftCard s={s} />;
  }
}

function HeroCard({ today, busy, onSync }: { today: Today; busy: boolean; onSync: () => void }) {
  const md = useMd();
  const router = useRouter();
  const hero = today.hero;
  return (
    <Card
      tone="primary"
      gap={16}
      style={{ borderRadius: 40 }}
      onPress={hero ? () => router.push(metricRoute(hero.id)) : undefined}
    >
      <T v="label" color={md.onPrimaryContainer}>
        {today.dateLabel}
      </T>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 20 }}>
        <Cookie size={144} color={md.primary}>
          {hero ? (
            <T v="display" color={md.onPrimary}>
              {hero.text}
            </T>
          ) : (
            <MaterialIcons name="watch" size={56} color={md.onPrimary} />
          )}
        </Cookie>
        <View style={{ flex: 1, gap: 4 }}>
          <T v="headlineSmall" color={md.onPrimaryContainer}>
            {hero?.label ?? "Welcome"}
          </T>
          {hero?.pct != null ? (
            <T v="bodyMedium" color={md.onPrimaryContainer}>
              out of 100
            </T>
          ) : null}
        </View>
      </View>
      <T v="body" color={md.onPrimaryContainer}>
        {today.lede}
      </T>
      {hero ? null : <SyncButton busy={busy} onPress={onSync} />}
    </Card>
  );
}

// The welcome card has no data to show, so it offers the first sync.
function SyncButton({ busy, onPress }: { busy: boolean; onPress: () => void }) {
  const md = useMd();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ disabled: busy, busy }}
      disabled={busy}
      onPress={onPress}
      android_ripple={{ color: `${md.onPrimary}33` }}
      style={{
        alignSelf: "flex-start",
        minHeight: 48,
        borderRadius: 24,
        paddingHorizontal: 24,
        justifyContent: "center",
        backgroundColor: md.primary,
        opacity: busy ? 0.6 : 1,
      }}
    >
      <T v="label" color={md.onPrimary}>
        {busy ? "Syncing" : "Sync now"}
      </T>
    </Pressable>
  );
}

// A row near the top that opens the coach. The FAB does the same while the list scrolls.
function AskCard() {
  const router = useRouter();
  const md = useMd();
  return (
    <Card gap={0} onPress={() => router.push("/ask")}>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 14 }}>
        <IconTile name="chat" tone="secondary" />
        <View style={{ flex: 1 }}>
          <T v="titleMedium">Ask the coach</T>
          <T v="bodyMedium" color={md.onSurfaceVariant}>
            Questions about your sleep, training and recovery
          </T>
        </View>
        <MaterialIcons name="chevron-right" size={24} color={md.onSurfaceVariant} />
      </View>
    </Card>
  );
}

function TodayBody({ today, busy, onSync }: { today: Today; busy: boolean; onSync: () => void }) {
  return (
    <Content>
      <HeroCard today={today} busy={busy} onSync={onSync} />
      <AskCard />
      {today.sections.map((s) => (
        <SectionView key={s.kind === "text" ? s.key : s.kind} s={s} hero={today.hero} />
      ))}
    </Content>
  );
}

function SyncLine({ line, error }: { line: string | null; error: string | null }) {
  const md = useMd();
  if (!line && !error) return null;
  return (
    <View style={{ paddingHorizontal: 20 }}>
      <T v="labelSmall" color={error ? md.error : md.onSurfaceVariant}>
        {error ?? line}
      </T>
    </View>
  );
}

export function TodayScreen() {
  const { data, error, refetch } = useToday();
  const sync = useSync();
  const greeting = useGreeting();
  const router = useRouter();
  const fab = useFabExtended();
  return (
    <View style={{ flex: 1 }}>
      <LargeAppBarScreen
        title={data ? greeting : "Today"}
        refreshing={sync.busy}
        onRefresh={() => sync.refresh()}
        onScrollOffset={fab.onScroll}
        bottomSpace={FAB_CLEARANCE}
      >
        <SyncLine line={sync.line} error={sync.error} />
        {data ? <TodayBody today={data} busy={sync.busy} onSync={() => sync.refresh()} /> : null}
        {!data && !error ? <Loading /> : null}
        {error ? <ErrorCard message={String(error)} onRetry={refetch} /> : null}
      </LargeAppBarScreen>
      <AskFab extended={fab.extended} onPress={() => router.push("/ask")} />
    </View>
  );
}
