import { useMd } from "../theme/md";
import type { ChatTheme } from "./types";

export type { ChatTheme } from "./types";

// Farmer's roles on the Material 3 roles of the app.
export function useChatTheme(): ChatTheme {
  const md = useMd();
  return {
    bg: md.background,
    text: md.onSurface,
    muted: md.onSurfaceVariant,
    faint: md.outline,
    line: md.outlineVariant,
    field: md.surfaceContainerHigh,
    header: md.surfaceContainer,
    bubble: md.secondaryContainer,
    onBubble: md.onSecondaryContainer,
    accent: md.primary,
    onAccent: md.onPrimary,
    error: md.error,
    pressed: md.surfaceContainerHigh,
    stop: md.secondaryContainer,
    onStop: md.onSecondaryContainer,
  };
}
