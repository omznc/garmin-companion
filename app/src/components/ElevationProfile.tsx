/**
 * The climb, laid out flat.
 *
 * The map says where the route went; this says what it cost. Plotted against
 * distance rather than time on purpose — a hill is a feature of the ground, and
 * against time it would stretch out exactly where you slowed down for it, which
 * is the one place you don't want the shape to lie.
 *
 * Filled with the same colouring the trace above uses, so the metric picked up
 * there is answered here too: where in the climb the heart rate went, where the
 * pace went, all against the profile that explains both. Hover is shared with
 * the map and the charts, so pointing at a moment marks it everywhere.
 */
import { useMemo } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import type { ActivitySeries, ZoneProfile } from "../lib/api";
import { duration } from "../lib/format";
import { group, paceLabel, smooth, styleFor, type ColourBy } from "../lib/trace";

const W = 1000;
const H = 168;
/** Vertical breathing room, so the ridge and the floor aren't on the frame. */
const PAD = 12;

/**
 * A barometric altimeter wanders by a metre or so at rest, and summing every
 * positive wobble over an hour invents a few hundred metres of climbing that
 * never happened. Smoothed first, then only rises past this count.
 */
const NOISE_M = 0.4;
const SMOOTH = 9;

interface Plotted {
  x: number;
  y: number;
  i: number;
}

export function ElevationProfile({
  series,
  zones,
  colour,
  hover,
  reading,
  onHover,
}: {
  series: ActivitySeries;
  zones: ZoneProfile;
  colour: ColourBy;
  hover: number | null;
  /**
   * Whether the pointer is in this chart rather than on the map above.
   *
   * The mark is drawn either way — a moment pointed at on the map belongs on
   * the profile too. The card is not: the owner of the pointer is the only one
   * that should answer, or hovering once puts two cards on the screen.
   */
  reading: boolean;
  onHover: (index: number | null) => void;
}) {
  const plot = useMemo(() => layout(series), [series]);

  if (!plot) return null;
  const { points, min, max, gain, loss, span, unit, byIndex } = plot;

  const runs = group(points, styleFor(series, zones, colour));
  const cursor = hover != null ? byIndex.get(hover) : undefined;

  const track = (e: ReactPointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    if (!rect.width) return;
    const vx = ((e.clientX - rect.left) / rect.width) * W;
    let nearest: Plotted | null = null;
    let best = Infinity;
    for (const p of points) {
      const d = Math.abs(p.x - vx);
      if (d < best) {
        best = d;
        nearest = p;
      }
    }
    onHover(nearest ? nearest.i : null);
  };

  const leave = () => {
    // Only clears what this chart set. Leaving the profile shouldn't wipe a
    // mark the map or a chart is still holding under its own pointer.
    if (reading) onHover(null);
  };

  return (
    <div style={{ marginTop: 34 }}>
      <div className="section-head" style={{ marginBottom: 10 }}>
        <div className="eyebrow">Elevation</div>
        <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)" }} className="mono">
          ↑ {gain.toFixed(0)} m · ↓ {loss.toFixed(0)} m · {min.toFixed(0)}–{max.toFixed(0)} m
        </div>
      </div>

      <div
        style={{ position: "relative", touchAction: "none" }}
        onPointerMove={track}
        onPointerLeave={leave}
      >
        <svg
          viewBox={`0 0 ${W} ${H}`}
          preserveAspectRatio="none"
          style={{ width: "100%", height: H, display: "block" }}
          aria-hidden
        >
          {/* Every fill first, so a ridge line is never dimmed by the wash of
              the segment next to it. */}
          {runs.map((run, i) => (
            <polygon
              key={`f${i}`}
              points={[
                `${run.points[0].x},${H}`,
                ...run.points.map((p) => `${p.x},${p.y}`),
                `${run.points[run.points.length - 1].x},${H}`,
              ].join(" ")}
              fill={run.stroke}
              fillOpacity={0.28}
              stroke="none"
            />
          ))}
          {runs.map((run, i) => (
            <polyline
              key={`l${i}`}
              points={run.points.map((p) => `${p.x},${p.y}`).join(" ")}
              fill="none"
              stroke={run.stroke}
              strokeWidth={run.width * 0.6}
              strokeLinejoin="round"
              strokeLinecap="round"
              vectorEffect="non-scaling-stroke"
            />
          ))}
          <line
            x1={0}
            y1={H - 0.5}
            x2={W}
            y2={H - 0.5}
            stroke="var(--line2)"
            strokeWidth={1}
            vectorEffect="non-scaling-stroke"
          />
        </svg>

        {/* HTML rather than SVG: the viewBox is stretched horizontally, and a
            circle drawn inside it would come out an ellipse and the labels
            would come out squashed with it. */}
        {cursor && (
          <>
            <div className="chart-guide" style={{ left: `${(cursor.x / W) * 100}%` }} />
            <div
              className="chart-dot"
              style={{
                left: `${(cursor.x / W) * 100}%`,
                top: `${(cursor.y / H) * 100}%`,
                background: "var(--acc)",
              }}
            />
            {reading && <Tip series={series} index={cursor.i} left={(cursor.x / W) * 100} />}
          </>
        )}
      </div>

      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          marginTop: 6,
          fontSize: "var(--fs-micro)",
          color: "var(--faint)",
        }}
        className="mono"
      >
        <span>0</span>
        <span>{unit === "km" ? `${(span / 2000).toFixed(1)} km` : duration(span / 2)}</span>
        <span>{unit === "km" ? `${(span / 1000).toFixed(1)} km` : duration(span)}</span>
      </div>
    </div>
  );
}

/* ----------------------------------------------------------------- shape --- */

interface Layout {
  points: Plotted[];
  byIndex: Map<number, Plotted>;
  min: number;
  max: number;
  gain: number;
  loss: number;
  /** Length of the horizontal axis, in metres or in seconds. */
  span: number;
  unit: "km" | "time";
}

function layout(series: ActivitySeries): Layout | null {
  const elevation = smooth(series.elevationM, SMOOTH);
  // Distance where there is one — a treadmill-style series with no positions
  // can still have both, and an outdoor pause is a spot on the profile you
  // want to be one point wide rather than a plateau the length of the rest.
  const along = series.distanceM.some((v) => v != null) ? series.distanceM : series.elapsedS;
  const unit: "km" | "time" = along === series.distanceM ? "km" : "time";

  const raw: Array<{ at: number; v: number; i: number }> = [];
  for (let i = 0; i < elevation.length; i++) {
    const v = elevation[i];
    const at = along[i];
    if (v == null || at == null || !isFinite(v) || !isFinite(at)) continue;
    raw.push({ at, v, i });
  }
  if (raw.length < 2) return null;

  const first = raw[0].at;
  const span = raw[raw.length - 1].at - first;
  if (!(span > 0)) return null;

  const values = raw.map((p) => p.v);
  const min = Math.min(...values);
  const max = Math.max(...values);
  // A flat course has no range to scale against; give it one so the line sits
  // in the middle of the box rather than jittering across the whole of it.
  const range = Math.max(max - min, 10);

  let gain = 0;
  let loss = 0;
  for (let i = 1; i < raw.length; i++) {
    const d = raw[i].v - raw[i - 1].v;
    if (d > NOISE_M) gain += d;
    else if (d < -NOISE_M) loss -= d;
  }

  const points: Plotted[] = raw.map((p) => ({
    x: ((p.at - first) / span) * W,
    y: H - PAD - ((p.v - min) / range) * (H - PAD * 2),
    i: p.i,
  }));

  return {
    points,
    byIndex: new Map(points.map((p) => [p.i, p])),
    min,
    max,
    gain,
    loss,
    span,
    unit,
  };
}

/* ------------------------------------------------------------------- tip --- */

function Tip({ series, index, left }: { series: ActivitySeries; index: number; left: number }) {
  const rows: Array<[string, string]> = [];
  const elevation = series.elevationM[index];
  const grade = gradeAt(series, index);
  const hr = series.hr[index];
  const pace = series.paceMinKm[index];
  const d = series.distanceM[index];
  const t = series.elapsedS[index];

  if (elevation != null) rows.push(["Elevation", `${elevation.toFixed(0)} m`]);
  if (grade != null) rows.push(["Grade", `${grade > 0 ? "+" : ""}${grade.toFixed(1)}%`]);
  if (hr != null) rows.push(["HR", `${hr.toFixed(0)} bpm`]);
  if (pace != null) rows.push(["Pace", `${paceLabel(pace)} /km`]);
  if (d != null) rows.push(["At", `${(d / 1000).toFixed(2)} km`]);
  if (!rows.length) return null;

  const anchor = left < 12 ? "0" : left > 88 ? "-100%" : "-50%";

  return (
    <div className="chart-tip" style={{ left: `${left}%`, transform: `translateX(${anchor})` }}>
      {t != null && <div className="chart-tip-when">{duration(t)} in</div>}
      {rows.map(([key, value]) => (
        <div key={key} className="chart-tip-row">
          <span className="chart-tip-key">{key}</span>
          <span className="mono">{value}</span>
        </div>
      ))}
    </div>
  );
}

/**
 * Rise over run around a sample, as a percentage.
 *
 * Read over a window rather than between neighbours: consecutive samples are a
 * few metres apart, and a half-metre of altimeter noise across three metres of
 * ground reads as a 16% wall.
 */
function gradeAt(series: ActivitySeries, index: number): number | null {
  const WINDOW = 8;
  const lo = Math.max(0, index - WINDOW);
  const hi = Math.min(series.elevationM.length - 1, index + WINDOW);
  const rise = pick(series.elevationM, lo, hi);
  const run = pick(series.distanceM, lo, hi);
  if (!rise || !run) return null;
  const along = run.b - run.a;
  if (along < 12) return null;
  return ((rise.b - rise.a) / along) * 100;
}

/** The first and last values present in a window, if there are two. */
function pick(values: (number | null)[], lo: number, hi: number): { a: number; b: number } | null {
  let a: number | null = null;
  let b: number | null = null;
  for (let i = lo; i <= hi; i++) {
    const v = values[i];
    if (v == null || !isFinite(v)) continue;
    if (a == null) a = v;
    b = v;
  }
  return a != null && b != null ? { a, b } : null;
}
