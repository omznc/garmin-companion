/**
 * Polyline geometry for the SVG charts.
 *
 * The design's charts are all the same shape: a baseline rule and one or two
 * polylines — some over a gradient wash, most not — auto-scaled to whatever
 * the series happens to contain.
 * Real data has gaps — a day with no watch data, a run with no HR — so unlike
 * the design's synthetic series these helpers have to break the line rather
 * than draw through a hole.
 */

export type Point = number | null;

export interface PolyOpts {
  width: number;
  height: number;
  /** Vertical breathing room so peaks don't clip the viewBox. */
  pad?: number;
  /** Fix the scale instead of deriving it, to make two charts comparable. */
  min?: number;
  max?: number;
}

export interface Scale {
  /** Index to horizontal position, in viewBox units. */
  x: (i: number) => number;
  /** Value to vertical position, in viewBox units. */
  y: (v: number) => number;
}

/**
 * The mapping `polylines` draws with, exposed so an overlay — the hover dot and
 * its guide — can land on the line rather than near it. Null when the series
 * has nothing to scale against.
 */
export function scaleFor(values: Point[], opts: PolyOpts): Scale | null {
  const { width, height, pad = 4 } = opts;
  const present = values.filter((v): v is number => v != null && isFinite(v));
  if (present.length === 0) return null;

  const min = opts.min ?? Math.min(...present);
  const max = opts.max ?? Math.max(...present);
  const range = max - min || 1;
  const lastIndex = Math.max(values.length - 1, 1);

  return {
    x: (i) => (i / lastIndex) * width,
    y: (v) => height - pad - ((v - min) / range) * (height - pad * 2),
  };
}

/**
 * One `points` string per unbroken run of data. Render each as its own
 * `<polyline>`; a gap in the input becomes a gap on the chart instead of a
 * straight line across the missing days.
 */
export function polylines(values: Point[], opts: PolyOpts): string[] {
  const scale = scaleFor(values, opts);
  if (!scale) return [];

  const segments: string[] = [];
  let current: string[] = [];

  values.forEach((v, i) => {
    if (v == null || !isFinite(v)) {
      if (current.length) segments.push(current.join(" "));
      current = [];
      return;
    }
    current.push(`${scale.x(i).toFixed(1)},${scale.y(v).toFixed(1)}`);
  });
  if (current.length) segments.push(current.join(" "));

  // A single isolated sample has no line to draw. Give it a hairline so the
  // chart isn't silently blank when only one day has data.
  return segments.map((s) => (s.includes(" ") ? s : `${s} ${s}`));
}

/**
 * One `d` per unbroken run, closed down to the foot of the viewBox — the shape
 * the gradient wash under a line is painted into. Runs of a single sample are
 * dropped: an area with no width paints nothing, and `polylines` already gives
 * that sample a hairline to be seen by.
 */
export function areas(values: Point[], opts: PolyOpts): string[] {
  const scale = scaleFor(values, opts);
  if (!scale) return [];

  const { height } = opts;
  const paths: string[] = [];
  let current: Array<[string, string]> = [];

  const close = () => {
    if (current.length > 1) {
      const first = current[0][0];
      const last = current[current.length - 1][0];
      const line = current.map(([x, y]) => `L${x},${y}`).join(" ");
      paths.push(`M${first},${height} ${line} L${last},${height} Z`);
    }
    current = [];
  };

  values.forEach((v, i) => {
    if (v == null || !isFinite(v)) {
      close();
      return;
    }
    current.push([scale.x(i).toFixed(1), scale.y(v).toFixed(1)]);
  });
  close();

  return paths;
}

/** Convenience for the common case of one continuous series. */
export function polyline(values: Point[], opts: PolyOpts): string {
  return polylines(values, opts).join(" ");
}

export const hasData = (values: Point[]) =>
  values.some((v) => v != null && isFinite(v));

/** Centred rolling mean, for the trend line drawn under a noisy series. */
export function smooth(values: Point[], window = 7): Point[] {
  const half = Math.floor(window / 2);
  return values.map((_, i) => {
    let sum = 0;
    let n = 0;
    for (let k = -half; k <= half; k++) {
      const v = values[i + k];
      if (v != null && isFinite(v)) {
        sum += v;
        n++;
      }
    }
    return n ? sum / n : null;
  });
}

export function mean(values: Point[]): number | null {
  const present = values.filter((v): v is number => v != null && isFinite(v));
  if (!present.length) return null;
  return present.reduce((a, b) => a + b, 0) / present.length;
}

/**
 * Pearson correlation over the pairs where both series have a value.
 * Returns null under `minPairs` — a correlation from four points is noise
 * dressed as a finding, and the Insights screen shows its own basis count.
 */
export function correlation(
  xs: Point[],
  ys: Point[],
  minPairs = 8,
): { r: number; n: number } | null {
  const pairs: Array<[number, number]> = [];
  for (let i = 0; i < Math.min(xs.length, ys.length); i++) {
    const x = xs[i];
    const y = ys[i];
    if (x != null && y != null && isFinite(x) && isFinite(y)) pairs.push([x, y]);
  }
  if (pairs.length < minPairs) return null;

  const n = pairs.length;
  const mx = pairs.reduce((a, p) => a + p[0], 0) / n;
  const my = pairs.reduce((a, p) => a + p[1], 0) / n;
  let num = 0;
  let dx = 0;
  let dy = 0;
  for (const [x, y] of pairs) {
    num += (x - mx) * (y - my);
    dx += (x - mx) ** 2;
    dy += (y - my) ** 2;
  }
  const den = Math.sqrt(dx * dy);
  if (den === 0) return null;
  return { r: num / den, n };
}
