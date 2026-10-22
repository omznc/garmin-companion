// Tabs on Android: move a tab up or down, choose the start tab, reset. A drag handle would
// need a gesture library, so each row has two buttons. More stays last.
import MaterialIcons from "@expo/vector-icons/MaterialIcons";
import { Pressable } from "react-native";
import { type IconName, ListRow } from "../components/MdKit";
import { toggled } from "../ui/haptics";
import { useTabsEditor } from "../nav/useTabsEditor";
import { useMd } from "../theme/md";
import { Item, Note, Page, Section } from "./Kit";

const ICONS: Record<string, IconName> = {
  today: "wb-sunny",
  activities: "directions-run",
  health: "favorite",
  sleep: "bedtime",
  more: "more-horiz",
};

function MoveButton({
  icon,
  label,
  disabled,
  onPress,
}: {
  icon: IconName;
  label: string;
  disabled: boolean;
  onPress: () => void;
}) {
  const md = useMd();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={() => {
        toggled();
        onPress();
      }}
      style={{ width: 48, height: 48, alignItems: "center", justifyContent: "center" }}
    >
      <MaterialIcons
        name={icon}
        size={24}
        color={disabled ? md.outlineVariant : md.onSurfaceVariant}
      />
    </Pressable>
  );
}

export function TabsPage() {
  const t = useTabsEditor();
  const last = t.movable.length - 1;
  return (
    <Page title="Tabs">
      <Section title="Order">
        {t.movable.map((tab, i) => (
          <ListRow
            key={tab.name}
            icon={ICONS[tab.name]}
            title={tab.title}
            trailing={
              <>
                <MoveButton
                  icon="arrow-upward"
                  label={`Move ${tab.title} up`}
                  disabled={i === 0}
                  onPress={() => t.move(tab.name, -1)}
                />
                <MoveButton
                  icon="arrow-downward"
                  label={`Move ${tab.title} down`}
                  disabled={i === last}
                  onPress={() => t.move(tab.name, 1)}
                />
              </>
            }
          />
        ))}
        <ListRow icon={ICONS.more} title="More" supporting="Always last" />
      </Section>
      <Section title="Start tab">
        {t.all.map((tab) => (
          <Item
            key={tab.name}
            title={tab.title}
            checked={t.start.name === tab.name}
            onPress={() => t.chooseStart(tab.name)}
          />
        ))}
      </Section>
      <Note>The app opens on the start tab.</Note>
      <Section>
        <Item title="Reset to default" tone="action" disabled={t.shipped} onPress={t.reset} />
      </Section>
    </Page>
  );
}
