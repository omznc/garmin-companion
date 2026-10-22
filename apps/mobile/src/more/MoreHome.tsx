// The More tab: a grouped list. Each row pushes its own page in the More stack.
import { useRouter } from "expo-router";
import { useFitnessShown, useStrengthShown } from "../data/body";
import { THEMES } from "../data/useSettingsActions";
import { updatesLine } from "../update/updatesLine";
import { SELF_UPDATE, useApkUpdate } from "../update/useApkUpdate";
import { useAppearanceLine } from "../theme/useAppearanceLine";
import { useSettings } from "../data/useAccount";
import { useCoachConfig } from "../data/useAsk";
import { Item, type ItemProps, Page, Section } from "./Kit";
import { providerName } from "./pages";

type MdIcon = NonNullable<ItemProps["md"]>["icon"];
type Row = Omit<ItemProps, "md"> & { href: string; mdIcon: MdIcon };

const row = (
  title: string,
  href: string,
  sf: string,
  color: string,
  mdIcon: MdIcon,
  value?: string,
): Row => ({
  title,
  href,
  value,
  icon: { sf, color },
  mdIcon,
  chevron: true,
});

export function MoreHome() {
  const router = useRouter();
  const strength = useStrengthShown().data === true;
  const fitness = useFitnessShown().data === true;
  const settings = useSettings().data;
  const coach = useCoachConfig().data;
  const theme = useAppearanceLine(THEMES.find((t) => t.value === settings?.theme)?.label);
  const update = useApkUpdate();
  const updates = updatesLine(update);
  const health: Row[] = [
    row("Sleep", "/more/sleep", "bed.double.fill", "systemIndigo", "bedtime"),
    row("Weight", "/more/weight", "scalemass.fill", "systemTeal", "monitor-weight"),
    row("Food", "/more/food", "fork.knife", "systemGreen", "restaurant"),
    ...(strength
      ? [row("Strength", "/more/strength", "dumbbell.fill", "systemOrange", "fitness-center")]
      : []),
  ];
  const training: Row[] = [
    row("Ask the coach", "/ask", "bubble.left.fill", "systemPurple", "chat"),
    row("Insights", "/more/insights", "lightbulb.fill", "systemYellow", "lightbulb"),
    ...(fitness ? [row("Fitness", "/more/fitness", "gauge.medium", "systemRed", "speed")] : []),
    row("Weekly report", "/more/reports", "doc.text.fill", "systemBlue", "summarize"),
    row("Plan", "/more/plan", "list.bullet.clipboard.fill", "systemIndigo", "event-note"),
    row("Routes", "/more/routes", "map.fill", "systemGreen", "route"),
    row("Gear", "/more/gear", "bag.fill", "systemBrown", "hiking"),
  ];
  const prefs: Row[] = [
    row(
      "Garmin account",
      "/more/garmin",
      "person.crop.circle.fill",
      "systemBlue",
      "account-circle",
      settings ? (settings.connected ? "Connected" : "Not connected") : undefined,
    ),
    row("Sync", "/more/sync", "arrow.triangle.2.circlepath", "systemGreen", "sync"),
    row("Appearance", "/more/appearance", "paintbrush.fill", "systemPink", "palette", theme),
    row(
      "Coach",
      "/more/coach",
      "sparkles",
      "systemPurple",
      "auto-awesome",
      coach && providerName(coach.provider),
    ),
    ...(SELF_UPDATE
      ? [
          {
            ...row(
              "Updates",
              "updates",
              "arrow.down.circle.fill",
              "systemBlue",
              "system-update",
              updates.value,
            ),
            chevron: false,
            disabled: updates.busy,
            onPress: updates.onPress,
          },
        ]
      : []),
    row("About", "/more/about", "info.circle.fill", "systemGray", "info"),
  ];
  // One tone for each section, so the icon tiles of a group match on Android.
  const draw = (rows: Row[], tone: NonNullable<ItemProps["md"]>["tone"]) =>
    rows.map(({ href, mdIcon, ...item }) => (
      <Item
        key={href}
        onPress={() => router.push(href as never)}
        md={{ icon: mdIcon, tone }}
        {...item}
      />
    ));
  return (
    <Page title="More" large>
      <Section title="Health">{draw(health, "primary")}</Section>
      <Section title="Training">{draw(training, "tertiary")}</Section>
      <Section title="Settings">{draw(prefs, "secondary")}</Section>
    </Page>
  );
}
