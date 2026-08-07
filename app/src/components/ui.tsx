/** The primitives every screen is built from. */
import type { CSSProperties, ReactNode } from "react";
import { polylines, hasData, type Point } from "../lib/chart";

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

/** Page heading — the serif display face, used once per screen. */
export function PageTitle({
  children,
  style,
}: {
  children: ReactNode;
  style?: CSSProperties;
}) {
  return (
    <h1 className="h1" style={{ marginBottom: 10, ...style }}>
      {children}
    </h1>
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
          font: "400 10.5px/1 'Instrument Sans', sans-serif",
          letterSpacing: "0.11em",
          textTransform: "uppercase",
          color: "var(--mut)",
          marginTop: 11,
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
    <div style={{ display: "flex", gap, flexWrap: "wrap", ...style }}>{children}</div>
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
}

/**
 * The design's one chart: a baseline hairline and one or more polylines,
 * stretched to the container width. `preserveAspectRatio="none"` is deliberate
 * — these are trend shapes, not plots to measure off.
 */
export function LineChart({
  series,
  height = 84,
  viewWidth = 720,
  pad = 8,
  baseline = true,
  shareScale = false,
}: {
  series: Series[];
  height?: number;
  viewWidth?: number;
  pad?: number;
  baseline?: boolean;
  /** Scale every series together, for charts where the lines are comparable. */
  shareScale?: boolean;
}) {
  // Shared scaling has to see the same values the polylines will, or an
  // inverted series would be scaled against the wrong range.
  const all = series
    .flatMap((s) => (s.invert ? s.values.map((v) => (v == null ? null : -v)) : s.values))
    .filter((v): v is number => v != null);
  const shared = shareScale && all.length
    ? { min: Math.min(...all), max: Math.max(...all) }
    : {};

  return (
    <svg
      viewBox={`0 0 ${viewWidth} ${height}`}
      preserveAspectRatio="none"
      style={{ width: "100%", height, display: "block" }}
      aria-hidden
    >
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
      {series.map((s, i) =>
        polylines(s.invert ? s.values.map((v) => (v == null ? null : -v)) : s.values, {
          width: viewWidth,
          height,
          pad,
          ...shared,
        }).map(
          (points, j) => (
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
          ),
        ),
      )}
    </svg>
  );
}

/** Inline sparkline sized to sit on a line of text. */
export function Spark({
  values,
  width = 90,
  height = 20,
  stroke = "var(--acc)",
}: {
  values: Point[];
  width?: number;
  height?: number;
  stroke?: string;
}) {
  if (!hasData(values)) return null;
  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      style={{
        width,
        height,
        verticalAlign: -4,
        margin: "0 6px",
        overflow: "visible",
      }}
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
  );
}

export function AxisLabels({ labels }: { labels: string[] }) {
  return (
    <div
      style={{
        display: "flex",
        justifyContent: "space-between",
        font: "400 10.5px/1 'Instrument Sans', sans-serif",
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
export function Bullet({
  children,
  accent = false,
}: {
  children: ReactNode;
  accent?: boolean;
}) {
  return (
    <div
      style={{
        display: "flex",
        gap: 14,
        alignItems: "baseline",
        fontSize: 14.5,
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
      <div
        className="serif"
        style={{ fontSize: 23, lineHeight: 1.3, marginBottom: 12 }}
      >
        {title}
      </div>
      <p className="lede" style={{ fontSize: 15, lineHeight: 1.7 }}>
        {body}
      </p>
      {action && <div style={{ marginTop: 22 }}>{action}</div>}
    </div>
  );
}

export function Loading({ label = "Reading the cache" }: { label?: string }) {
  return (
    <div style={{ fontSize: 13.5, color: "var(--faint)", padding: "8px 0" }}>
      {label}…
    </div>
  );
}

export function ErrorNote({ error }: { error: unknown }) {
  const msg = error instanceof Error ? error.message : String(error);
  return (
    <div
      style={{
        fontSize: 13.5,
        lineHeight: 1.6,
        color: "var(--acc)",
        padding: "10px 0",
        maxWidth: "60ch",
      }}
    >
      {msg}
    </div>
  );
}

/* ----------------------------------------------------------------- icons --- */

export function ArrowRight() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.3"
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ flex: "none" }}
      aria-hidden
    >
      <path d="M3 8h9.5M8.5 4l4 4-4 4" />
    </svg>
  );
}

export function ChevronLeft() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ flex: "none" }}
      aria-hidden
    >
      <path d="M9.5 3.5 5 8l4.5 4.5" />
    </svg>
  );
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
        fontSize: 12.5,
        ...style,
      }}
    >
      <ChevronLeft />
      {children}
    </button>
  );
}
