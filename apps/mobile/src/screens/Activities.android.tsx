// Activities on Android: one grouped card for each day, a tonal icon for each sport family.
// A row opens the detail route in the same stack.
import { useRouter } from "expo-router";
import { View } from "react-native";
import { FocusAction } from "../components/FocusChip";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import { Card, GAP, Group, ListRow, Loading, MARGIN, SectionLabel, T } from "../components/MdKit";
import { FAMILY_ICON } from "../components/TodayKit";
import { type ActivitiesView, type ActivityCard, useActivities } from "../data/activities";
import { emptyActivitiesText, useFocus } from "../data/focus";
import { shareActivitiesCsv } from "../data/share";
import { useMd } from "../theme/md";

function SessionRow({ c }: { c: ActivityCard }) {
  const router = useRouter();
  const md = useMd();
  const sub = [c.sportLabel, c.time].filter(Boolean).join(" - ");
  return (
    <ListRow
      icon={FAMILY_ICON[c.family] ?? "bolt"}
      tone="primary"
      title={c.name}
      supporting={[sub, c.detail].filter(Boolean).join("\n")}
      onPress={() => router.push(`/activities/${c.activityId}`)}
      trailing={
        <View style={{ maxWidth: 120 }}>
          <T v="headlineSmall" color={md.primary} align="right" lines={1}>
            {c.headline}
          </T>
        </View>
      }
    />
  );
}

type Day = ActivitiesView["groups"][number];

// The padding of Content, spread over the list parts: it has no one wrapper any more.
function DayGroup({ g, first, last }: { g: Day; first: boolean; last: boolean }) {
  return (
    <View
      style={{
        gap: 8,
        paddingHorizontal: MARGIN,
        paddingTop: first ? MARGIN : GAP,
        paddingBottom: last ? MARGIN : 0,
      }}
    >
      <SectionLabel>{g.label}</SectionLabel>
      <Group>
        {g.sessions.map((c) => (
          <SessionRow key={c.activityId} c={c} />
        ))}
      </Group>
    </View>
  );
}

// Loading, the error and the empty note. They sit above the days.
function Head({ data, error }: { data: ActivitiesView | undefined; error: unknown }) {
  const md = useMd();
  const focus = useFocus();
  return (
    <View style={{ padding: MARGIN, gap: GAP }}>
      {!data && !error ? <Loading /> : null}
      {error ? (
        <Card tone="error">
          <T color={md.onErrorContainer}>{`The list could not be read. ${String(error)}`}</T>
        </Card>
      ) : null}
      {data?.empty ? (
        <Card tone="secondary">
          <T v="body" color={md.onSecondaryContainer}>
            {emptyActivitiesText(focus)}
          </T>
        </Card>
      ) : null}
    </View>
  );
}

// One list item for each day, so a long history draws only the days near the screen.
export function ActivitiesScreen() {
  const { data, error } = useActivities();
  const groups = data?.groups ?? [];
  const showHead = !data || Boolean(error) || data.empty;
  return (
    <LargeAppBarScreen
      title="Activities"
      actions={<FocusAction />}
      share={{ label: "Export as CSV", onPress: () => void shareActivitiesCsv() }}
      list={{
        data: groups,
        keyExtractor: (g) => g.date,
        renderItem: (g) => (
          <DayGroup
            g={g}
            first={g === groups[0] && !showHead}
            last={g === groups[groups.length - 1]}
          />
        ),
      }}
    >
      {showHead ? <Head data={data} error={error} /> : null}
    </LargeAppBarScreen>
  );
}
