// Ask on iOS: the native large title, system colours, Dynamic Type, the native text input.
import { PlatformColor } from "react-native";
import { AskView } from "../components/AskView";

export function Ask() {
  return (
    <AskView
      top={0}
      theme={{
        bg: PlatformColor("systemGroupedBackground"),
        text: PlatformColor("label"),
        muted: PlatformColor("secondaryLabel"),
        faint: PlatformColor("tertiaryLabel"),
        accent: PlatformColor("systemOrange"),
        onAccent: PlatformColor("white"),
        bubble: PlatformColor("secondarySystemGroupedBackground"),
        field: PlatformColor("secondarySystemGroupedBackground"),
        line: PlatformColor("separator"),
        warn: PlatformColor("systemRed"),
      }}
    />
  );
}
