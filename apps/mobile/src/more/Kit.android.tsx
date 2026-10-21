// The list pieces of More on Android: grouped Material 3 rows under the app bar.
// Each section is one rounded container with a large outer corner and small inner corners.
import MaterialIcons from "@expo/vector-icons/MaterialIcons";
import type { ReactNode } from "react";
import { View } from "react-native";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import {
  Content,
  Group,
  type IconName,
  ListRow,
  SectionLabel,
  T,
  type Tone,
} from "../components/MdKit";
import { useMd } from "../theme/md";

export function useColors() {
  const md = useMd();
  return {
    text: md.onSurface,
    muted: md.onSurfaceVariant,
    field: md.surfaceContainerHigh,
    accent: md.primary,
    onAccent: md.onPrimary,
    warn: md.error,
  };
}

export type ItemProps = {
  title: string;
  value?: string;
  /** Used on iOS only. */
  icon?: { sf: string; color: string };
  /** Used on Android only: a Material icon in a tonal circle. */
  md?: { icon: IconName; tone: Tone };
  chevron?: boolean;
  checked?: boolean;
  /** Used on iOS only: a switch with this state. Android has its own row. */
  on?: boolean;
  tone?: "action" | "danger";
  disabled?: boolean;
  onPress?: () => void;
};

/** A More page. The home page has the large bar. A settings page has the small bar and a back button. */
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
    <LargeAppBarScreen title={title} back={!large} small={!large}>
      <Content>{children}</Content>
    </LargeAppBarScreen>
  );
}

export function Section({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <View style={{ gap: 8 }}>
      {title ? <SectionLabel>{title}</SectionLabel> : null}
      <Group>{children}</Group>
    </View>
  );
}

/** Small grey text under a section. */
export function Note({ children }: { children: string }) {
  const md = useMd();
  return (
    <View style={{ paddingHorizontal: 20 }}>
      <T v="bodyMedium" color={md.onSurfaceVariant}>
        {children}
      </T>
    </View>
  );
}

function Radio({ on }: { on: boolean }) {
  const md = useMd();
  return (
    <MaterialIcons
      name={on ? "radio-button-checked" : "radio-button-unchecked"}
      size={24}
      color={on ? md.primary : md.onSurfaceVariant}
    />
  );
}

export function Item({
  title,
  value,
  md: look,
  chevron,
  checked,
  tone,
  disabled,
  onPress,
}: ItemProps) {
  return (
    <ListRow
      icon={look?.icon}
      tone={look?.tone}
      title={title}
      supporting={value}
      chevron={chevron}
      trailing={checked === undefined ? undefined : <Radio on={checked} />}
      action={tone === "action"}
      danger={tone === "danger"}
      disabled={disabled}
      onPress={onPress}
    />
  );
}
