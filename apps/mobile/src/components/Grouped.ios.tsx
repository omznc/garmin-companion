// The grouped list pieces that the Health and Sleep screens share. System colours and Dynamic Type.
import type { ReactNode } from "react";
import { ActivityIndicator, Button, PlatformColor, Text, View } from "react-native";

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

export function Row({ left, right, last }: { left: string; right?: string; last?: boolean }) {
  return (
    <View
      style={{
        flexDirection: "row",
        justifyContent: "space-between",
        alignItems: "center",
        gap: 12,
        minHeight: 44,
        paddingHorizontal: 16,
        paddingVertical: 10,
        borderBottomWidth: last ? 0 : 0.5,
        borderBottomColor: separator,
      }}
    >
      <Text style={{ fontSize: 17, color: label, flexShrink: 1 }}>{left}</Text>
      {right ? (
        <Text style={{ fontSize: 17, color: secondary, flexShrink: 1, textAlign: "right" }}>
          {right}
        </Text>
      ) : null}
    </View>
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
