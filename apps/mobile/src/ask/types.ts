import type { ColorValue } from "react-native";

// The colours of the Ask screen. Android fills them from the Material 3 roles,
// iOS from the system colours. No component here has a fixed colour.
export type ChatTheme = {
  bg: ColorValue;
  /** The main text. */
  text: ColorValue;
  /** The quiet text, and the quiet icons. */
  muted: ColorValue;
  /** A dash for an empty cell, and a disabled mark. */
  faint: ColorValue;
  /** Outlines: chips, table, trace rule. */
  line: ColorValue;
  /** The text field, and a code background. */
  field: ColorValue;
  /** The head row of a table. */
  header: ColorValue;
  /** The user's message. */
  bubble: ColorValue;
  onBubble: ColorValue;
  /** The send button, links. */
  accent: ColorValue;
  onAccent: ColorValue;
  error: ColorValue;
  /** The fill of a chip while it is pressed (iOS). */
  pressed: ColorValue;
  /** The stop button. */
  stop: ColorValue;
  onStop: ColorValue;
};
