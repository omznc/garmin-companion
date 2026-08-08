/**
 * Material You: the wallpaper's colours, as a palette this app can wear.
 *
 * Android derives five tonal ramps from whatever is behind the home screen, and
 * exposes them as framework resources. `MainActivity.kt` reads all sixty-five
 * and binds them to the window before the first line of the app runs — the same
 * guarantee `platform.ts` documents for `platform()` and `COMPOSITES_ALPHA`, and
 * for the same reason: an asynchronous palette means painting one frame in the
 * wrong colour on every launch.
 *
 * What arrives is a ladder of tones with no opinion about what any of them is
 * for. Deciding that is this file's job, and it is a design decision rather than
 * a lookup — which is why it lives here, next to `customTheme.ts`, rather than
 * in the Kotlin.
 *
 * The result is shaped as a `CustomTheme` so it goes through exactly the same
 * seven-colours-in, fifteen-tokens-out derivation every other palette does. It
 * is not a theme on disk and never gets saved; the shape is the interface.
 */
import type { Appearance, CustomTheme } from "./api";
import { readDynamicColors } from "./android";
import { IS_MOBILE } from "./platform";

/** What `MainActivity.kt` binds: `{"accent1_500": "#7b5ea7", …}`. */
type Ramps = Record<string, string>;

/**
 * Android's tone ladder, lightest first. 0 is white and 1000 is black — the
 * inverse of Material's own tone numbering, which counts lightness upwards.
 */
const TONES = [0, 10, 50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000];

/**
 * Read once, at import.
 *
 * The wallpaper cannot change while a frame is being painted, and the whole
 * point of the binding is that the answer is already there — so this is a
 * constant for the life of the window rather than something to call. A wallpaper
 * changed while the app is running is picked up by `refresh()` below.
 */
let ramps: Ramps | null = read();

function read(): Ramps | null {
  const raw = readDynamicColors();
  if (!raw) return null;
  try {
    const parsed: unknown = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? (parsed as Ramps) : null;
  } catch {
    return null;
  }
}

/** Whether this build can offer Material You at all. */
export function dynamicAvailable(): boolean {
  return IS_MOBILE && ramps !== null;
}

/**
 * Re-read the ramps. Returns whether they moved.
 *
 * Changing the wallpaper doesn't restart the app, so without this the palette
 * would be whatever it was at launch until the next cold start. Called on focus
 * rather than pushed, because Android has no callback for this that survives
 * the app being in the background — and by the time you have come back to it,
 * a repaint is a change you were expecting rather than one that happens under
 * your hands.
 */
export function refresh(): boolean {
  const next = read();
  const moved = JSON.stringify(next) !== JSON.stringify(ramps);
  ramps = next;
  return moved;
}

/* ------------------------------------------------------------- the mapping --- */

/**
 * Which tone of which ramp does each of the app's seven jobs.
 *
 * The grounds and the text come from the neutrals, which is what carries the
 * wallpaper as a tint rather than as a colour — a page is not supposed to be
 * purple, it is supposed to be paper that knows the wallpaper is purple. The
 * accent is the one place the hue is allowed to be the point.
 *
 * `warn` is deliberately absent. It is the one token with a job that isn't
 * "look like the rest of this" — `customTheme.ts` calls it "only ever
 * warnings" — and the obvious source for it, `accent3`, is derived from the
 * same wallpaper as `acc`. On anything orange the two land within a few degrees
 * of each other and a warning stops reading as one. The built-in warn is a
 * fixed amber that has no such problem, and a palette borrowing one colour from
 * the house palette is a much smaller cost than a warning nobody notices.
 */
const MAP: Record<
  Appearance,
  Record<"bg" | "bg2" | "fg" | "muted" | "faint" | "acc", [string, number]>
> = {
  light: {
    bg: ["neutral1", 10],
    bg2: ["neutral1", 50],
    fg: ["neutral1", 900],
    muted: ["neutral2", 700],
    faint: ["neutral2", 500],
    acc: ["accent1", 600],
  },
  dark: {
    bg: ["neutral1", 900],
    bg2: ["neutral1", 800],
    fg: ["neutral1", 50],
    muted: ["neutral2", 200],
    faint: ["neutral2", 400],
    acc: ["accent1", 200],
  },
};

/** The house warn, in both appearances. Lifted from `styles.css`. */
const WARN: Record<Appearance, string> = { light: "#8a6a1f", dark: "#c0a15a" };

/* ------------------------------------------------------------- contrast --- */

const srgb = (hex: string) => [
  parseInt(hex.slice(1, 3), 16) / 255,
  parseInt(hex.slice(3, 5), 16) / 255,
  parseInt(hex.slice(5, 7), 16) / 255,
];

/** WCAG relative luminance. */
function luminance(hex: string): number {
  const [r, g, b] = srgb(hex).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

/**
 * The floors the two quiet text steps have to clear against the page.
 *
 * Lower than AA's 4.5 on purpose. These are not body copy — `faint` is captions
 * and `muted` is labels — and the built-in palettes this app ships don't meet
 * 4.5 on them either, by design. What the guard is for is the case a hand-picked
 * palette can't produce: a saturated wallpaper pushes the neutral ramp far
 * enough that `faint` lands within touching distance of the page and the
 * captions simply disappear. 3.0 is the step where that stops being true.
 */
const FLOOR = { muted: 4.0, faint: 3.0 };

/**
 * Walk a tone towards the far end of the ramp until it clears the floor.
 *
 * Along the ladder rather than by darkening the colour, so the result is still
 * one of Android's own tones and still belongs to the same wallpaper. Steps
 * away from the page: darker on a light appearance, lighter on a dark one.
 *
 * Falls back to the last tone it managed to read. A ramp that runs out before
 * clearing the floor is a wallpaper with almost no range in it, and the end of
 * the ladder is the best answer available.
 */
function legible(
  r: Ramps,
  ramp: string,
  tone: number,
  bg: string,
  floor: number,
  appearance: Appearance,
): string | null {
  const ladder = appearance === "light" ? TONES : [...TONES].reverse();
  const from = ladder.indexOf(tone);
  if (from < 0) return r[`${ramp}_${tone}`] ?? null;

  let best: string | null = null;
  for (let i = from; i < ladder.length; i++) {
    const hex = r[`${ramp}_${ladder[i]}`];
    if (!hex) continue;
    best = hex;
    if (contrast(hex, bg) >= floor) return hex;
  }
  return best;
}

/**
 * The wallpaper as one of this app's palettes, or null if there isn't one.
 *
 * Both appearances are available from the same ramps, which is what makes this
 * the only palette besides the built-in that "match system" still applies to —
 * see the note on `Preset.appearance` in `theme.ts`.
 */
export function dynamicTheme(appearance: Appearance): CustomTheme | null {
  const r = ramps;
  if (!r) return null;

  const pick = (key: keyof (typeof MAP)["light"]): string | null => {
    const [ramp, tone] = MAP[appearance][key];
    return r[`${ramp}_${tone}`] ?? null;
  };

  const bg = pick("bg");
  const bg2 = pick("bg2");
  const fg = pick("fg");
  const acc = pick("acc");
  // Any missing one means the ramps aren't what this expects, and half a
  // palette is worse than none — the app falls back to its own.
  if (!bg || !bg2 || !fg || !acc) return null;

  const muted = legible(r, ...MAP[appearance].muted, bg, FLOOR.muted, appearance);
  const faint = legible(r, ...MAP[appearance].faint, bg, FLOOR.faint, appearance);
  if (!muted || !faint) return null;

  return {
    slug: "",
    name: "Material You",
    appearance,
    note: "From your wallpaper",
    colors: { bg, bg2, fg, muted, faint, acc, warn: WARN[appearance] },
  };
}
