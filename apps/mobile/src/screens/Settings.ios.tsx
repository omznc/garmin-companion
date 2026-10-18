// Settings on iOS: grouped sections and system colours. The theme is a list with a check mark.
import type { ReactNode } from "react";
import { PlatformColor, Pressable, ScrollView, Text, View } from "react-native";
import type { SettingsView } from "../data/types";
import { THEMES, useSettingsActions } from "../data/useSettingsActions";

type Actions = ReturnType<typeof useSettingsActions>;

const label = PlatformColor("label");
const secondary = PlatformColor("secondaryLabel");
const separator = PlatformColor("separator");
const tint = PlatformColor("systemOrange");

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <View style={{ marginTop: 24 }}>
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
        {title}
      </Text>
      <View
        style={{
          backgroundColor: PlatformColor("secondarySystemGroupedBackground"),
          borderRadius: 26,
          marginHorizontal: 16,
          overflow: "hidden",
        }}
      >
        {children}
      </View>
    </View>
  );
}

function Row({
  text,
  onPress,
  color = label,
  checked,
  disabled,
}: {
  text: string;
  onPress?: () => void;
  color?: ReturnType<typeof PlatformColor>;
  checked?: boolean;
  disabled?: boolean;
}) {
  return (
    <Pressable
      accessibilityRole={onPress ? "button" : "text"}
      disabled={disabled || !onPress}
      onPress={onPress}
      style={{
        flexDirection: "row",
        justifyContent: "space-between",
        alignItems: "center",
        minHeight: 44,
        paddingHorizontal: 16,
        paddingVertical: 10,
        opacity: disabled ? 0.4 : 1,
        borderBottomWidth: 0.5,
        borderBottomColor: separator,
      }}
    >
      <Text style={{ fontSize: 17, color, flexShrink: 1 }}>{text}</Text>
      {checked ? <Text style={{ fontSize: 17, color: tint }}>✓</Text> : null}
    </Pressable>
  );
}

function GarminGroup({ s, a }: { s: SettingsView; a: Actions }) {
  return (
    <Group title="Garmin">
      <Row text={s.accountLine} />
      <Row text={s.cacheLine} />
      {a.sync.line ? <Row text={a.sync.line} /> : null}
      {a.sync.error ? <Row text={a.sync.error} color={PlatformColor("systemRed")} /> : null}
      <Row text="Sync recent" color={tint} disabled={a.sync.busy} onPress={() => a.runSync({})} />
      <Row
        text="Full re-sync"
        color={tint}
        disabled={a.sync.busy}
        onPress={() => a.runSync({ full: true, days: 365 })}
      />
      {s.connected ? (
        <Row text="Disconnect" color={PlatformColor("systemRed")} onPress={a.disconnect} />
      ) : null}
    </Group>
  );
}

export function SettingsScreen() {
  const a = useSettingsActions();
  const s = a.settings.data;
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      {s ? (
        <>
          <GarminGroup s={s} a={a} />
          <Group title="Appearance">
            {THEMES.map((t) => (
              <Row
                key={t.value}
                text={t.label}
                checked={s.theme === t.value}
                onPress={() => a.chooseTheme(t.value)}
              />
            ))}
          </Group>
          <Group title="What leaves this device">
            <Row text={s.privacyLine} />
          </Group>
          <Group title="About">
            <Row text={`Version ${s.version}`} />
            {s.dataFolder ? <Row text={s.dataFolder} /> : null}
          </Group>
        </>
      ) : null}
      {a.settings.error ? (
        <Group title="Settings">
          <Row text={String(a.settings.error)} />
        </Group>
      ) : null}
    </ScrollView>
  );
}
