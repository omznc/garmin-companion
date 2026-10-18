// Activities on iOS: native large title, grouped lists by day, system colours, Dynamic Type.
// A row pushes the detail route onto the stack.
import { useRouter } from "expo-router";
import { ActivityIndicator, PlatformColor, Pressable, ScrollView, Text, View } from "react-native";
import { type ActivityCard, useActivities } from "../data/activities";

const label = PlatformColor("label");
const secondary = PlatformColor("secondaryLabel");
const cell = PlatformColor("secondarySystemGroupedBackground");
const separator = PlatformColor("separator");

function SessionRow({ c, last }: { c: ActivityCard; last: boolean }) {
  const router = useRouter();
  const sub = [c.sportLabel, c.time].filter(Boolean).join(", ");
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${c.name}, ${sub}, ${c.headline}`}
      onPress={() => router.push(`/activities/${c.activityId}`)}
      style={({ pressed }) => ({
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        minHeight: 60,
        paddingHorizontal: 16,
        paddingVertical: 10,
        borderBottomWidth: last ? 0 : 0.5,
        borderBottomColor: separator,
        opacity: pressed ? 0.6 : 1,
      })}
    >
      <View style={{ flex: 1 }}>
        <Text style={{ fontSize: 17, color: label }}>{c.name}</Text>
        <Text style={{ fontSize: 15, color: secondary }}>{sub}</Text>
        {c.detail ? <Text style={{ fontSize: 13, color: secondary }}>{c.detail}</Text> : null}
      </View>
      <Text style={{ fontSize: 17, color: label }}>{c.headline}</Text>
      <Text style={{ fontSize: 17, color: PlatformColor("tertiaryLabel") }}>{"›"}</Text>
    </Pressable>
  );
}

function Message({ children }: { children: string }) {
  return (
    <View style={{ margin: 16, padding: 16, borderRadius: 26, backgroundColor: cell }}>
      <Text style={{ fontSize: 15, color: secondary }}>{children}</Text>
    </View>
  );
}

export function ActivitiesScreen() {
  const { data, error } = useActivities();
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      {!data && !error ? <ActivityIndicator style={{ margin: 32 }} /> : null}
      {error ? <Message>{`The list could not be read. ${String(error)}`}</Message> : null}
      {data?.empty ? (
        <Message>No sessions yet. When your watch records a session, it shows here.</Message>
      ) : null}
      {data?.groups.map((g) => (
        <View key={g.date} style={{ marginTop: 24 }}>
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
            {g.label}
          </Text>
          <View
            style={{
              backgroundColor: cell,
              borderRadius: 26,
              marginHorizontal: 16,
              overflow: "hidden",
            }}
          >
            {g.sessions.map((c, i) => (
              <SessionRow key={c.activityId} c={c} last={i === g.sessions.length - 1} />
            ))}
          </View>
        </View>
      ))}
    </ScrollView>
  );
}
