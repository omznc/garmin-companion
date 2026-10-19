// Links from Health to Weight, Food and Strength. Each row shows the value line of its screen.
// Strength shows only for a gym profile.
import { useRouter } from "expo-router";
import { PlatformColor, Pressable, Text } from "react-native";
import { type BodyKind, useBodyLine, useStrengthShown } from "../data/body";
import { Group, secondary } from "./Grouped";

const ROWS: { kind: BodyKind; label: string }[] = [
  { kind: "weight", label: "Weight" },
  { kind: "food", label: "Food" },
  { kind: "strength", label: "Strength" },
];

function BodyRow({ kind, label, last }: { kind: BodyKind; label: string; last: boolean }) {
  const router = useRouter();
  const { data } = useBodyLine(kind);
  return (
    <Pressable
      accessibilityRole="button"
      onPress={() => router.push(`/health/${kind}`)}
      style={{
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        minHeight: 44,
        paddingHorizontal: 16,
        paddingVertical: 10,
        borderBottomWidth: last ? 0 : 0.5,
        borderBottomColor: PlatformColor("separator"),
      }}
    >
      <Text style={{ fontSize: 17, color: PlatformColor("label") }}>{label}</Text>
      <Text
        numberOfLines={1}
        style={{ flex: 1, fontSize: 17, color: secondary, textAlign: "right" }}
      >
        {data ?? ""}
      </Text>
      <Text style={{ fontSize: 17, color: PlatformColor("tertiaryLabel") }}>{">"}</Text>
    </Pressable>
  );
}

export function BodyLinks() {
  const strength = useStrengthShown().data === true;
  const rows = ROWS.filter((r) => r.kind !== "strength" || strength);
  return (
    <Group>
      {rows.map((r, i) => (
        <BodyRow key={r.kind} kind={r.kind} label={r.label} last={i === rows.length - 1} />
      ))}
    </Group>
  );
}
