/**
 * What the app looks like: a base mode, and optionally a palette that overrides
 * it.
 *
 * Two settings rather than one because they answer different questions. The
 * mode — light, dark, match system — is about the room you're sitting in, and
 * "match system" means it can change without you. A palette is a taste, and a
 * taste doesn't flip at sunset: each one here is a fixed set of colours that is
 * either light or dark by construction. So picking one settles the question the
 * mode was there to answer, and the mode stops applying. It isn't cleared —
 * it's still what you drop back to — it just stops being consulted, which is
 * the thing Settings has to say out loud.
 *
 * The state lives here rather than in a hook because four components read it
 * (the shell, the sidebar, Settings, Setup) and a `useState` each meant four
 * copies that only agreed until one of them changed. Subscribing to one value
 * is what lets the sidebar know the palette moved.
 */

import { listen } from "@tauri-apps/api/event";
import { themesList, type CustomTheme } from "./api";
import { applyTokens } from "./customTheme";
import { setBarAppearance } from "./android";
import { dynamicAvailable, dynamicTheme, refresh as refreshDynamic } from "./dynamic";

export type Theme = "light" | "dark" | "system";

/** The two the stylesheet actually knows about. Everything resolves to one. */
export type Mode = "light" | "dark";

/** A premade palette. `null` is the built-in one, which the mode still drives. */
export type Palette = string | null;

export type Preset = {
  /** Matches a `[data-palette="…"]` block in `styles.css`, which is where the
   *  colours themselves live — nothing here knows a hex. */
  id: string;
  name: string;
  note: string;
  /** Which of the two it's a version of. Fixed: it's what makes the mode
   *  inert, and it's what the dark-only rules in `styles.css` key off. */
  appearance: Mode;
};

/**
 * The shipped palettes.
 *
 * Four, and each one earns its place by being somewhere the default isn't. The
 * app's own palette is warm paper and ember, so these go where that can't:
 * cold, green, neutral, and a warm light that isn't orange. A fifth in the same
 * family as one of these would only make the list harder to choose from.
 */
export const PRESETS: readonly Preset[] = [
  { id: "nocturne", name: "Nocturne", note: "Cold midnight, soft blue", appearance: "dark" },
  { id: "moss", name: "Moss", note: "Deep forest, sage", appearance: "dark" },
  { id: "newsprint", name: "Newsprint", note: "Neutral grey, ink", appearance: "light" },
  { id: "linen", name: "Linen", note: "Warm oat, plum", appearance: "light" },
];

/**
 * The built-in palette's two handles.
 *
 * `styles.css` gives its base light and dark blocks these names alongside
 * `:root`, purely so a swatch can preview them the same way it previews a
 * preset — by wearing the attribute. Not selectable, and not in `PRESETS`.
 */
export const BUILT_IN: Record<Mode, string> = { light: "paper", dark: "ink" };

/**
 * Material You: the wallpaper's colours, on Android 12 and up.
 *
 * The second palette in the app that the mode still applies to, and the first
 * that isn't the built-in. Every other one — shipped or hand-written — declares
 * a fixed `appearance` and that fixedness is the point: it's what makes the
 * mode inert while the palette is on. This one can't work that way, because
 * Android derives a light set and a dark set from the same wallpaper, so
 * pinning it to either would break "match system" for the palette that most
 * wants it. So it isn't a `Preset`; it's a case in `compute()`.
 *
 * See `lib/dynamic.ts` for where the colours come from and how the ramps are
 * mapped onto the app's seven.
 */
export const DYNAMIC = "dynamic";

const THEME_KEY = "garmin-companion:theme";
const PALETTE_KEY = "garmin-companion:palette";

/**
 * Written when someone picks the built-in palette on purpose.
 *
 * The stored value used to be absent for both "never chose one" and "chose
 * Default", which was fine while they meant the same thing. They stopped
 * meaning the same thing the moment Android got a different default: without
 * this, choosing Default on a phone would be forgotten on the next launch and
 * the wallpaper palette would come back over the top of it.
 */
const DEFAULT = "default";

const media = () => window.matchMedia("(prefers-color-scheme: dark)");

/**
 * A custom theme's slug, wrapped so it can't be mistaken for a preset id.
 *
 * One field holds both because there is only ever one palette in force, and
 * splitting it into two would create a state where both are set. The prefix is
 * what keeps a theme someone named "Moss" from shadowing the shipped one.
 */
const CUSTOM = "custom:";

export const customPalette = (slug: string): Palette => `${CUSTOM}${slug}`;

export function customSlug(palette: Palette): string | null {
  return palette?.startsWith(CUSTOM) ? palette.slice(CUSTOM.length) : null;
}

export function findPreset(id: Palette): Preset | null {
  return PRESETS.find((p) => p.id === id) ?? null;
}

/** What the mode alone would give you — i.e. what "Default" is a preview of. */
export function builtIn(theme: Theme): Mode {
  return theme === "system" ? (media().matches ? "dark" : "light") : theme;
}

export type ThemeState = {
  theme: Theme;
  palette: Palette;
  /** What's actually on screen, after a palette has had its say. */
  mode: Mode;
  /** The shipped palette in force, if the selection names one. */
  preset: Preset | null;
  /** The custom theme in force, if the selection names one that has loaded. */
  custom: CustomTheme | null;
  /** Everything currently in the themes folder. */
  customs: CustomTheme[];
};

function compute(theme: Theme, palette: Palette, customs: CustomTheme[]): ThemeState {
  const preset = findPreset(palette);
  const slug = customSlug(palette);
  const saved = slug ? (customs.find((c) => c.slug === slug) ?? null) : null;

  // A selected custom theme that hasn't loaded yet resolves to the mode
  // underneath rather than to nothing, so the window is never unstyled — the
  // theme snaps in when the folder read lands a moment later.
  //
  // The wallpaper palette resolves that way too, but for the opposite reason:
  // it has no fixed appearance to fall back from, so the mode is what picks
  // which half of it applies. That ordering is why the theme is built after
  // the mode rather than beside it.
  const mode = preset ? preset.appearance : saved ? saved.appearance : builtIn(theme);
  const custom = palette === DYNAMIC ? dynamicTheme(mode) : saved;

  return { theme, palette, mode, preset, custom, customs };
}

/**
 * The selected custom theme's own record, mirrored beside the selection.
 *
 * The themes folder is read over IPC, which resolves a tick or two after the
 * first paint — long enough to see the default palette flash past on every
 * launch. Keeping a copy of the one in force means the first paint is already
 * right, and the folder read only ever confirms it or corrects it.
 */
const CACHE_KEY = "garmin-companion:palette-cache";

function cached(slug: string): CustomTheme | null {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(CACHE_KEY) ?? "null");
    const theme = raw as CustomTheme | null;
    return theme && theme.slug === slug ? theme : null;
  } catch {
    // A half-written or hand-mangled cache is the same as no cache.
    return null;
  }
}

/**
 * What a build with nothing stored opens on.
 *
 * Material You where there is one, the app's own palette everywhere else. This
 * is the only place the Android default is decided, and it is only ever
 * consulted when the key is absent — an explicit choice, Default included,
 * writes something and is honoured from then on.
 */
function initialPalette(): Palette {
  return dynamicAvailable() ? DYNAMIC : null;
}

function stored(): ThemeState {
  const t = localStorage.getItem(THEME_KEY);
  const theme = t === "light" || t === "dark" || t === "system" ? t : "system";

  const p = localStorage.getItem(PALETTE_KEY);
  const palette =
    p === DEFAULT
      ? null
      : // Not on this phone — an older Android, or a desktop build reading a
        // profile written on one. Falls through to whatever is default here
        // rather than to an unstyled window.
        p === DYNAMIC
        ? initialPalette()
        : // A palette dropped from a later build reads as no palette rather
          // than as an attribute with no stylesheet behind it.
          findPreset(p) || customSlug(p)
          ? p
          : initialPalette();

  const slug = customSlug(palette);
  const seed = slug ? cached(slug) : null;
  return compute(theme, palette, seed ? [seed] : []);
}

let current = stored();
const listeners = new Set<() => void>();

/**
 * Write the current state onto the document.
 *
 * A shipped palette rides on an attribute the stylesheet has a block for; a
 * custom one has no block, so its tokens go on inline. Both are cleared when
 * absent, which is what makes switching between the two kinds work — otherwise
 * a preset's attribute would still be matching underneath a custom theme's
 * inline values.
 */
function paint(): void {
  const root = document.documentElement;
  root.setAttribute("data-theme", current.mode);
  if (current.preset) root.setAttribute("data-palette", current.preset.id);
  else root.removeAttribute("data-palette");
  applyTokens(current.custom);
  // The status and navigation bars are the one part of the app's surface it
  // doesn't paint, and they have to agree with the part it does. Driven from
  // the resolved mode rather than the phone's, because the two disagree
  // whenever the app's appearance is not the system's. No-op off Android.
  setBarAppearance(current.mode === "light");
}

/**
 * Show a theme without selecting it — what the editor does on every keystroke.
 *
 * Deliberately not a commit: nothing is written, nothing is announced, and
 * `repaint()` puts back whatever is actually selected. An editor you had to
 * save before you could see the change would be a form, not an editor.
 *
 * It sets `data-theme` as well as the tokens, because a few rules key off it
 * directly rather than off a variable — a light draft previewed from a dark
 * palette would otherwise keep the dark menu.
 */
export function previewTheme(theme: CustomTheme): void {
  const root = document.documentElement;
  root.setAttribute("data-theme", theme.appearance);
  root.removeAttribute("data-palette");
  applyTokens(theme);
}

/** Undo a live preview: put back whatever is actually selected. */
export function repaint(): void {
  paint();
}

function commit(theme: Theme, palette: Palette, customs = current.customs): void {
  // A new object every time on purpose: `useSyncExternalStore` compares
  // snapshots by identity, and the mode can move while the other two don't.
  current = compute(theme, palette, customs);
  localStorage.setItem(THEME_KEY, theme);
  // Always written, `null` included — see `DEFAULT` for why the absence of this
  // key can no longer stand for the built-in palette.
  localStorage.setItem(PALETTE_KEY, palette ?? DEFAULT);
  // Only a theme that came off disk is worth mirroring. The wallpaper palette
  // is rebuilt from the window binding before the first line of the app runs,
  // so it is already as early as the cache would make it — and caching it here
  // would evict a real theme's copy on the way past.
  if (current.custom && customSlug(palette)) {
    localStorage.setItem(CACHE_KEY, JSON.stringify(current.custom));
  } else {
    localStorage.removeItem(CACHE_KEY);
  }
  paint();
  for (const fn of listeners) fn();
}

export function setTheme(theme: Theme): void {
  commit(theme, current.palette);
}

export function setPalette(palette: Palette): void {
  commit(current.theme, palette);
}

/**
 * Take a fresh listing of the themes folder.
 *
 * A theme that has vanished — deleted from Settings, or dragged out of the
 * folder — stops being the selection rather than leaving the app pointing at a
 * file that isn't there. An edit to the file backing the current selection is
 * picked up the same way, which is what makes hand-editing one work.
 */
export function setCustomThemes(customs: CustomTheme[]): void {
  const slug = customSlug(current.palette);
  const gone = slug !== null && !customs.some((c) => c.slug === slug);
  commit(current.theme, gone ? null : current.palette, customs);
}

/** Re-read the folder. Safe to call whenever a theme may have changed. */
export async function refreshCustomThemes(): Promise<void> {
  try {
    setCustomThemes(await themesList());
  } catch {
    // No folder yet, or no Tauri backend to ask. "Couldn't read the folder" is
    // not the same as "the folder is empty", so the known list stays put — the
    // alternative deselects someone's theme over a transient failure.
  }
}

export function getThemeState(): ThemeState {
  return current;
}

export function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

/**
 * Put the stored appearance on the document.
 *
 * Called from `main.tsx` before the tree renders rather than from an effect
 * inside it: an effect runs after the first paint, which is a frame of the
 * wrong colour on every launch. Explicit rather than a side effect of importing
 * this module, so nothing depends on which screen happens to pull it in first.
 */
export function startAppearance(): void {
  paint();
  // Then confirm it against the folder. Nothing waits on this: the paint above
  // has already used the mirrored copy of whatever is selected, so this only
  // corrects a theme edited since, or drops one deleted since.
  void refreshCustomThemes();

  /**
   * The folder can change without this app touching it — the model writes one
   * from Ask, or a file gets edited in a text editor — and until it's re-read,
   * a theme that exists is one the app has never heard of. That was reachable
   * only by reloading the window, which is not something anyone should have to
   * know to do.
   *
   * Two triggers, because there are two ways it happens. The model's writes
   * announce themselves; a hand edit can't, so coming back to the window is
   * taken as the moment to look again. Both land on the same reconcile, and a
   * listing that matches what's already loaded changes nothing on screen.
   */
  void listen<{ apply?: string | null }>("themes:changed", async (event) => {
    await refreshCustomThemes();
    const apply = event.payload?.apply;
    // Absent means the list moved but the selection didn't. Empty string is
    // the model asking for the built-in palette back, which is `null` here.
    if (apply !== undefined && apply !== null) {
      setPalette(apply === "" ? null : customPalette(apply));
    }
  });

  window.addEventListener("focus", () => {
    void refreshCustomThemes();
    // And the wallpaper, which can be changed while the app is in the
    // background and announces itself to nobody. Coming back to the window is
    // the moment to look — a palette that shifts as you return is a change you
    // just made, where one that shifted mid-session would be one that happened
    // to you. Only repaints if the ramps actually moved.
    if (current.palette === DYNAMIC && refreshDynamic()) {
      commit(current.theme, current.palette);
    }
  });
}

// One listener for the life of the process. An explicit light or dark was never
// the OS's call, so only "match system" listens at all.
//
// It stays listening under a palette, which used to look redundant — a palette
// with a fixed appearance holds what's on screen, so nothing repaints. Two
// reasons it still has to run. "Match system" is what you'd drop back to, and
// Settings draws that as a live swatch on the Default row, so the flip changes
// the answer even while nothing is showing it. And under Material You it is not
// redundant at all: that palette has both appearances and the mode is what
// picks between them, so sunset genuinely repaints the app.
media().addEventListener("change", () => {
  if (current.theme !== "system") return;
  commit(current.theme, current.palette);
});
