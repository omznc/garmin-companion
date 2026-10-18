// Appearance on Android: the scheme (System, Light, Dark) and the theme colour.
// "Material You" takes the colours of the wallpaper (Android 12 and later). The other
// swatches are seed colours. Compose builds a full Material 3 scheme from each seed.
import MaterialIcons from "@expo/vector-icons/MaterialIcons";
import { Pressable, View } from "react-native";
import { SectionLabel, T } from "../components/MdKit";
import { THEMES as SCHEMES, useSettingsActions } from "../data/useSettingsActions";
import { isMaterialYouAvailable, useMd } from "../theme/md";
import { setThemePref, useThemePrefs } from "../theme/prefs";
import { THEMES } from "../theme/themes";
import { Item, Page, Section } from "./Kit";

const SIZE = 56;

function Swatch({
  label,
  selected,
  onPress,
  children,
}: {
  label: string;
  selected: boolean;
  onPress: () => void;
  children: (ink: string) => React.ReactNode;
}) {
  const md = useMd();
  return (
    <Pressable
      accessibilityRole="radio"
      accessibilityLabel={label}
      accessibilityState={{ checked: selected }}
      onPress={onPress}
      style={{ width: 72, alignItems: "center", gap: 6 }}
    >
      <View
        style={{
          width: SIZE,
          height: SIZE,
          borderRadius: SIZE / 2,
          overflow: "hidden",
          borderWidth: selected ? 3 : 1,
          borderColor: selected ? md.primary : md.outlineVariant,
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {children("#ffffff")}
        {selected ? (
          <View style={{ position: "absolute" }}>
            <MaterialIcons name="check" size={26} color="#ffffff" />
          </View>
        ) : null}
      </View>
      <T v="labelSmall" color={selected ? md.onSurface : md.onSurfaceVariant} align="center">
        {label}
      </T>
    </Pressable>
  );
}

function Swatches() {
  const md = useMd();
  const prefs = useThemePrefs();
  const dynamic = prefs.materialYou && isMaterialYouAvailable;
  return (
    <View
      accessibilityRole="radiogroup"
      style={{
        flexDirection: "row",
        flexWrap: "wrap",
        rowGap: 12,
        padding: 16,
        backgroundColor: md.surfaceContainer,
        borderRadius: 28,
      }}
    >
      {isMaterialYouAvailable ? (
        <Swatch
          label="Material You"
          selected={dynamic}
          onPress={() => setThemePref("materialYou", true)}
        >
          {() => (
            <View style={{ flex: 1, alignSelf: "stretch", flexDirection: "row" }}>
              <View style={{ flex: 1, backgroundColor: md.primary }} />
              <View style={{ flex: 1 }}>
                <View style={{ flex: 1, backgroundColor: md.tertiary }} />
                <View style={{ flex: 1, backgroundColor: md.secondary }} />
              </View>
            </View>
          )}
        </Swatch>
      ) : null}
      {THEMES.map((t) => (
        <Swatch
          key={t.id}
          label={t.label}
          selected={!dynamic && prefs.theme === t.id}
          onPress={() => {
            setThemePref("theme", t.id);
            setThemePref("materialYou", false);
          }}
        >
          {() => <View style={{ alignSelf: "stretch", flex: 1, backgroundColor: t.seed }} />}
        </Swatch>
      ))}
    </View>
  );
}

export function AppearancePage() {
  const a = useSettingsActions();
  const { scheme } = useThemePrefs();
  return (
    <Page title="Appearance">
      <Section title="Mode">
        {SCHEMES.map((s) => (
          <Item
            key={s.value}
            title={s.label}
            checked={scheme === s.value}
            onPress={() => a.chooseTheme(s.value)}
          />
        ))}
      </Section>
      <SectionLabel>Theme colour</SectionLabel>
      <Swatches />
    </Page>
  );
}
