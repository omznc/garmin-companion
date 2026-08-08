/**
 * Turning a saved theme's seven colours into the tokens the stylesheet reads.
 *
 * A custom theme declares what is genuinely a choice — page, nav ground, three
 * steps of text, an accent, a warning — and this fills in the rest. The rest is
 * everything with a correct answer: the two hairlines are the foreground at a
 * fixed fraction, the selection tint is the same idea one step quieter, the
 * elevation is a shadow whose colour follows the appearance, and the duotone
 * icons' back layer is the same construction the built-in dark palette uses.
 *
 * That division is what makes a theme writable by a model and editable in seven
 * colour wells rather than fifteen. It also keeps the derived tokens honest: a
 * theme cannot ship a hairline heavier than the text it separates, because it
 * never gets to name one.
 *
 * The numbers are lifted from the built-in palettes in `styles.css`, so a
 * custom theme is built the same way the shipped ones are — see the comments
 * there for why each is what it is.
 */
import type { Appearance, CustomTheme } from "./api";

/** The CSS custom properties `styles.css` expects, as a plain object. */
export type Tokens = Record<string, string>;

/** `#rrggbb` at a fraction of full opacity. */
const alpha = (hex: string, a: number) =>
  `color-mix(in srgb, ${hex} ${Math.round(a * 1000) / 10}%, transparent)`;

/**
 * The tokens for one theme.
 *
 * Used in two places from one definition: written onto `:root` when the theme
 * is in force, and set inline on a 26px swatch to preview a theme that isn't.
 * Any drift between those two would show up as a preview that lies.
 */
export function tokens(theme: CustomTheme): Tokens {
  const { bg, bg2, fg, muted, faint, acc, warn } = theme.colors;
  const dark = theme.appearance === "dark";

  return {
    "--bg": bg,
    "--bg2": bg2,
    "--fg": fg,
    "--mut": muted,
    "--faint": faint,
    "--acc": acc,
    "--warn": warn,

    "--line": alpha(fg, dark ? 0.13 : 0.11),
    "--line2": alpha(fg, dark ? 0.06 : 0.055),
    "--sel": alpha(fg, dark ? 0.05 : 0.045),

    // On paper the shadow is the ink it's cast by; on a dark ground a tinted
    // one disappears into the page and only a near-black still reads as a gap.
    "--lift": dark ? "0 8px 22px rgba(0, 0, 0, 0.5)" : `0 8px 22px ${alpha(fg, 0.14)}`,

    // Lifted toward the foreground on a dark theme, laid down as-is on a light
    // one. The long version of why is in `styles.css`, above the dark block.
    "--icon2": dark ? `color-mix(in srgb, ${acc} 74%, ${fg})` : acc,
    "--icon2-a": String(theme.iconTintAlpha ?? (dark ? 0.4 : 0.3)),
  };
}

/**
 * Put a theme on the document, or take it off again.
 *
 * Inline on the root element rather than as an injected `<style>` block,
 * because inline declarations beat every selector in the stylesheet without
 * having to reason about which `:root[data-…]` rule is also matching. Clearing
 * is exact — only the properties this module sets are removed, so nothing else
 * writing to the root style is disturbed.
 */
export function applyTokens(theme: CustomTheme | null): void {
  const style = document.documentElement.style;
  // Every key this can set, whether or not the incoming theme sets it, so
  // switching between two themes can't leave one's token behind on the other.
  const next = theme ? tokens(theme) : {};
  for (const key of Object.keys(tokens(BLANK))) {
    const value = next[key];
    if (value) style.setProperty(key, value);
    else style.removeProperty(key);
  }
}

/** Only ever used for its key set, above, and as the editor's starting point. */
const BLANK: CustomTheme = {
  slug: "",
  name: "",
  appearance: "light",
  note: "",
  colors: {
    bg: "#faf9f6",
    bg2: "#f4f2ed",
    fg: "#1b1a17",
    muted: "#79736a",
    faint: "#a9a296",
    acc: "#b0563a",
    warn: "#8a6a1f",
  },
};

/**
 * A theme to start editing from: the built-in palette in the matching
 * appearance, which is a working theme rather than a grid of black wells.
 *
 * Someone making their first theme is almost never designing from nothing —
 * they want this, but greener. Starting from the real values means the first
 * thing they change is the thing they came to change.
 */
export function blankTheme(appearance: Appearance): CustomTheme {
  if (appearance === "light") return { ...BLANK, colors: { ...BLANK.colors } };
  return {
    ...BLANK,
    appearance: "dark",
    colors: {
      bg: "#15140f",
      bg2: "#1c1b15",
      fg: "#ede8de",
      muted: "#8f887c",
      faint: "#6b6459",
      acc: "#c97354",
      warn: "#c0a15a",
    },
  };
}

/** The seven wells, in the order the editor shows them: grounds, then text,
 *  then the two colours. Labelled by the job, since that's what a value has to
 *  be judged against. */
export const FIELDS = [
  { key: "bg", label: "Page", note: "The ground everything sits on" },
  { key: "bg2", label: "Sidebar", note: "One step off the page" },
  { key: "fg", label: "Text", note: "Body copy, and anything you read" },
  { key: "muted", label: "Secondary", note: "Labels and supporting text" },
  { key: "faint", label: "Faint", note: "Captions — the quietest readable step" },
  { key: "acc", label: "Accent", note: "Links, selection, the tint behind icons" },
  { key: "warn", label: "Warning", note: "Only ever warnings" },
] as const satisfies ReadonlyArray<{
  key: keyof CustomTheme["colors"];
  label: string;
  note: string;
}>;
