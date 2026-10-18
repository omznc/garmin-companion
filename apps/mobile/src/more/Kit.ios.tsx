// The list pieces of More on iOS: inset grouped sections, SF Symbols in tinted squares, chevrons.
import { Stack } from "expo-router";
import { SymbolView } from "expo-symbols";
import { Children, type ReactNode } from "react";
import { type ColorValue, PlatformColor, Pressable, ScrollView, Text, View } from "react-native";

export function useColors() {
  return {
    text: PlatformColor("label"),
    muted: PlatformColor("secondaryLabel"),
    field: PlatformColor("secondarySystemGroupedBackground"),
    accent: PlatformColor("systemOrange"),
    onAccent: PlatformColor("white"),
    warn: PlatformColor("systemRed"),
  };
}

export type ItemProps = {
  title: string;
  value?: string;
  /** An SF Symbol and its tint, for the square in front of the title. */
  icon?: { sf: string; color: string };
  chevron?: boolean;
  checked?: boolean;
  tone?: "action" | "danger";
  disabled?: boolean;
  onPress?: () => void;
};

export function Page({
  title,
  large,
  children,
}: {
  title: string;
  large?: boolean;
  children: ReactNode;
}) {
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      <Stack.Screen options={large ? { title, headerLargeTitle: true } : { title }} />
      {children}
    </ScrollView>
  );
}

export function Section({ title, children }: { title?: string; children: ReactNode }) {
  const rows = Children.toArray(children);
  return (
    <View style={{ marginTop: 24 }}>
      {title ? (
        <Text
          accessibilityRole="header"
          style={{
            fontSize: 13,
            color: PlatformColor("secondaryLabel"),
            textTransform: "uppercase",
            marginLeft: 32,
            marginBottom: 6,
          }}
        >
          {title}
        </Text>
      ) : null}
      <View
        style={{
          backgroundColor: PlatformColor("secondarySystemGroupedBackground"),
          borderRadius: 26,
          marginHorizontal: 16,
          overflow: "hidden",
        }}
      >
        {rows.map((row, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: the rows are fixed in order
          <View key={i}>
            {i > 0 ? (
              <View
                style={{
                  height: 0.5,
                  marginLeft: 16,
                  backgroundColor: PlatformColor("separator"),
                }}
              />
            ) : null}
            {row}
          </View>
        ))}
      </View>
    </View>
  );
}

/** Small grey text under a section. */
export function Note({ children }: { children: string }) {
  return (
    <Text
      style={{
        fontSize: 13,
        color: PlatformColor("secondaryLabel"),
        marginHorizontal: 32,
        marginTop: 8,
      }}
    >
      {children}
    </Text>
  );
}

function Square({ sf, color }: { sf: string; color: string }) {
  return (
    <View
      style={{
        width: 29,
        height: 29,
        borderRadius: 7,
        alignItems: "center",
        justifyContent: "center",
        backgroundColor: PlatformColor(color),
      }}
    >
      <SymbolView name={sf as never} size={17} tintColor={PlatformColor("white")} />
    </View>
  );
}

function toneColor(tone: ItemProps["tone"]): ColorValue {
  if (tone === "danger") return PlatformColor("systemRed");
  return PlatformColor(tone === "action" ? "systemOrange" : "label");
}

export function Item({ title, value, icon, chevron, checked, tone, disabled, onPress }: ItemProps) {
  const color = disabled ? PlatformColor("tertiaryLabel") : toneColor(tone);
  return (
    <Pressable
      accessibilityRole={onPress ? "button" : "text"}
      disabled={disabled || !onPress}
      onPress={onPress}
      style={({ pressed }) => ({
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        minHeight: 44,
        paddingHorizontal: 16,
        paddingVertical: 8,
        backgroundColor: pressed ? PlatformColor("systemFill") : "transparent",
      })}
    >
      {icon ? <Square sf={icon.sf} color={icon.color} /> : null}
      <Text style={{ fontSize: 17, color, flexShrink: 1, flexGrow: 1 }}>{title}</Text>
      {value ? (
        <Text style={{ fontSize: 17, color: PlatformColor("secondaryLabel"), flexShrink: 2 }}>
          {value}
        </Text>
      ) : null}
      {checked ? (
        <SymbolView name="checkmark" size={15} tintColor={PlatformColor("systemOrange")} />
      ) : null}
      {chevron ? (
        <SymbolView name="chevron.right" size={13} tintColor={PlatformColor("tertiaryLabel")} />
      ) : null}
    </Pressable>
  );
}
