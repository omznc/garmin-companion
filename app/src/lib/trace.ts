/**
 * How a recorded trace is coloured.
 *
 * Shared between the map and the elevation profile, which are two drawings of
 * one set of samples: pointing at a moment on either marks it on both, and the
 * colour under the pointer means the same thing in both.
 *
 * The colouring carries two channels, not one. Colour alone — a ramp mixed out
 * of whatever accent the palette happens to be — is close to unreadable at the
 * width a route is drawn, which is why switching the metric used to look like
 * it did nothing. Weight carries the same value: harder, faster and higher are
 * drawn heavier. That reads at a glance, survives any palette, and survives
 * being drawn on top of a map.
 */
import { ZONE_FILL } from "../components/ZoneBar";
import type { ActivitySeries, ZoneProfile } from "./api";

/** Colouring available for a trace. */
export type ColourBy = "zone" | "pace" | "elevation" | "cadence" | "plain";

/** One sample's appearance. */
export interface Stroke {
  stroke: string;
  width: number;
}

/** Zones get the ladder the bars use, weighted so effort reads as weight. */
const ZONE_WIDTH = [2.6, 3.2, 4, 4.9, 5.8];

/** Steps in the continuous ramps. Enough to read as a gradient, few enough
 *  that the run-length grouping below still has runs to group. */
const STEPS = 8;

/**
 * Outliers are clipped rather than scaled to.
 *
 * One GPS-glitched 2 min/km sample sets the top of a pace ramp and pushes the
 * whole run into the bottom two steps — which is most of why the pace and
 * elevation views came out one flat colour. The ends of the ramp are the 5th
 * and 95th percentiles, so the ramp spends its range on the part of the
 * session that actually happened.
 */
const CLIP = 0.05;

/**
 * Samples are smoothed before they are coloured.
 *
 * Pace and cadence swing hard between neighbouring samples, and colouring them
 * raw produces a stipple where every third point is its own polyline. The eye
 * reads the average of that as one muddy colour, and the grouping below stops
 * grouping anything.
 */
const WINDOW = 7;

export function styleFor(
  series: ActivitySeries,
  zones: ZoneProfile,
  colour: ColourBy,
): (index: number) => Stroke | null {
  if (colour === "plain") return () => ({ stroke: "var(--acc)", width: 4 });

  if (colour === "zone") {
    return (i) => {
      const hr = series.hr[i];
      if (hr == null) return null;
      const z = zoneOf(hr, zones.floors);
      return { stroke: ZONE_FILL[z - 1], width: ZONE_WIDTH[z - 1] };
    };
  }

  const raw =
    colour === "pace"
      ? series.paceMinKm
      : colour === "cadence"
        ? series.cadence
        : series.elevationM;
  const values = smooth(raw, WINDOW);
  const band = range(values);
  if (!band) return () => ({ stroke: "var(--acc)", width: 4 });

  return (i) => {
    const v = values[i];
    if (v == null) return null;
    const t = clamp((v - band.min) / band.span, 0, 1);
    // Faster is a smaller number, so pace ramps the other way round: the heavy
    // accent marks the quick stretches on a pace map, the high ones on
    // elevation, the quick feet on cadence.
    return step(colour === "pace" ? 1 - t : t);
  };
}

/** One quantised rung of the ramp. */
export function step(t: number): Stroke {
  const q = Math.round(clamp(t, 0, 1) * (STEPS - 1)) / (STEPS - 1);
  return {
    // oklab rather than srgb: mixing an accent toward a grey in srgb dips
    // through a dead middle, which is another reason the old ramp read flat.
    stroke: `color-mix(in oklab, var(--acc) ${(14 + q * 86).toFixed(0)}%, var(--mut))`,
    width: 2.6 + q * 3.6,
  };
}

/** Which zone a heart rate falls in, 1-indexed. Below Z1's floor is still Z1. */
export function zoneOf(hr: number, floors: readonly number[]): number {
  let z = 1;
  floors.forEach((floor, i) => {
    if (hr >= floor) z = i + 1;
  });
  return z;
}

/** The 5th to 95th percentile of whatever the column holds. */
export function range(
  values: (number | null)[],
): { min: number; max: number; span: number } | null {
  const present = values.filter((v): v is number => v != null && isFinite(v));
  if (!present.length) return null;
  const sorted = [...present].sort((a, b) => a - b);
  const min = quantile(sorted, CLIP);
  const max = quantile(sorted, 1 - CLIP);
  return { min, max, span: max - min || 1 };
}

function quantile(sorted: number[], p: number): number {
  const at = (sorted.length - 1) * p;
  const lo = Math.floor(at);
  const hi = Math.ceil(at);
  return sorted[lo] + (sorted[hi] - sorted[lo]) * (at - lo);
}

/**
 * Rolling mean over the samples that have a value.
 *
 * A gap stays a gap — a window centred on a null returns null rather than
 * bridging it with the readings either side, which would draw a line through
 * the stretch where the strap fell off.
 */
export function smooth(values: (number | null)[], window: number): (number | null)[] {
  const half = Math.floor(window / 2);
  return values.map((v, i) => {
    if (v == null || !isFinite(v)) return null;
    let sum = 0;
    let n = 0;
    for (let j = Math.max(0, i - half); j <= Math.min(values.length - 1, i + half); j++) {
      const w = values[j];
      if (w == null || !isFinite(w)) continue;
      sum += w;
      n++;
    }
    return n ? sum / n : v;
  });
}

export function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v;
}

/**
 * Consecutive points sharing an appearance, as one run each.
 *
 * A five-hundred-sample route drawn a segment at a time is five hundred
 * elements; grouped, the same route is a few dozen. Runs overlap by one point
 * so there is no seam where the colour changes.
 */
export function group<T extends { i: number }>(
  points: T[],
  style: (index: number) => Stroke | null,
): Array<Stroke & { points: T[] }> {
  const runs: Array<Stroke & { points: T[] }> = [];
  let current: (Stroke & { points: T[] }) | null = null;
  // Tracked alongside rather than read back off `current`: the new run is
  // seeded from the old one's tail, and doing that inline makes the assignment
  // depend on the variable it assigns to.
  let previous: T | null = null;

  for (const p of points) {
    const s = style(p.i);
    if (s == null) {
      current = null;
      previous = null;
      continue;
    }
    if (current === null || current.stroke !== s.stroke || current.width !== s.width) {
      const next = { ...s, points: previous ? [previous, p] : [p] };
      runs.push(next);
      current = next;
    } else {
      current.points.push(p);
    }
    previous = p;
  }
  return runs.filter((r) => r.points.length > 1);
}

/** Which colourings this session has the columns for. */
export function available(series: ActivitySeries): ColourBy[] {
  const has = (v: (number | null)[]) => v.some((x) => x != null);
  const out: ColourBy[] = [];
  if (has(series.hr)) out.push("zone");
  if (has(series.paceMinKm)) out.push("pace");
  if (has(series.elevationM)) out.push("elevation");
  if (has(series.cadence)) out.push("cadence");
  out.push("plain");
  return out;
}

export const COLOUR_LABEL: Record<ColourBy, string> = {
  zone: "HR zone",
  pace: "Pace",
  elevation: "Elevation",
  cadence: "Cadence",
  plain: "Plain",
};

/** Decimal minutes to "5:01". */
export function paceLabel(minPerKm: number): string {
  const m = Math.floor(minPerKm);
  const s = Math.round((minPerKm - m) * 60);
  return s === 60 ? `${m + 1}:00` : `${m}:${String(s).padStart(2, "0")}`;
}
