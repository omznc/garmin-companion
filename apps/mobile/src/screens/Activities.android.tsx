// Activities on Android: one grouped card for each day, a tonal icon for each sport family.
// A row opens the detail route in the same stack.
import { useRouter } from "expo-router";
import { View } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import {
  Card,
  Content,
  Group,
  type IconName,
  ListRow,
  Loading,
  SectionLabel,
  T,
} from "../components/MdKit";
import { type ActivityCard, useActivities } from "../data/activities";
import { useMd } from "../theme/md";

const FAMILY_ICON: Record<string, IconName> = {
  endurance: "directions-run",
  gym: "fitness-center",
  golf: "golf-course",
  water: "pool",
  mindbody: "self-improvement",
  mind_body: "self-improvement",
};

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

export function ActivitiesScreen() {
  const md = useMd();
  const { data, error } = useActivities();
  return (
    <LargeAppBarScreen title="Activities">
      <Content>
        {!data && !error ? <Loading /> : null}
        {error ? (
          <Card tone="error">
            <T color={md.onErrorContainer}>{`The list could not be read. ${String(error)}`}</T>
          </Card>
        ) : null}
        {data?.empty ? (
          <Card tone="secondary">
            <T v="body" color={md.onSecondaryContainer}>
              No sessions yet. When your watch records a session, it shows here.
            </T>
          </Card>
        ) : null}
        {data?.groups.map((g) => (
          <View key={g.date} style={{ gap: 8 }}>
            <SectionLabel>{g.label}</SectionLabel>
            <Group>
              {g.sessions.map((c) => (
                <SessionRow key={c.activityId} c={c} />
              ))}
            </Group>
          </View>
        ))}
      </Content>
    </LargeAppBarScreen>
  );
}
