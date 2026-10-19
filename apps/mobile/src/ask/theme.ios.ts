import { PlatformColor } from "react-native";
import type { ChatTheme } from "./types";

export type { ChatTheme } from "./types";

// The colours are system colours, so they follow light, dark and contrast.
const THEME: ChatTheme = {
  bg: PlatformColor("systemGroupedBackground"),
  text: PlatformColor("label"),
  muted: PlatformColor("secondaryLabel"),
  faint: PlatformColor("tertiaryLabel"),
  line: PlatformColor("separator"),
  field: PlatformColor("secondarySystemGroupedBackground"),
  header: PlatformColor("tertiarySystemFill"),
  bubble: PlatformColor("secondarySystemFill"),
  onBubble: PlatformColor("label"),
  accent: PlatformColor("systemOrange"),
  onAccent: PlatformColor("white"),
  error: PlatformColor("systemRed"),
  pressed: PlatformColor("secondarySystemFill"),
  stop: PlatformColor("secondarySystemFill"),
  onStop: PlatformColor("label"),
};

export function useChatTheme(): ChatTheme {
  return THEME;
}
