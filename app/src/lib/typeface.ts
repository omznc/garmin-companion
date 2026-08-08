/**
 * Whether the display face is the serif.
 *
 * Instrument Serif is most of this app's character, but it's a display face
 * with thin strokes, and at small sizes on a low-DPI screen some people find
 * it genuinely hard to read. Turning it off swaps one CSS variable, which is
 * why every serif rule in `styles.css` goes through `--serif` rather than
 * naming the family.
 */

export type Typeface = "serif" | "sans";

const KEY = "garmin-companion:typeface";

export function loadTypeface(): Typeface {
  return localStorage.getItem(KEY) === "sans" ? "sans" : "serif";
}

export function applyTypeface(t: Typeface): void {
  // Only the off state gets an attribute; the serif is the default and needs
  // no marker.
  if (t === "sans") document.documentElement.setAttribute("data-serif", "off");
  else document.documentElement.removeAttribute("data-serif");
  localStorage.setItem(KEY, t);
}
