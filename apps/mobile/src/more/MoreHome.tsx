// The More tab: a grouped list. Each row pushes its own page in the More stack.
import { useRouter } from "expo-router";
import { useStrengthShown } from "../data/body";
import { THEMES } from "../data/useSettingsActions";
import { updatesLine } from "../update/updatesLine";
import { SELF_UPDATE, useApkUpdate } from "../update/useApkUpdate";
import { useAppearanceLine } from "../theme/useAppearanceLine";
import { useSettings } from "../data/useAccount";
import { useCoachConfig } from "../data/useAsk";
import { Item, type ItemProps, Page, Section } from "./Kit";
import { providerName } from "./pages";

type Row = ItemProps & { href: string };

const row = (
  title: string,
  href: string,
  sf: string,
  color: string,
  md: ItemProps["md"],
  value?: string,
): Row => ({
  title,
  href,
  value,
  icon: { sf, color },
  md,
  chevron: true,
});

export function MoreHome() {
  const router = useRouter();
  const strength = useStrengthShown().data === true;
  const settings = useSettings().data;
  const coach = useCoachConfig().data;
  const theme = useAppearanceLine(THEMES.find((t) => t.value === settings?.theme)?.label);
  const update = useApkUpdate();
  const updates = updatesLine(update);
  const health: Row[] = [
    row("Sleep", "/more/sleep", "bed.double.fill", "systemIndigo", {
      icon: "bedtime",
      tone: "primary",
    }),
    row("Weight", "/more/weight", "scalemass.fill", "systemTeal", {
      icon: "monitor-weight",
      tone: "tertiary",
    }),
    row("Food", "/more/food", "fork.knife", "systemGreen", {
      icon: "restaurant",
      tone: "secondary",
    }),
    ...(strength
      ? [
          row("Strength", "/more/strength", "dumbbell.fill", "systemOrange", {
            icon: "fitness-center",
            tone: "primary",
          }),
        ]
      : []),
  ];
  const prefs: Row[] = [
    row(
      "Garmin account",
      "/more/garmin",
      "person.crop.circle.fill",
      "systemBlue",
      { icon: "account-circle", tone: "primary" },
      settings ? (settings.connected ? "Connected" : "Not connected") : undefined,
    ),
    row("Sync", "/more/sync", "arrow.triangle.2.circlepath", "systemGreen", {
      icon: "sync",
      tone: "secondary",
    }),
    row(
      "Appearance",
      "/more/appearance",
      "paintbrush.fill",
      "systemPink",
      { icon: "palette", tone: "tertiary" },
      theme,
    ),
    row(
      "Coach",
      "/more/coach",
      "sparkles",
      "systemPurple",
      { icon: "auto-awesome", tone: "primary" },
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
              {
                icon: "system-update",
                tone: "secondary",
              },
              updates.value,
            ),
            chevron: false,
            disabled: updates.busy,
            onPress: updates.onPress,
          },
        ]
      : []),
    row("About", "/more/about", "info.circle.fill", "systemGray", {
      icon: "info",
      tone: "secondary",
    }),
  ];
  const draw = (rows: Row[]) =>
    rows.map(({ href, ...item }) => (
      <Item key={href} onPress={() => router.push(href as never)} {...item} />
    ));
  return (
    <Page title="More" large>
      <Section title="Health">{draw(health)}</Section>
      <Section title="Settings">{draw(prefs)}</Section>
    </Page>
  );
}
