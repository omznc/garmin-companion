// The grouped list pieces that the Health and Sleep screens share. System colours and Dynamic Type.
import { SymbolView } from "expo-symbols";
import type { ReactNode } from "react";
import { ActivityIndicator, Button, PlatformColor, Pressable, Text, View } from "react-native";

export const label = PlatformColor("label");
export const secondary = PlatformColor("secondaryLabel");
export const tint = PlatformColor("systemOrange");
const cell = PlatformColor("secondarySystemGroupedBackground");
const separator = PlatformColor("separator");

export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <View style={{ marginTop: 24 }}>
      {title ? (
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
      ) : null}
      <View
        style={{
          backgroundColor: cell,
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

/** A row of a group. With `onPress` it is a button with a chevron. */
export function Row({
  left,
  right,
  last,
  onPress,
}: {
  left: string;
  right?: string;
  last?: boolean;
  onPress?: () => void;
}) {
  return (
    <Pressable
      accessibilityRole={onPress ? "button" : "text"}
      disabled={!onPress}
      onPress={onPress}
      style={({ pressed }) => ({
        flexDirection: "row",
        justifyContent: "space-between",
        alignItems: "center",
        gap: 12,
        minHeight: 44,
        paddingHorizontal: 16,
        paddingVertical: 10,
        borderBottomWidth: last ? 0 : 0.5,
        borderBottomColor: separator,
        opacity: pressed ? 0.6 : 1,
      })}
    >
      <Text style={{ fontSize: 17, color: label, flexShrink: 1 }}>{left}</Text>
      {right ? (
        <Text style={{ fontSize: 17, color: secondary, flexShrink: 1, textAlign: "right" }}>
          {right}
        </Text>
      ) : null}
      {onPress ? (
        <SymbolView name="chevron.right" size={13} tintColor={PlatformColor("tertiaryLabel")} />
      ) : null}
    </Pressable>
  );
}

/** Plain text in a group. The first line is the main one. */
export function Note({ children, small }: { children: string; small?: boolean }) {
  return (
    <View style={{ padding: 16 }}>
      <Text style={{ fontSize: small ? 13 : 15, color: small ? secondary : label }}>
        {children}
      </Text>
    </View>
  );
}

export function Loading() {
  return <ActivityIndicator style={{ margin: 32 }} />;
}

export function ErrorGroup({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <Group>
      <View style={{ padding: 16 }}>
        <Text style={{ fontSize: 15, color: secondary }}>{message}</Text>
        <Button title="Try again" onPress={onRetry} />
      </View>
    </Group>
  );
}
