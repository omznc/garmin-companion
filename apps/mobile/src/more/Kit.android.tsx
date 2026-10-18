// The list pieces of More on Android: Material 3 list items under the app bar.
import { Host } from "@expo/ui";
import { Column, Icon, ListItem, Text } from "@expo/ui/jetpack-compose";
import { clickable, fillMaxWidth, paddingAll } from "@expo/ui/jetpack-compose/modifiers";
import type { ReactNode } from "react";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import { HOST_SEED, useMd } from "../theme/md";

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
    <LargeAppBarScreen title={title} back={!large}>
      {children}
    </LargeAppBarScreen>
  );
}

export function Section({ title, children }: { title?: string; children: ReactNode }) {
  const md = useMd();
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column modifiers={[fillMaxWidth()]}>
        {title ? (
          <Column modifiers={[fillMaxWidth(), paddingAll(16)]}>
            <Text style={{ typography: "labelLarge" }} color={md.primary}>
              {title}
            </Text>
          </Column>
        ) : null}
        {children}
      </Column>
    </Host>
  );
}

/** Small grey text under a section. */
export function Note({ children }: { children: string }) {
  const md = useMd();
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column modifiers={[fillMaxWidth(), paddingAll(16)]}>
        <Text style={{ typography: "bodySmall" }} color={md.onSurfaceVariant}>
          {children}
        </Text>
      </Column>
    </Host>
  );
}

function toneColor(md: ReturnType<typeof useMd>, tone: ItemProps["tone"], disabled?: boolean) {
  if (disabled) return md.onSurfaceVariant;
  if (tone === "danger") return md.error;
  return tone === "action" ? md.primary : md.onSurface;
}

export function Item({ title, value, chevron, checked, tone, disabled, onPress }: ItemProps) {
  const md = useMd();
  const color = toneColor(md, tone, disabled);
  return (
    <ListItem modifiers={[fillMaxWidth(), ...(onPress && !disabled ? [clickable(onPress)] : [])]}>
      <ListItem.HeadlineContent>
        <Text color={color}>{title}</Text>
      </ListItem.HeadlineContent>
      {value ? (
        <ListItem.SupportingContent>
          <Text color={md.onSurfaceVariant}>{value}</Text>
        </ListItem.SupportingContent>
      ) : null}
      {checked || chevron ? (
        <ListItem.TrailingContent>
          <Icon
            source={
              checked
                ? require("../../assets/icons/check.xml")
                : require("../../assets/icons/chevron.xml")
            }
            size={24}
            tint={checked ? md.primary : md.onSurfaceVariant}
          />
        </ListItem.TrailingContent>
      ) : null}
    </ListItem>
  );
}
