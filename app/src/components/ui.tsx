/** The primitives every screen is built from. */
import { useEffect, useId, useState } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { areas, polylines, scaleFor, hasData, type Point } from "../lib/chart";
import { tokens } from "../lib/customTheme";
import { IS_MOBILE } from "../lib/platform";
import type { CustomTheme } from "../lib/api";
import { ArrowRightIcon, BackIcon, ErrorIcon, SpinnerIcon, SyncIcon } from "../lib/icons";

/**
 * How wide a `.col` or `.col-key` in a list row is.
 *
 * A property rather than a `width`, so `styles.css` can drop it on a phone —
 * an inline width would outrank every rule in the file and the columns could
 * only ever be the size a desktop needs. The long version is above `.col`.
 */
export function colWidth(px: number): CSSProperties {
  // React types custom properties as unknown keys; the cast is the standard
  // way round it and is checked by the one place that reads `--w`.
  return { "--w": `${px}px` } as CSSProperties;
}

/**
 * A palette at 26px: its page, its accent, its ink.
 *
 * The one thing in the app that has to render colours it isn't wearing, which
 * is why it takes a palette rather than three props. A shipped palette arrives
 * as its `data-palette` handle and picks its own tokens up from `styles.css`; a
 * custom one arrives as the theme and carries its tokens inline. Either way the
 * three dots below read `var(--bg)`, `var(--acc)` and `var(--fg)` and never
 * learn which palette they're in — so a swatch cannot drift from the thing it
 * claims to preview.
 */
export function Swatch({ of, small = false }: { of: string | CustomTheme; small?: boolean }) {
  const custom = typeof of === "string" ? null : of;
  return (
    <span
      className={small ? "swatch swatch-sm" : "swatch"}
      data-palette={custom ? undefined : of}
      style={custom ? (tokens(custom) as CSSProperties) : undefined}
      aria-hidden="true"
    >
      <span style={{ background: "var(--acc)" }} />
      <span style={{ background: "var(--fg)" }} />
    </span>
  );
}

/* --------------------------------------------------------------- layout --- */

/**
 * The horizontal divider. One weight on purpose — there was a `faint` variant
 * too, and the two ended up separating sections on the same screen at
 * different strengths, which read as an accident rather than as hierarchy.
 * Distance between blocks carries that instead, through `m`.
 */
export function Rule({ m = "44px 0 22px" }: { m?: string }) {
  return <div className="rule" style={{ margin: m }} />;
}

/**
 * A labelled on/off switch.
 *
 * The rest of the app sets preferences by picking one of several words with an
 * accent underline, which works when there are three of them and reads as a
 * broken link when there are two. A setting that is simply on or off deserves
 * to look like one — the whole row is the target, so it's hard to miss.
 */
export function Switch({
  on,
  onChange,
  label,
  note,
}: {
  on: boolean;
  onChange: (on: boolean) => void;
  label: ReactNode;
  note?: ReactNode;
}) {
  return (
    <button role="switch" aria-checked={on} onClick={() => onChange(!on)} className="switch-row">
      <span style={{ flex: 1, minWidth: 0 }}>
        <span style={{ display: "block", fontSize: "var(--fs-md)" }}>{label}</span>
        {note && (
          <span
            style={{
              display: "block",
              fontSize: "var(--fs-small)",
              color: "var(--faint)",
              marginTop: 5,
              lineHeight: 1.5,
            }}
          >
            {note}
          </span>
        )}
      </span>
      <span
        aria-hidden="true"
        style={{
          flex: "none",
          width: 38,
          height: 21,
          borderRadius: 11,
          background: on ? "var(--acc)" : "var(--line)",
          border: `1px solid ${on ? "var(--acc)" : "var(--line)"}`,
          transition: "background var(--dur-slow)",
          position: "relative",
        }}
      >
        <span
          style={{
            position: "absolute",
            top: 2,
            left: 2,
            width: 15,
            height: 15,
            borderRadius: "50%",
            background: "var(--bg)",
            // The knob travels the 17px between its two seats. As a transform
            // rather than as `left`, so the slide is composited instead of
            // relaying out the track on every frame.
            transform: on ? "translateX(17px)" : "none",
            transition: "transform var(--dur-slow) cubic-bezier(.3,1.4,.5,1)",
          }}
        />
      </span>
    </button>
  );
}

export function Eyebrow({
  children,
  large = false,
  style,
}: {
  children: ReactNode;
  large?: boolean;
  style?: CSSProperties;
}) {
  return (
    <div className={large ? "eyebrow-lg" : "eyebrow"} style={style}>
      {children}
    </div>
  );
}

/**
 * The header every screen opens with: an eyebrow line stating what you're
 * looking at, the serif title under it, and a paragraph of lede.
 *
 * The eyebrow row is what fixes the geometry. `action` — the refresh button on
 * the screens that have one — sits at its far right, so the title always begins
 * at the same height whether or not the screen can refresh, and the control
 * never moves between screens. This used to be per-screen: some put refresh on
 * the title's own baseline and some on an eyebrow above it, which shifted the
 * title by a line as you moved through the sidebar.
 *
 * `eyebrow` is required for that reason. Every screen can say what slice of
 * data it's showing — a date, a window, a source — and if one genuinely can't,
 * that's a sign the screen doesn't know its own scope.
 */
export function PageHeader({
  eyebrow,
  title,
  lede,
  action,
  space = 44,
}: {
  eyebrow: ReactNode;
  title: ReactNode;
  lede?: ReactNode;
  action?: ReactNode;
  /** Distance to the first block of content. Lower it when a control row follows. */
  space?: number;
}) {
  return (
    // `page-header` is what lets the screen transition animate this block's
    // words without animating the action beside them — see `.screen` in the
    // stylesheet. Nothing visual hangs off the class.
    <header className="page-header" style={{ marginBottom: space }}>
      <div
        className="section-head"
        style={{
          // Holds the row open on screens with no action, so a missing refresh
          // button can't pull the title up.
          minHeight: 14,
        }}
      >
        {/* Stacks on a phone like every other section head, and here it earns
            it twice over: Weight's eyebrow alone is wider than the column, so
            side by side the two either overlap or the words break mid-phrase
            behind the button. */}
        <div className="eyebrow-lg">{eyebrow}</div>
        {action}
      </div>
      <h1 className="h1" style={{ margin: "18px 0 0" }}>
        {title}
      </h1>
      {lede && (
        <p
          className="lede"
          style={{
            fontSize: "var(--fs-lg)",
            lineHeight: 1.7,
            maxWidth: "62ch",
            margin: "12px 0 0",
          }}
        >
          {lede}
        </p>
      )}
    </header>
  );
}

/* --------------------------------------------------------------- metric --- */

/**
 * A large mono number over a small uppercase caption — the design's main way
 * of showing a figure. `unit` renders smaller and inline, as in "7h 12m".
 */
export function Metric({
  value,
  label,
  size = 38,
  accent = false,
}: {
  value: ReactNode;
  label: string;
  size?: number;
  accent?: boolean;
}) {
  return (
    <div>
      <div
        className="mono"
        style={{
          fontSize: size,
          lineHeight: 1,
          letterSpacing: "-0.04em",
          color: accent ? "var(--acc)" : undefined,
        }}
      >
        {value}
      </div>
      <div
        style={{
          // 1.35 rather than 1, because a label that qualifies itself — "Fuel
          // balance · yesterday" — can reach two lines in a phone's column, and
          // at a line-height of 1 the two sit on top of each other. `balance`
          // splits them evenly instead of leaving one word on the second line.
          font: "400 var(--fs-micro)/1.35 'Instrument Sans', sans-serif",
          letterSpacing: "0.11em",
          textTransform: "uppercase",
          color: "var(--mut)",
          marginTop: 9,
          textWrap: "balance",
        }}
      >
        {label}
      </div>
    </div>
  );
}

/** The smaller unit that trails a metric value: "7<small>h</small> 12<small>m</small>". */
export function Unit({ children, size = 26 }: { children: ReactNode; size?: number }) {
  return <span style={{ fontSize: size }}>{children}</span>;
}

/**
 * `gap` is the desktop's. A phone gets a fixed, much tighter pair.
 *
 * 54px between two figures in a 320px column means at most two per line and
 * usually one, so Today's six metrics came out as a five-row ladder with a
 * different number of items in each row. The rhythm the gap is there to create
 * is a desktop-width effect; at phone width the same number just wastes the
 * line. 26 across and 22 down fits three of the short ones and reads as a grid.
 */
const METRIC_GAP_MOBILE = "22px 26px";

export function MetricRow({
  children,
  gap = 54,
  style,
}: {
  children: ReactNode;
  gap?: number;
  style?: CSSProperties;
}) {
  return (
    <div
      style={{
        display: "flex",
        gap: IS_MOBILE ? METRIC_GAP_MOBILE : gap,
        flexWrap: "wrap",
        ...style,
      }}
    >
      {children}
    </div>
  );
}

/* --------------------------------------------------------------- charts --- */

export interface Series {
  values: Point[];
  stroke?: string;
  width?: number;
  dashed?: boolean;
  /** Draw low values at the top. Pace is the case: a smaller number is faster,
   *  so an uninverted pace chart reads upside down. */
  invert?: boolean;
  /** Wash the area under the line in its own colour, fading out downwards.
   *  For the series a chart is *about* — a second line laid over the first for
   *  comparison reads better left bare. */
  fill?: boolean;
  /** Named in the hover readout, when a chart carries more than one line. */
  name?: string;
  /** How the hover readout writes this series' numbers. */
  format?: (v: number) => string;
}

/** Values are all over the place — kg, kcal, bpm — so a call site that doesn't
 *  say how to write its own gets the least presumptuous thing possible. */
const plain = (v: number) => v.toLocaleString(undefined, { maximumFractionDigits: 1 });

interface Reading {
  /** Vertical position of the point, as a fraction of the chart height. */
  top: number;
  stroke: string;
  name?: string;
  text: string;
}

/**
 * The design's one chart: a baseline hairline and one or more polylines,
 * stretched to the container width. `preserveAspectRatio="none"` is deliberate
 * — these are trend shapes, not plots to measure off.
 *
 * Which is exactly why hovering has to answer the question the shape raises.
 * The pointer snaps to the nearest sample that actually holds a value, so a
 * chart full of gaps can't report a reading from a day that has none.
 */
export function LineChart({
  series,
  height = 84,
  viewWidth = 720,
  pad = 8,
  baseline = true,
  shareScale = false,
  labels,
  hoverIndex,
  onHoverIndex,
}: {
  series: Series[];
  height?: number;
  viewWidth?: number;
  pad?: number;
  baseline?: boolean;
  /** Scale every series together, for charts where the lines are comparable. */
  shareScale?: boolean;
  /** One per index — the date or day the hover readout is headed with. */
  labels?: string[];
  /**
   * Hover driven from outside, for charts that share an index with something
   * else on the page — the route map and these charts are two views of one set
   * of samples, and pointing at a moment on either should mark it on both.
   *
   * Omit both props and the chart keeps its own hover, which is what every
   * other screen wants.
   */
  hoverIndex?: number | null;
  onHoverIndex?: (index: number | null) => void;
}) {
  const [internal, setInternal] = useState<number | null>(null);
  const controlled = hoverIndex !== undefined;
  const hover = controlled ? hoverIndex : internal;
  const setHover = (index: number | null) => {
    // The internal value is kept up to date either way. It costs one render and
    // means a chart that stops being controlled doesn't resume from a stale mark.
    setInternal(index);
    onHoverIndex?.(index);
  };
  // Gradients are referenced by fragment, so two charts on one screen can't
  // share an id. React's colons are legal in an id but not in a `url(#…)`, so
  // they come out before the reference is written.
  const uid = useId().replace(/:/g, "");

  // Inverted series are negated before scaling, so everything downstream — the
  // shared range, the polylines, the hover dot — works off the same arrays.
  const drawn = series.map((s) =>
    s.invert ? s.values.map((v) => (v == null ? null : -v)) : s.values,
  );
  const all = drawn.flat().filter((v): v is number => v != null);
  const shared = shareScale && all.length ? { min: Math.min(...all), max: Math.max(...all) } : {};
  const opts = { width: viewWidth, height, pad, ...shared };

  const count = Math.max(...series.map((s) => s.values.length), 0);
  // Only indices with something to report are hoverable; snapping to a blank
  // one would open an empty card over the gap it's pointing at.
  const populated: number[] = [];
  for (let i = 0; i < count; i++) {
    if (series.some((s) => s.values[i] != null && isFinite(s.values[i]!))) populated.push(i);
  }

  const track = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!populated.length || count < 2) return;
    const rect = e.currentTarget.getBoundingClientRect();
    if (!rect.width) return;
    const t = Math.min(Math.max((e.clientX - rect.left) / rect.width, 0), 1);
    const at = t * (count - 1);
    let nearest = populated[0];
    for (const i of populated) {
      if (Math.abs(i - at) < Math.abs(nearest - at)) nearest = i;
    }
    setHover(nearest);
  };

  const readings: Reading[] = [];
  if (hover != null) {
    series.forEach((s, i) => {
      const v = s.values[hover];
      if (v == null || !isFinite(v)) return;
      const scale = scaleFor(drawn[i], opts);
      if (!scale) return;
      readings.push({
        top: scale.y(drawn[i][hover] as number) / height,
        stroke: s.stroke ?? "var(--acc)",
        name: s.name,
        text: (s.format ?? plain)(v),
      });
    });
  }

  // Anchored to the sample rather than to the pointer, so the card sits over
  // the reading it names. Near an edge it hangs off the point instead of
  // straddling it, which would put half the card outside the chart.
  const left = hover != null && count > 1 ? (hover / (count - 1)) * 100 : 0;
  const anchor = left < 12 ? "0" : left > 88 ? "-100%" : "-50%";

  return (
    <div
      style={{ position: "relative" }}
      onPointerMove={track}
      onPointerLeave={() => setHover(null)}
    >
      <svg
        viewBox={`0 0 ${viewWidth} ${height}`}
        preserveAspectRatio="none"
        style={{ width: "100%", height, display: "block" }}
        aria-hidden
      >
        <defs>
          {series.map((s, i) =>
            s.fill ? (
              <linearGradient key={i} id={`${uid}-${i}`} x1="0" y1="0" x2="0" y2="1">
                {/* Styles rather than attributes: the colour is usually a
                    custom property, and only the style resolves one for sure. */}
                <stop
                  offset="0%"
                  style={{ stopColor: s.stroke ?? "var(--acc)", stopOpacity: 0.2 }}
                />
                <stop
                  offset="100%"
                  style={{ stopColor: s.stroke ?? "var(--acc)", stopOpacity: 0 }}
                />
              </linearGradient>
            ) : null,
          )}
        </defs>
        {baseline && (
          <line
            x1="0"
            y1={height - 1}
            x2={viewWidth}
            y2={height - 1}
            stroke="var(--line2)"
            strokeWidth="1"
            vectorEffect="non-scaling-stroke"
          />
        )}
        {/* Every wash first, so a line is never dimmed by the one next to it. */}
        {series.map((s, i) =>
          s.fill
            ? areas(drawn[i], opts).map((d, j) => (
                <path key={`f${i}-${j}`} d={d} fill={`url(#${uid}-${i})`} stroke="none" />
              ))
            : null,
        )}
        {series.map((s, i) =>
          polylines(drawn[i], opts).map((points, j) => (
            <polyline
              key={`${i}-${j}`}
              points={points}
              fill="none"
              stroke={s.stroke ?? "var(--acc)"}
              strokeWidth={s.width ?? 1.3}
              strokeDasharray={s.dashed ? "3 3" : undefined}
              strokeLinejoin="round"
              strokeLinecap="round"
              vectorEffect="non-scaling-stroke"
            />
          )),
        )}
      </svg>
      {/* The marks are HTML rather than SVG on purpose: the viewBox is stretched
          horizontally, and a circle drawn inside it would come out an ellipse. */}
      {readings.length > 0 && (
        <>
          <div className="chart-guide" style={{ left: `${left}%` }} />
          {readings.map((r, i) => (
            <div
              key={i}
              className="chart-dot"
              style={{ left: `${left}%`, top: `${r.top * 100}%`, background: r.stroke }}
            />
          ))}
          <div
            className="chart-tip"
            style={{ left: `${left}%`, transform: `translateX(${anchor})` }}
          >
            {labels?.[hover!] && <div className="chart-tip-when">{labels[hover!]}</div>}
            {readings.map((r, i) => (
              <div key={i} className="chart-tip-row">
                {r.name && (
                  <span className="chart-tip-key" style={{ color: r.stroke }}>
                    {r.name}
                  </span>
                )}
                <span className="mono">{r.text}</span>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

/**
 * Inline sparkline sized to sit on a line of text. Small enough that the hover
 * readout is a single figure with no guide or dot — anything more would be
 * furniture larger than the chart it decorates.
 */
export function Spark({
  values,
  width = 90,
  height = 20,
  stroke = "var(--acc)",
  format = plain,
}: {
  values: Point[];
  width?: number;
  height?: number;
  stroke?: string;
  format?: (v: number) => string;
}) {
  const [hover, setHover] = useState<number | null>(null);
  if (!hasData(values)) return null;

  const track = (e: ReactPointerEvent<HTMLSpanElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    if (!rect.width || values.length < 2) return;
    const t = Math.min(Math.max((e.clientX - rect.left) / rect.width, 0), 1);
    const at = t * (values.length - 1);
    let nearest: number | null = null;
    values.forEach((v, i) => {
      if (v == null || !isFinite(v)) return;
      if (nearest == null || Math.abs(i - at) < Math.abs(nearest - at)) nearest = i;
    });
    setHover(nearest);
  };

  const value = hover == null ? null : values[hover];

  return (
    /* The wrapper takes the sparkline's place on the text line — the svg is
       block-level inside it now, so vertical-align has to sit out here. */
    <span
      style={{
        position: "relative",
        display: "inline-block",
        margin: "0 6px",
        verticalAlign: -4,
      }}
      onPointerMove={track}
      onPointerLeave={() => setHover(null)}
    >
      <svg
        viewBox={`0 0 ${width} ${height}`}
        style={{ width, height, display: "block", overflow: "visible" }}
        aria-hidden
      >
        {polylines(values, { width, height, pad: 3 }).map((points, i) => (
          <polyline
            key={i}
            points={points}
            fill="none"
            stroke={stroke}
            strokeWidth="1.25"
            strokeLinejoin="round"
          />
        ))}
      </svg>
      {value != null && (
        <span
          className="chart-tip"
          style={{
            left: `${(hover! / Math.max(values.length - 1, 1)) * 100}%`,
            transform: "translateX(-50%)",
          }}
        >
          <span className="mono">{format(value)}</span>
        </span>
      )}
    </span>
  );
}

export function AxisLabels({ labels }: { labels: string[] }) {
  return (
    <div
      style={{
        display: "flex",
        justifyContent: "space-between",
        font: "400 var(--fs-micro)/1 'Instrument Sans', sans-serif",
        color: "var(--faint)",
        marginTop: 9,
      }}
    >
      {labels.map((l, i) => (
        <span key={i}>{l}</span>
      ))}
    </div>
  );
}

/* ---------------------------------------------------------------- bullet --- */

/** The dotted list used for "Attention" and risk flags. */
export function Bullet({ children, accent = false }: { children: ReactNode; accent?: boolean }) {
  return (
    <div
      style={{
        display: "flex",
        gap: 14,
        alignItems: "baseline",
        fontSize: "var(--fs-md)",
        lineHeight: 1.55,
      }}
    >
      <span
        style={{
          width: 4,
          height: 4,
          borderRadius: "50%",
          background: accent ? "var(--acc)" : "var(--faint)",
          flex: "none",
          transform: "translateY(-3px)",
        }}
      />
      <span style={{ flex: 1 }}>{children}</span>
    </div>
  );
}

/* ---------------------------------------------------------------- states --- */

/**
 * Shown wherever a screen has no data to draw. Deliberately explicit about
 * *why* it's empty — an unconnected integration and an empty cache need
 * different actions from the user, and inventing numbers to fill the space
 * would make the app worse than blank.
 *
 * No icon. This used to lead with the screen's own nav glyph at 26px, which
 * only ever appeared on whichever screen happened to be empty — so a stray
 * backpack floated over Gear while every other screen showed plain text. The
 * app puts icons in the nav and in buttons, nowhere in the reading column.
 */
export function Empty({
  title,
  body,
  action,
}: {
  title: string;
  body: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div style={{ padding: "10px 0 0", maxWidth: "58ch" }}>
      <div className="serif" style={{ fontSize: 25, lineHeight: 1.3, marginBottom: 12 }}>
        {title}
      </div>
      <p className="lede" style={{ fontSize: "var(--fs-md)", lineHeight: 1.7 }}>
        {body}
      </p>
      {action && <div style={{ marginTop: 22 }}>{action}</div>}
    </div>
  );
}

/**
 * How long a screen is allowed to have nothing before it admits to waiting.
 * A cache read resolves in two or three frames — long enough for a spinner to
 * mount and unmount, which reads as a flash of broken layout rather than as
 * progress. Past this the wait is real and worth narrating.
 */
const SPINNER_DELAY = 220;

export function Loading({ label = "Reading the cache" }: { label?: string }) {
  const [show, setShow] = useState(false);

  useEffect(() => {
    const t = setTimeout(() => setShow(true), SPINNER_DELAY);
    return () => clearTimeout(t);
  }, []);

  // Nothing at all, not a reserved box: the page transition is already fading
  // this column in, so an empty screen for a fifth of a second is invisible.
  if (!show) return null;

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 9,
        fontSize: "var(--fs-base)",
        color: "var(--faint)",
        padding: "8px 0",
      }}
    >
      <SpinnerIcon size={13} className="spin" style={{ flex: "none" }} aria-hidden />
      {label}…
    </div>
  );
}

export function ErrorNote({ error }: { error: unknown }) {
  const msg = error instanceof Error ? error.message : String(error);
  return (
    <div
      style={{
        display: "flex",
        // Not centred: the message wraps to several lines often enough that a
        // centred icon would drift into the middle of the paragraph.
        alignItems: "flex-start",
        gap: 9,
        fontSize: "var(--fs-base)",
        lineHeight: 1.6,
        color: "var(--acc)",
        padding: "10px 0",
        maxWidth: "60ch",
      }}
    >
      <ErrorIcon size={16} style={{ flex: "none", marginTop: 2 }} aria-hidden />
      <span>{msg}</span>
    </div>
  );
}

/* ----------------------------------------------------------------- icons --- */

/* Thin wrappers over the Phosphor set rather than bare re-exports: every icon
 * in a text row wants `flex: none` so it can't be squeezed by a long label, and
 * these three are the ones screens reach for by name. The glyphs themselves,
 * and the shared duotone weight, come from `lib/icons` and the context in
 * `main.tsx`. */

export function ArrowRight() {
  return <ArrowRightIcon size={16} style={{ flex: "none" }} aria-hidden />;
}

export function RotateArrow({ spinning = false }: { spinning?: boolean }) {
  return (
    <SyncIcon
      size={13}
      className={spinning ? "spin" : undefined}
      style={{ flex: "none" }}
      aria-hidden
    />
  );
}

export function ChevronLeft() {
  return <BackIcon size={14} style={{ flex: "none" }} aria-hidden />;
}

export function BackLink({
  children,
  onClick,
  style,
}: {
  children: ReactNode;
  onClick: () => void;
  style?: CSSProperties;
}) {
  return (
    <button
      className="quiet"
      onClick={onClick}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 7,
        fontSize: "var(--fs-small)",
        ...style,
      }}
    >
      <ChevronLeft />
      {children}
    </button>
  );
}
