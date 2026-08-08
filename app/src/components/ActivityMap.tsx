/**
 * The route, on a map.
 *
 * What identifies a route to the person who ran it is its shape, its length,
 * and where on it the effort went. The shape alone was drawn here before, on
 * the argument that a photograph of the streets underneath adds nothing — but
 * it does: it is the difference between "a loop" and "the loop round the lake",
 * and between "a climb at 4 km" and "the road out of town". So the streets are
 * under it now, from OpenStreetMap, behind a toggle, and everything still works
 * with the network off because the drawing was always the part that mattered.
 *
 * The trace is coloured *and weighted* by whichever metric is being read, so
 * the map answers "where did this go wrong" rather than only "where did this
 * go". Colouring by zone uses the same ladder as the bar on the rest of the
 * screen; a red here and a red there are the same red for the same reason.
 *
 * Geography is kept honest — Web Mercator, one scale on both axes, no stretch
 * to fill the box. That is also what lets the tiles line up with the trace,
 * since Mercator is the projection they are cut in.
 */
import { useMemo, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import type { ActivitySeries, Highlight, ZoneProfile } from "../lib/api";
import { ZONE_FILL } from "./ZoneBar";
import { ElevationProfile } from "./ElevationProfile";
import { duration } from "../lib/format";
import { useTheme } from "../lib/useTheme";
import { mercator, tilesFor, TILE_CREDIT, type Frame } from "../lib/tiles";
import {
  available,
  COLOUR_LABEL,
  group,
  paceLabel,
  range,
  step,
  styleFor,
  type ColourBy,
} from "../lib/trace";

/** viewBox width. The height follows from the route's own proportions. */
const W = 1000;

/**
 * Bounds on how tall the drawing may get relative to its width.
 *
 * A dead-straight north–south route has an aspect ratio in the tens, and
 * honouring it literally would produce a hairline a metre long. Past these the
 * box letterboxes — the trace stays the shape it is and simply doesn't fill the
 * width, which is the honest way to run out of room.
 */
const MAX_ASPECT = 1.1;
const MIN_ASPECT = 0.34;

/** Inset in viewBox units, so the stroke and the end caps aren't clipped. */
const PAD = 34;

/** Rendered height ceiling. A tall route narrows rather than growing the page. */
const MAX_HEIGHT = 460;

interface Projected {
  x: number;
  y: number;
  /** Index back into the original series, for the readout and the pins. */
  i: number;
}

export function ActivityMap({
  series,
  zones,
  highlights = [],
  hover,
  onHover,
}: {
  series: ActivitySeries;
  zones: ZoneProfile;
  highlights?: Highlight[];
  /** Sample index highlighted from elsewhere on the screen — a chart hover. */
  hover?: number | null;
  onHover?: (index: number | null) => void;
}) {
  const options = available(series);
  const [colour, setColour] = useState<ColourBy>(() => options[0] ?? "plain");
  const [basemap, setBasemap] = useState(true);
  const { mode } = useTheme();

  /**
   * Which of the two drawings the pointer is actually in, and where.
   *
   * The mark is shared — pointing at a moment on the map should put the guide
   * on the profile, and the other way round. The *card* is not: two cards for
   * one pointer is one of them answering a question nobody asked. So the
   * reading belongs to whichever drawing is under the pointer, and null when
   * the index came from a chart further down the page instead.
   */
  const [owned, setOwned] = useState<{ at: number; by: "map" | "profile" } | null>(null);
  const at = owned?.at ?? hover ?? null;

  const geometry = useMemo(() => project(series), [series]);
  const tiles = useMemo(() => (geometry ? tilesFor(geometry.frame, mode) : []), [geometry, mode]);
  // Tracked per tile rather than as one flag: a single tile failing is normal,
  // and only all of them failing means there is no network to draw a map with.
  // The keys carry the zoom, so a different route can't inherit these.
  const [broken, setBroken] = useState<ReadonlySet<string>>(() => new Set());
  const failed = tiles.length > 0 && tiles.every((t) => broken.has(t.key));

  const active = options.includes(colour) ? colour : "plain";

  if (!geometry) {
    return <Indoor />;
  }
  const { points, height, byIndex } = geometry;
  const showTiles = basemap && tiles.length > 0 && !failed;

  const runs = group(points, styleFor(series, zones, active));
  const marks = kilometreMarks(series, byIndex);
  const pins = highlightPins(series, byIndex, highlights);

  const track = (e: ReactPointerEvent<SVGSVGElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    if (!rect.width || !rect.height) return;
    // Client space to viewBox space. The two share an aspect ratio because the
    // wrapper is sized from the viewBox, so this is one linear step per axis.
    const vx = ((e.clientX - rect.left) / rect.width) * W;
    const vy = ((e.clientY - rect.top) / rect.height) * height;

    let nearest: Projected | null = null;
    let best = Infinity;
    for (const p of points) {
      const d = (p.x - vx) ** 2 + (p.y - vy) ** 2;
      if (d < best) {
        best = d;
        nearest = p;
      }
    }
    // Only when the pointer is actually near the line. Snapping from the far
    // corner of the box would report a reading nobody pointed at.
    const within = best <= (W * 0.08) ** 2;
    const next = within && nearest ? nearest.i : null;
    setOwned(next == null ? null : { at: next, by: "map" });
    onHover?.(next);
  };

  const leave = () => {
    setOwned(null);
    onHover?.(null);
  };

  const cursor = at != null ? byIndex.get(at) : undefined;
  const start = points[0];
  const finish = points[points.length - 1];

  return (
    <div>
      <div className="section-head" style={{ marginBottom: 12 }}>
        <div className="eyebrow">Route</div>
        <div style={{ display: "flex", gap: 16, alignItems: "baseline", flexWrap: "wrap" }}>
          {options.length > 1 &&
            options.map((key) => (
              <Tab
                key={key}
                label={COLOUR_LABEL[key]}
                on={key === active}
                onClick={() => setColour(key)}
              />
            ))}
          {/* Set apart from the colourings: those pick what the line says, this
              picks whether there is a world behind it. */}
          {tiles.length > 0 && (
            <Tab label="Map" on={showTiles} onClick={() => setBasemap((v) => !v)} muted />
          )}
        </div>
      </div>

      <div
        style={{
          position: "relative",
          // Sized from the viewBox rather than capped afterwards. A max-height
          // on the SVG itself letterboxes the drawing inside a box that is
          // still full width, and then every pointer position is read against
          // the wrong rectangle.
          width: "100%",
          maxWidth: (MAX_HEIGHT * W) / height,
          aspectRatio: `${W} / ${height}`,
          marginInline: "auto",
        }}
      >
        <svg
          viewBox={`0 0 ${W} ${height}`}
          preserveAspectRatio="xMidYMid meet"
          style={{
            width: "100%",
            height: "100%",
            display: "block",
            touchAction: "none",
            borderRadius: 8,
            background: showTiles ? "var(--line2)" : undefined,
          }}
          onPointerMove={track}
          onPointerLeave={leave}
          role="img"
          aria-label="Route of this activity"
        >
          {showTiles && (
            <g style={{ opacity: mode === "dark" ? 0.72 : 0.82 }}>
              {tiles.map((t) => (
                <image
                  key={t.key}
                  href={t.href}
                  x={t.x}
                  y={t.y}
                  // Half a unit of bleed, or the seams between tiles show as
                  // hairlines wherever the scale lands off a whole pixel.
                  width={t.size + 0.5}
                  height={t.size + 0.5}
                  preserveAspectRatio="none"
                  onError={() => setBroken((s) => new Set(s).add(t.key))}
                />
              ))}
            </g>
          )}

          {/* Under the coloured trace: a halo in the page colour, so the line
              separates from whatever it is drawn over, and one continuous
              hairline, so a stretch the metric has no value for still reads as
              part of the route rather than as a gap in it. */}
          {showTiles && (
            <polyline
              points={points.map((p) => `${p.x},${p.y}`).join(" ")}
              fill="none"
              stroke="var(--bg)"
              strokeWidth={9}
              strokeOpacity={0.75}
              strokeLinejoin="round"
              strokeLinecap="round"
              vectorEffect="non-scaling-stroke"
            />
          )}
          <polyline
            points={points.map((p) => `${p.x},${p.y}`).join(" ")}
            fill="none"
            stroke="var(--line)"
            strokeWidth={5}
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />
          {runs.map((run, i) => (
            <polyline
              key={i}
              points={run.points.map((p) => `${p.x},${p.y}`).join(" ")}
              fill="none"
              stroke={run.stroke}
              strokeWidth={run.width}
              strokeLinejoin="round"
              strokeLinecap="round"
              vectorEffect="non-scaling-stroke"
            />
          ))}

          {marks.map((m) => (
            <g key={m.km}>
              <circle
                cx={m.x}
                cy={m.y}
                r={3}
                fill="var(--bg)"
                stroke="var(--mut)"
                strokeWidth={1.5}
                vectorEffect="non-scaling-stroke"
              />
              <text
                x={m.x + 8}
                y={m.y + 4}
                fill="var(--faint)"
                style={{ fontSize: 20, fontFamily: "var(--mono, monospace)" }}
                stroke="var(--bg)"
                strokeWidth={3}
                paintOrder="stroke"
              >
                {m.km}
              </text>
            </g>
          ))}

          {/* Where something worth saying happened. Drawn over the kilometre
              marks because a pin is the reason to look at the map at all. */}
          {pins.map((p, i) => (
            <circle
              key={i}
              cx={p.x}
              cy={p.y}
              r={5}
              fill={p.tone === "good" ? "var(--fg)" : "var(--acc)"}
              stroke="var(--bg)"
              strokeWidth={2}
              vectorEffect="non-scaling-stroke"
            >
              <title>{p.title}</title>
            </circle>
          ))}

          {/* Hollow start, filled finish — the same convention a lap chart
              uses, and it survives a route that ends where it began. */}
          <circle
            cx={start.x}
            cy={start.y}
            r={6}
            fill="var(--bg)"
            stroke="var(--fg)"
            strokeWidth={2}
            vectorEffect="non-scaling-stroke"
          />
          <circle
            cx={finish.x}
            cy={finish.y}
            r={5}
            fill="var(--fg)"
            stroke="var(--bg)"
            strokeWidth={1.5}
            vectorEffect="non-scaling-stroke"
          />

          {cursor && (
            <circle
              cx={cursor.x}
              cy={cursor.y}
              r={6}
              fill="var(--acc)"
              stroke="var(--bg)"
              strokeWidth={2}
              vectorEffect="non-scaling-stroke"
            />
          )}
        </svg>

        {/* The dot above marks the moment wherever it came from; the card only
            appears for a pointer that is actually in this box. */}
        {cursor && owned?.by === "map" && (
          <Readout series={series} index={cursor.i} x={cursor.x} y={cursor.y} height={height} />
        )}
      </div>

      <div className="section-head">
        <Legend colour={active} zones={zones} series={series} />
        {basemap && tiles.length > 0 && (
          <span style={{ fontSize: "var(--fs-micro)", color: "var(--faint)", marginTop: 12 }}>
            {failed
              ? "Map tiles couldn't be fetched — the route is drawn without them."
              : TILE_CREDIT}
          </span>
        )}
      </div>

      <ElevationProfile
        series={series}
        zones={zones}
        colour={active}
        hover={at}
        reading={owned?.by === "profile"}
        onHover={(i) => {
          setOwned(i == null ? null : { at: i, by: "profile" });
          onHover?.(i);
        }}
      />
    </div>
  );
}

/** Plain text, no boxes — the same restraint the rest of the app shows. */
function Tab({
  label,
  on,
  onClick,
  muted = false,
}: {
  label: string;
  on: boolean;
  onClick: () => void;
  muted?: boolean;
}) {
  return (
    <button
      aria-pressed={on}
      onClick={onClick}
      style={{
        fontSize: "var(--fs-caption)",
        cursor: "pointer",
        color: on ? "var(--fg)" : "var(--faint)",
        borderBottom: `1px solid ${on ? (muted ? "var(--mut)" : "var(--acc)") : "transparent"}`,
        paddingBottom: 3,
        transition: "color var(--dur-base)",
      }}
    >
      {label}
    </button>
  );
}

/* ------------------------------------------------------------ projection --- */

interface Geometry {
  points: Projected[];
  height: number;
  byIndex: Map<number, Projected>;
  frame: Frame;
}

/**
 * Latitude and longitude to viewBox coordinates, through Web Mercator.
 *
 * One scale drives both axes, so the route keeps the proportions it has on the
 * ground rather than whatever fills the box — and so the tile grid, which is
 * square in this same projection, lands square on top of it.
 */
function project(series: ActivitySeries): Geometry | null {
  const raw: Projected[] = [];
  for (let i = 0; i < series.lat.length; i++) {
    const lat = series.lat[i];
    const lon = series.lon[i];
    if (lat == null || lon == null) continue;
    const { x, y } = mercator(lat, lon);
    raw.push({ x, y, i });
  }
  if (raw.length < 2) return null;

  const xs = raw.map((p) => p.x);
  const ys = raw.map((p) => p.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);

  // A floor on the span as well as a guard against zero: a session that never
  // left a car park would otherwise zoom to a level with no tiles at it.
  const spanX = Math.max(maxX - minX, 2e-7);
  const spanY = Math.max(maxY - minY, 2e-7);

  // The drawing is as tall as the route is, relative to how wide it is, within
  // the bounds above. Past them it letterboxes rather than distorting.
  const aspect = Math.min(Math.max(spanY / spanX, MIN_ASPECT), MAX_ASPECT);
  const height = W * aspect;

  const scale = Math.min((W - PAD * 2) / spanX, (height - PAD * 2) / spanY);
  const offsetX = (W - spanX * scale) / 2;
  const offsetY = (height - spanY * scale) / 2;

  const points = raw.map((p) => ({
    x: offsetX + (p.x - minX) * scale,
    y: offsetY + (p.y - minY) * scale,
    i: p.i,
  }));

  return {
    points,
    height,
    byIndex: new Map(points.map((p) => [p.i, p])),
    frame: { scale, minX, minY, offsetX, offsetY, width: W, height },
  };
}

/* ----------------------------------------------------------------- marks --- */

/** Where each whole kilometre fell, from the cumulative distance column. */
function kilometreMarks(series: ActivitySeries, byIndex: Map<number, Projected>) {
  const out: Array<{ km: number; x: number; y: number }> = [];
  let next = 1000;

  for (let i = 0; i < series.distanceM.length; i++) {
    const d = series.distanceM[i];
    if (d == null) continue;
    while (d >= next) {
      const p = byIndex.get(i);
      if (p) out.push({ km: next / 1000, x: p.x, y: p.y });
      next += 1000;
    }
    // A route long enough to carry more than a dozen marks is better read
    // without them; past this they are a dotted line, not a scale.
    if (out.length >= 12) break;
  }
  return out;
}

/** Highlights that happened somewhere in particular, placed on the trace. */
function highlightPins(
  series: ActivitySeries,
  byIndex: Map<number, Projected>,
  highlights: Highlight[],
) {
  const out: Array<{ x: number; y: number; tone: string; title: string }> = [];

  for (const h of highlights) {
    if (h.atS == null) continue;
    const i = nearestIndex(series.elapsedS, h.atS);
    if (i == null) continue;
    const p = byIndex.get(i);
    if (p) out.push({ x: p.x, y: p.y, tone: h.tone, title: h.title });
  }
  return out;
}

/** The sample taken closest to a given elapsed time. */
function nearestIndex(elapsed: (number | null)[], at: number): number | null {
  let best: number | null = null;
  let distance = Infinity;
  for (let i = 0; i < elapsed.length; i++) {
    const t = elapsed[i];
    if (t == null) continue;
    const d = Math.abs(t - at);
    if (d < distance) {
      distance = d;
      best = i;
    }
  }
  return best;
}

/* -------------------------------------------------------------- readouts --- */

function Readout({
  series,
  index,
  x,
  y,
  height,
}: {
  series: ActivitySeries;
  index: number;
  x: number;
  y: number;
  height: number;
}) {
  const rows: Array<[string, string]> = [];
  const t = series.elapsedS[index];
  const hr = series.hr[index];
  const pace = series.paceMinKm[index];
  const cadence = series.cadence[index];
  const elevation = series.elevationM[index];
  const d = series.distanceM[index];

  if (hr != null) rows.push(["HR", `${hr.toFixed(0)} bpm`]);
  if (pace != null) rows.push(["Pace", `${paceLabel(pace)} /km`]);
  if (cadence != null) rows.push(["Cadence", `${cadence.toFixed(0)} spm`]);
  if (elevation != null) rows.push(["Elevation", `${elevation.toFixed(0)} m`]);
  if (d != null) rows.push(["At", `${(d / 1000).toFixed(2)} km`]);
  if (!rows.length) return null;

  // Anchored to the point rather than the pointer, and flipped near an edge so
  // the card never straddles the boundary of the box it sits in.
  const left = (x / W) * 100;
  const top = (y / height) * 100;
  const anchor = left < 18 ? "0" : left > 82 ? "-100%" : "-50%";
  // Above the point, unless the point is near the top of the box and there is
  // no room up there.
  const lift = top < 26 ? "16px" : "calc(-100% - 16px)";

  return (
    <div
      className="chart-tip"
      style={{
        left: `${left}%`,
        top: `${top}%`,
        // The shared class pins the card above a chart with `bottom`. Left set,
        // it would apply here alongside the `top` above, over-constrain the box
        // and collapse it to no height at all — which is what used to leave the
        // text sitting outside its own card.
        bottom: "auto",
        transform: `translate(${anchor}, ${lift})`,
      }}
    >
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

const ZONE_LABELS = ["Z1", "Z2", "Z3", "Z4", "Z5"];

function Legend({
  colour,
  zones,
  series,
}: {
  colour: ColourBy;
  zones: ZoneProfile;
  series: ActivitySeries;
}) {
  const style = {
    display: "flex",
    gap: 16,
    marginTop: 12,
    fontSize: "var(--fs-caption)",
    color: "var(--mut)",
    flexWrap: "wrap" as const,
    alignItems: "center",
  };

  if (colour === "zone") {
    // Only the zones this session actually visited — five swatches under a run
    // that never left Z2 is a legend for somebody else's session.
    const visited = zones.percent.map((p, i) => ({ p, i })).filter(({ p }) => p >= 1);
    return (
      <div style={style}>
        {visited.map(({ i, p }) => (
          <span key={i} style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
            <span
              style={{
                width: 14,
                height: 2 + i,
                borderRadius: 2,
                background: ZONE_FILL[i],
                display: "inline-block",
              }}
            />
            {ZONE_LABELS[i]} {p.toFixed(0)}%
          </span>
        ))}
      </div>
    );
  }

  if (colour === "plain") return <div />;

  const values =
    colour === "pace"
      ? series.paceMinKm
      : colour === "cadence"
        ? series.cadence
        : series.elevationM;
  const band = range(values);
  if (!band) return <div />;
  const label = (v: number) =>
    colour === "pace"
      ? `${paceLabel(v)} /km`
      : colour === "cadence"
        ? `${v.toFixed(0)} spm`
        : `${v.toFixed(0)} m`;

  // The ramp is drawn as the rungs it actually has, weight and all, rather
  // than as a smooth gradient that promises a resolution the trace doesn't use.
  const rungs = Array.from({ length: 8 }, (_, i) => step(i / 7));

  return (
    <div style={style}>
      <span className="mono">{label(colour === "pace" ? band.max : band.min)}</span>
      <span style={{ display: "inline-flex", alignItems: "center", gap: 2 }}>
        {rungs.map((r, i) => (
          <span
            key={i}
            style={{
              width: 11,
              height: r.width,
              borderRadius: 2,
              background: r.stroke,
              display: "inline-block",
            }}
          />
        ))}
      </span>
      <span className="mono">{label(colour === "pace" ? band.min : band.max)}</span>
      <span style={{ color: "var(--faint)" }}>
        {colour === "pace"
          ? "quicker to the right"
          : colour === "cadence"
            ? "quicker feet to the right"
            : "higher to the right"}
      </span>
    </div>
  );
}

/**
 * What a session with no coordinates gets.
 *
 * An empty box would read as a failure to load. This says which of the two it
 * is, and — since the athlete's runs are almost all treadmill — what the
 * missing trace costs them beyond the picture.
 */
function Indoor() {
  return (
    <div>
      <div className="eyebrow" style={{ marginBottom: 12 }}>
        Route
      </div>
      <p
        style={{
          fontSize: "var(--fs-base)",
          lineHeight: 1.7,
          color: "var(--mut)",
          margin: 0,
          maxWidth: "58ch",
          textWrap: "pretty",
        }}
      >
        No position was recorded for this session, so there's no route to draw — a treadmill, a
        rower or a strength session never had one. It also means this can't contribute to VO2 max,
        which Garmin only calculates from outdoor runs.
      </p>
    </div>
  );
}
