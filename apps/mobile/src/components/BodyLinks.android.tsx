// Links from Health to Weight, Food and Strength, as one grouped card. Each row shows
// the first sentence of its screen. Strength shows only for a gym profile.
import { useRouter } from "expo-router";
import { type BodyKind, type BodyScreenData, useBody, useStrengthShown } from "../data/body";
import { Group, type IconName, ListRow, type Tone } from "./MdKit";

/** The sentence that opens the screen: a lede, a note, or the empty-state text. */
export function bodySummary(data: BodyScreenData | undefined): string | undefined {
  for (const b of data?.blocks ?? []) {
    if (b.kind === "text" && b.tone !== "heading") return b.text;
    if (b.kind === "empty") return b.body;
  }
  return undefined;
}

type BodyRowInfo = { kind: BodyKind; label: string; icon: IconName; tone: Tone };

export const BODY_ROWS: BodyRowInfo[] = [
  { kind: "weight", label: "Weight", icon: "monitor-weight", tone: "tertiary" },
  { kind: "food", label: "Food", icon: "restaurant", tone: "secondary" },
  { kind: "strength", label: "Strength", icon: "fitness-center", tone: "primary" },
];

export function BodyRow({ info, base }: { info: BodyRowInfo; base: string }) {
  const router = useRouter();
  const { data } = useBody(info.kind);
  return (
    <ListRow
      icon={info.icon}
      tone={info.tone}
      title={info.label}
      supporting={bodySummary(data)}
      chevron
      onPress={() => router.push(`${base}/${info.kind}` as never)}
    />
  );
}

export function BodyLinks() {
  const strength = useStrengthShown().data === true;
  return (
    <Group>
      {BODY_ROWS.filter((r) => r.kind !== "strength" || strength).map((r) => (
        <BodyRow key={r.kind} info={r} base="/health" />
      ))}
    </Group>
  );
}
