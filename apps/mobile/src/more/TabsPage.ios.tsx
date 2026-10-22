// Tabs on iOS: move a tab up or down, choose the start tab, reset. More stays last.
// The tab bar is native and cannot reorder while the app runs, so a new order shows at the next start.
import { SymbolView } from "expo-symbols";
import { PlatformColor, Pressable, Text, View } from "react-native";
import { toggled } from "../ui/haptics";
import { useTabsEditor } from "../nav/useTabsEditor";
import { Item, Note, Page, Section } from "./Kit";

function MoveButton({
  symbol,
  label,
  disabled,
  onPress,
}: {
  symbol: "chevron.up" | "chevron.down";
  label: string;
  disabled: boolean;
  onPress: () => void;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled }}
      disabled={disabled}
      hitSlop={4}
      onPress={() => {
        toggled();
        onPress();
      }}
      style={{ width: 44, height: 44, alignItems: "center", justifyContent: "center" }}
    >
      <SymbolView
        name={symbol}
        size={17}
        tintColor={PlatformColor(disabled ? "tertiaryLabel" : "systemOrange")}
      />
    </Pressable>
  );
}

function MoveRow({
  title,
  first,
  last,
  onMove,
}: {
  title: string;
  first: boolean;
  last: boolean;
  onMove: (by: -1 | 1) => void;
}) {
  return (
    <View style={{ flexDirection: "row", alignItems: "center", paddingLeft: 16, minHeight: 44 }}>
      <Text style={{ fontSize: 17, color: PlatformColor("label"), flexGrow: 1 }}>{title}</Text>
      <MoveButton
        symbol="chevron.up"
        label={`Move ${title} up`}
        disabled={first}
        onPress={() => onMove(-1)}
      />
      <MoveButton
        symbol="chevron.down"
        label={`Move ${title} down`}
        disabled={last}
        onPress={() => onMove(1)}
      />
    </View>
  );
}

export function TabsPage() {
  const t = useTabsEditor();
  const last = t.movable.length - 1;
  return (
    <Page title="Tabs">
      <Section title="Order">
        {t.movable.map((tab, i) => (
          <MoveRow
            key={tab.name}
            title={tab.title}
            first={i === 0}
            last={i === last}
            onMove={(by) => t.move(tab.name, by)}
          />
        ))}
        <Item title="More" value="Always last" />
      </Section>
      <Note>The new order shows when you open the app again.</Note>
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
      <Section>
        <Item title="Reset to default" tone="action" disabled={t.shipped} onPress={t.reset} />
      </Section>
    </Page>
  );
}
