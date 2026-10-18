import type { ReactNode } from "react";
import { Host } from "@expo/ui";
import { Column, Text } from "@expo/ui/jetpack-compose";
import { fillMaxWidth, paddingAll } from "@expo/ui/jetpack-compose/modifiers";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import { useHostTheme, useMd } from "../theme/md";

export function Placeholder({
  title,
  back,
  children,
}: {
  title: string;
  back?: boolean;
  children?: ReactNode;
}) {
  const md = useMd();
  const host = useHostTheme();
  return (
    <LargeAppBarScreen title={title} back={back}>
      <Host matchContents={{ vertical: true }} {...host} style={{ width: "100%" }}>
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
