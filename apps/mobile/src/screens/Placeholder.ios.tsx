import type { ReactNode } from "react";
import { PlatformColor, ScrollView, Text } from "react-native";

export function Placeholder({
  children,
}: {
  title?: string;
  back?: boolean;
  children?: ReactNode;
}) {
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ padding: 16 }}
    >
      <Text style={{ fontSize: 17, color: PlatformColor("secondaryLabel") }}>
        This screen comes in a later phase.
      </Text>
      {children}
    </ScrollView>
  );
}
