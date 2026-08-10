/**
 * The route's shape, and nothing else.
 *
 * `ActivityMap` is the real one — streets underneath, the trace coloured by
 * whichever metric is being read, kilometre marks, a hover readout. None of
 * that belongs on a share card. The tiles are the clearest case: they're
 * fetched from CARTO over the network, and the rasteriser inlines what it draws
 * from the document, so a card built on them would either wait on the network
 * or come out with holes where the map was. The shape is the part that survives
 * being small anyway — at 200px a basemap is texture, while "this was the loop
 * round the lake" still reads.
 *
 * Same Web Mercator as the map, for the same reason: one scale on both axes, so
 * the route keeps the proportions it had on the ground instead of being
 * stretched to fill the box.
 */
import type { ActivitySeries } from "../lib/api";
import { mercator } from "../lib/tiles";

/** viewBox width. Height comes from the route's own proportions. */
const W = 1000;

/** Inset in viewBox units, so the stroke and its caps aren't clipped. */
const PAD = 26;

/** How far from square the drawing may get before it letterboxes instead. */
const MAX_ASPECT = 1.1;
const MIN_ASPECT = 0.34;

/**
 * Whether a card should offer a route at all.
 *
 * Exported so the screen can decide what else to put on the card before it
 * builds one — a treadmill session has `lat` and `lon` columns full of nulls
 * rather than no columns, so the presence of the series says nothing.
 */
export function hasRoute(series: ActivitySeries | undefined | null): boolean {
  if (!series) return false;
  let found = 0;
  for (let i = 0; i < series.lat.length; i++) {
    if (series.lat[i] != null && series.lon[i] != null && ++found >= 2) return true;
  }
  return false;
}

export function RouteTrace({ series, height }: { series: ActivitySeries; height: number }) {
  const raw: { x: number; y: number }[] = [];
  for (let i = 0; i < series.lat.length; i++) {
    const lat = series.lat[i];
    const lon = series.lon[i];
    if (lat == null || lon == null) continue;
    raw.push(mercator(lat, lon));
  }
  if (raw.length < 2) return null;

  const xs = raw.map((p) => p.x);
  const ys = raw.map((p) => p.y);
  const minX = Math.min(...xs);
  const minY = Math.min(...ys);
  // Floored rather than merely guarded against zero: a session that never left
  // the car park would otherwise scale its own GPS jitter up to fill the frame
  // and draw a scribble that looks like a route.
  const spanX = Math.max(Math.max(...xs) - minX, 2e-7);
  const spanY = Math.max(Math.max(...ys) - minY, 2e-7);

  // The viewBox takes the route's own proportions, within the same bounds the
  // map uses — a dead-straight north–south route has an aspect ratio in the
  // tens, and honouring it literally draws a hairline. Past the bounds the box
  // letterboxes: the trace stays the shape it is and doesn't fill the width.
  //
  // `meet` then fits that box into whatever slot the card gave it, so nothing
  // here needs to know how wide the card is.
  const boxH = W * Math.min(Math.max(spanY / spanX, MIN_ASPECT), MAX_ASPECT);

  const scale = Math.min((W - PAD * 2) / spanX, (boxH - PAD * 2) / spanY);
  const offsetX = (W - spanX * scale) / 2;
  const offsetY = (boxH - spanY * scale) / 2;

  const points = raw
    .map((p) => `${offsetX + (p.x - minX) * scale},${offsetY + (p.y - minY) * scale}`)
    .join(" ");

  const ends = [raw[0], raw[raw.length - 1]].map((p) => ({
    cx: offsetX + (p.x - minX) * scale,
    cy: offsetY + (p.y - minY) * scale,
  }));

  return (
    <svg
      viewBox={`0 0 ${W} ${boxH}`}
      // An explicit pixel height, handed down by the card, rather than the
      // `100%` this used to carry. The rasteriser lays its clone out in a
      // detached `foreignObject`, where a percentage height resolves against a
      // parent that isn't definite there — the route came out taller than its
      // band and pushed the zone legend off the bottom of the card, while the
      // live DOM measured as fitting and the fit pass saw nothing to do.
      width="100%"
      height={height}
      fill="none"
      // The route is the shape it is; letterboxing is handled above, so the
      // viewBox must not be stretched to the element.
      preserveAspectRatio="xMidYMid meet"
      aria-hidden="true"
    >
      {/* A hairline under the accent line, so a route that doubles back on
          itself still reads as two passes rather than one thick stroke. */}
      <polyline
        points={points}
        stroke="var(--line)"
        strokeWidth={9}
        strokeLinejoin="round"
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
      />
      <polyline
        points={points}
        stroke="var(--acc)"
        strokeWidth={4}
        strokeLinejoin="round"
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
      />
      {/* Start and finish. On a loop they land on top of each other, which is
          itself the correct picture of a loop. */}
      {ends.map((e, i) => (
        <circle
          key={i}
          cx={e.cx}
          cy={e.cy}
          r={7}
          fill={i === 0 ? "var(--bg)" : "var(--acc)"}
          stroke="var(--acc)"
          strokeWidth={3}
          vectorEffect="non-scaling-stroke"
        />
      ))}
    </svg>
  );
}
