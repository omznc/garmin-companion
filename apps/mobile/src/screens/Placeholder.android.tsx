import type { ReactNode } from "react";
import { Host } from "@expo/ui";
import { Column, Text } from "@expo/ui/jetpack-compose";
import { fillMaxWidth, paddingAll } from "@expo/ui/jetpack-compose/modifiers";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import { HOST_SEED, useMd } from "../theme/md";

export function Placeholder({
  title,
  settings,
  children,
}: {
  title: string;
  settings?: boolean;
  children?: ReactNode;
}) {
  const md = useMd();
  return (
    <LargeAppBarScreen title={title} settings={settings}>
      <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
        <Column modifiers={[fillMaxWidth(), paddingAll(16)]}>
          <Text style={{ typography: "bodyLarge" }} color={md.onSurfaceVariant}>
            This screen comes in a later phase.
          </Text>
        </Column>
      </Host>
      {children}
    </LargeAppBarScreen>
  );
}
