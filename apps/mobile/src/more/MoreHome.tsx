// The More tab: a grouped list. Each row pushes its own page in the More stack.
import { useRouter } from "expo-router";
import { useStrengthShown } from "../data/body";
import { THEMES } from "../data/useSettingsActions";
import { useSettings } from "../data/useAccount";
import { useCoachConfig } from "../data/useAsk";
import { Item, type ItemProps, Page, Section } from "./Kit";
import { providerName } from "./pages";

type Row = ItemProps & { href: string };

const row = (title: string, href: string, sf: string, color: string, value?: string): Row => ({
  title,
  href,
  value,
  icon: { sf, color },
  chevron: true,
});

export function MoreHome() {
  const router = useRouter();
  const strength = useStrengthShown().data === true;
  const settings = useSettings().data;
  const coach = useCoachConfig().data;
  const theme = THEMES.find((t) => t.value === settings?.theme)?.label;
  const health: Row[] = [
    row("Sleep", "/more/sleep", "bed.double.fill", "systemIndigo"),
    row("Weight", "/more/weight", "scalemass.fill", "systemTeal"),
    row("Food", "/more/food", "fork.knife", "systemGreen"),
    ...(strength ? [row("Strength", "/more/strength", "dumbbell.fill", "systemOrange")] : []),
  ];
  const prefs: Row[] = [
    row(
      "Garmin account",
      "/more/garmin",
      "person.crop.circle.fill",
      "systemBlue",
      settings ? (settings.connected ? "Connected" : "Not connected") : undefined,
    ),
    row("Sync", "/more/sync", "arrow.triangle.2.circlepath", "systemGreen"),
    row("Appearance", "/more/appearance", "paintbrush.fill", "systemPink", theme),
    row("Coach", "/more/coach", "sparkles", "systemPurple", coach && providerName(coach.provider)),
    row("About", "/more/about", "info.circle.fill", "systemGray"),
  ];
  const draw = (rows: Row[]) =>
    rows.map(({ href, ...item }) => (
      <Item key={href} {...item} onPress={() => router.push(href as never)} />
    ));
  return (
    <Page title="More" large>
      <Section title="Health">{draw(health)}</Section>
      <Section title="Settings">{draw(prefs)}</Section>
    </Page>
  );
}
