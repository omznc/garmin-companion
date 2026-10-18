// Activities on Android: Material 3 cards from @expo/ui, grouped by day.
// A card opens the detail route in the same stack.
import { Host } from "@expo/ui";
import { Card, Column, Row, Text } from "@expo/ui/jetpack-compose";
import { clickable, fillMaxWidth, paddingAll, weight } from "@expo/ui/jetpack-compose/modifiers";
import { useRouter } from "expo-router";
import { ActivityIndicator } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import { type ActivityCard, useActivities } from "../data/activities";
import { HOST_SEED, useMd } from "../theme/md";

type Md = ReturnType<typeof useMd>;

function SessionCard({ md, c }: { md: Md; c: ActivityCard }) {
  const router = useRouter();
  const sub = [c.sportLabel, c.time].filter(Boolean).join(" - ");
  return (
    <Card
      colors={{ containerColor: md.surfaceContainerLow }}
      modifiers={[fillMaxWidth(), clickable(() => router.push(`/activities/${c.activityId}`))]}
    >
      <Row
        horizontalArrangement={{ spacedBy: 16 }}
        verticalAlignment="center"
        modifiers={[fillMaxWidth(), paddingAll(16)]}
      >
        <Column modifiers={[weight(1)]}>
          <Text style={{ typography: "titleMedium" }} color={md.onSurface}>
            {c.name}
          </Text>
          <Text style={{ typography: "bodyMedium" }} color={md.onSurfaceVariant}>
            {sub}
          </Text>
          {c.detail ? (
            <Text style={{ typography: "bodySmall" }} color={md.onSurfaceVariant}>
              {c.detail}
            </Text>
          ) : null}
        </Column>
        <Text style={{ typography: "titleLarge" }} color={md.onSurface}>
          {c.headline}
        </Text>
      </Row>
    </Card>
  );
}

function Message({ md, children }: { md: Md; children: string }) {
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column modifiers={[fillMaxWidth(), paddingAll(16)]}>
        <Text style={{ typography: "bodyLarge" }} color={md.onSurfaceVariant}>
          {children}
        </Text>
      </Column>
    </Host>
  );
}

export function ActivitiesScreen() {
  const md = useMd();
  const { data, error } = useActivities();
  return (
    <LargeAppBarScreen title="Activities">
      {!data && !error ? <ActivityIndicator style={{ margin: 32 }} /> : null}
      {error ? <Message md={md}>{`The list could not be read. ${String(error)}`}</Message> : null}
      {data?.empty ? (
        <Message md={md}>
          No sessions yet. When your watch records a session, it shows here.
        </Message>
      ) : null}
      {data?.groups.map((g) => (
        <Host
          key={g.date}
          matchContents={{ vertical: true }}
          seedColor={HOST_SEED}
          style={{ width: "100%" }}
        >
          <Column
            verticalArrangement={{ spacedBy: 8 }}
            modifiers={[fillMaxWidth(), paddingAll(16)]}
          >
            <Text style={{ typography: "titleSmall" }} color={md.onSurfaceVariant}>
              {g.label}
            </Text>
            {g.sessions.map((c) => (
              <SessionCard key={c.activityId} md={md} c={c} />
            ))}
          </Column>
        </Host>
      ))}
    </LargeAppBarScreen>
  );
}
