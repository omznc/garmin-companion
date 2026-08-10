/**
 * The thing that actually gets shared.
 *
 * Deliberately not a screenshot. These screens are long scrolling columns with
 * a nav down one side, and any rectangle cropped out of one is a picture of a
 * cropped page — the reader gets whatever happened to be in frame rather than
 * the answer. So the card is composed from the same figures the screen already
 * derived, in the same type and the same palette, laid out for the shape it's
 * going into.
 *
 * Fixed pixel sizes throughout, and no `IS_MOBILE` anywhere. Everything else in
 * this app sizes itself to a viewport; a card has no viewport, only a canvas
 * that is 540 units wide on every platform. A phone and a desktop producing
 * visibly different cards from the same session would be a bug, and reading the
 * responsive scale here is how that happens.
 *
 * The palette is not fixed, though — it reads the live CSS variables, so a card
 * comes out in whatever theme the app is wearing, Android's dynamic colours
 * included.
 */
import type { ReactNode } from "react";
import { longDate } from "../lib/format";

export interface ShareMetric {
  label: string;
  value: string;
  /** Rendered smaller and tight against the value, as in "7h 12m". */
  unit?: string;
}

export interface ShareContent {
  /** The small uppercase line above the title — a date, or the screen's name. */
  eyebrow: string;
  title: string;
  /** The one figure the card is about, if the screen has one. */
  headline?: { value: string; unit?: string; caption: string };
  /** Three to six. Past six the grid stops being readable at thumbnail size. */
  metrics: ShareMetric[];
  /**
   * The route's shape, for a session that went somewhere. Given its own slot
   * rather than going through `chart` because it competes with nothing: an
   * outdoor run's card wants the shape *and* the zone bar, and they're the two
   * halves of what a session was.
   */
  route?: (height: number) => ReactNode;
  /** An SVG the screen already draws — a zone bar, a week of distance. */
  chart?: ReactNode;
  /** One line under the chart, explaining what it is. */
  chartLabel?: string;
  /**
   * A date for the footer, for screens whose eyebrow isn't already one.
   *
   * Omitted by the screens that lead with the date, because a card carrying it
   * twice reads as a template that didn't get filled in. A screen about one
   * past session passes that session's date rather than today's, so the card
   * doesn't stamp a run with the day it happened to be shared.
   */
  stamp?: Date;
}

export type Shape = "portrait" | "square";

/**
 * A phone shares into feeds that are 9:16 and crops anything that isn't; a
 * desktop is pasting into a chat window, where a tall image is a scroll. The
 * width is the same in both so the type scale below can be, too.
 */
export const CARD: Record<Shape, { width: number; height: number }> = {
  portrait: { width: 540, height: 960 },
  square: { width: 540, height: 540 },
};

/** Rendered at 2×, so 1080×1920 and 1080×1080 come out the far side. */
export const SCALE = 2;

/**
 * Marks the block the renderer is allowed to shrink.
 *
 * A card's content isn't a fixed height: the title can wrap to a second line, a
 * session may or may not have a route, and the square has about 330px to put
 * all of it in. Overflowing a centred flex box overflows in *both* directions,
 * so the first version of this put the distance through the middle of the title
 * and the footer rule through the zone legend.
 *
 * The layout below is sized so the ordinary cards fit outright. This is the
 * guarantee for the rest: whatever is left over gets scaled to fit rather than
 * drawn on top of something. A card at 94% is a card nobody notices; a card
 * with two lines of type in the same place is a bug on someone's timeline.
 */
export const FIT_ATTR = "data-share-fit";

/* ------------------------------------------------------------------ bits --- */

function Figure({
  value,
  unit,
  size,
  label,
  accent = false,
}: {
  value: string;
  unit?: string;
  size: number;
  label: string;
  accent?: boolean;
}) {
  return (
    <div>
      <div
        style={{
          fontFamily: '"Geist Mono", ui-monospace, monospace',
          fontWeight: 400,
          fontSize: size,
          lineHeight: 1,
          letterSpacing: "-0.04em",
          color: accent ? "var(--acc)" : "var(--fg)",
          whiteSpace: "nowrap",
        }}
      >
        {value}
        {unit && <span style={{ fontSize: Math.round(size * 0.55) }}>{unit}</span>}
      </div>
      <div
        style={{
          font: '400 11px/1.3 "Instrument Sans", sans-serif',
          letterSpacing: "0.11em",
          textTransform: "uppercase",
          color: "var(--mut)",
          marginTop: 8,
        }}
      >
        {label}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ card --- */

export function ShareCard({ content, shape }: { content: ShareContent; shape: Shape }) {
  const { width, height } = CARD[shape];
  const tall = shape === "portrait";
  const pad = tall ? 52 : 44;

  // What fits, which is mostly a question about the square.
  //
  // Without a route, both shapes hold everything they're given. A route costs
  // a row of the grid in either — the portrait drops to four figures and the
  // square to two — because the alternative is the renderer scaling the whole
  // body to fit, and a card at 82% is one where the zone bar no longer reaches
  // the margins and the type is quietly a size too small. Better to show fewer
  // figures at the size they were drawn for: the shape of where you went says
  // more on a card than the fifth and sixth numbers do.
  const metrics = content.metrics.slice(0, content.route ? (tall ? 4 : 2) : tall ? 6 : 4);

  /** The route band's height. Its own aspect is preserved inside this. */
  const routeHeight = tall ? 180 : 92;

  // On a square card carrying a route, the chart's caption goes: the zone
  // legend directly above it already names every band, so the line is a label
  // for something that just labelled itself, and it costs the 26px that decide
  // whether the rest is drawn full size.
  const chartLabel = tall || !content.route ? content.chartLabel : undefined;

  return (
    <div
      style={{
        width,
        height,
        // Explicit rather than inherited: the renderer rasterises this node on
        // its own, and a transparent PNG posted into a dark chat is unreadable.
        background: "var(--bg)",
        color: "var(--fg)",
        fontFamily: '"Instrument Sans", system-ui, sans-serif',
        // Stated, not inherited. Borrowed pieces like `ZoneBar` set a font size
        // and leave line-height to the page, and the page doesn't set one — so
        // their glyphs hang below a line box that measures shorter than the ink
        // in it. The renderer sizes the card off that measurement, so an
        // unstated line-height is how a legend ends up sliced along its
        // descenders with the arithmetic insisting it fits.
        lineHeight: 1.45,
        padding: pad,
        boxSizing: "border-box",
        display: "flex",
        flexDirection: "column",
        // No border radius. The corners belong to whatever this is posted into,
        // and rounding them here bakes in a guess about the background behind.
        overflow: "hidden",
      }}
    >
      <div
        style={{
          font: '400 12px/1 "Instrument Sans", sans-serif',
          letterSpacing: "0.14em",
          textTransform: "uppercase",
          color: "var(--faint)",
        }}
      >
        {content.eyebrow}
      </div>

      <h1
        style={{
          fontFamily: "var(--serif)",
          fontWeight: 400,
          fontSize: tall ? 46 : 38,
          lineHeight: 1.08,
          margin: `${tall ? 20 : 16}px 0 0`,
          textWrap: "balance",
        }}
      >
        {content.title}
      </h1>

      {/* The body takes the slack and centres in it, rather than stacking from
          the title down and leaving whatever's left as a hole above the footer.
          On a 9:16 card that hole is a third of the image — the card was
          designed against the square, where the same content very nearly fills
          the frame, and the portrait version has 420px more to find a use for.
          Centring is the use. */}
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          justifyContent: "center",
          minHeight: 0,
          paddingTop: tall ? 32 : 20,
          overflow: "hidden",
        }}
      >
        {/* The measured box. `renderCard` compares this against its parent
            after layout and scales it down if it doesn't fit — see FIT_ATTR.
            It has to be one element with a natural height for that to work,
            which is why the body's children are wrapped rather than being flex
            children of the centring box directly. */}
        <div
          {...{ [FIT_ATTR]: "" }}
          style={{
            display: "flex",
            flexDirection: "column",
            gap: tall ? 52 : 20,
            // Never compressed by the centring box above. A flex item shrinks
            // to fit by default, which would let this report a height that fits
            // while its own children spilled out of it — the renderer would
            // measure no overflow and scale nothing, and the card would come
            // out with its zone legend sliced in half.
            flexShrink: 0,
            // Left edge, not the centre. When the renderer scales this block
            // to fit, a centre origin walks it inwards from the margin the
            // title is aligned to — a 6% shrink reads as the figures being
            // indented by accident. Vertical centring still comes from the box.
            transformOrigin: "left center",
          }}
        >
          {content.headline && (
            <Figure
              value={content.headline.value}
              unit={content.headline.unit}
              size={tall ? 92 : content.route ? 56 : 60}
              label={content.headline.caption}
              accent
            />
          )}

          {metrics.length > 0 && (
            <div
              style={{
                display: "grid",
                // Two columns rather than a wrapping flex row: the figures are
                // different widths and a flex row leaves the second line ragged
                // against the first, which reads as a mistake at this size.
                gridTemplateColumns: "1fr 1fr",
                gap: tall ? "34px 24px" : "22px 20px",
              }}
            >
              {metrics.map((m) => (
                <Figure
                  key={m.label}
                  value={m.value}
                  unit={m.unit}
                  size={tall ? 34 : 28}
                  label={m.label}
                />
              ))}
            </div>
          )}

          {content.route && <div style={{ height: routeHeight }}>{content.route(routeHeight)}</div>}

          {content.chart && (
            <div>
              {content.chart}
              {chartLabel && (
                <div
                  style={{
                    font: '400 11px/1.3 "Instrument Sans", sans-serif',
                    letterSpacing: "0.11em",
                    textTransform: "uppercase",
                    color: "var(--mut)",
                    marginTop: 12,
                  }}
                >
                  {chartLabel}
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      <div
        style={{
          borderTop: "1px solid var(--line)",
          // Clear of the content above it. The rule is the card's floor, and a
          // figure sitting a few pixels off it reads as having run out of room.
          marginTop: tall ? 44 : 30,
          paddingTop: 16,
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
          font: '400 11px/1 "Instrument Sans", sans-serif',
          letterSpacing: "0.11em",
          textTransform: "uppercase",
          color: "var(--faint)",
        }}
      >
        {/* The repo rather than the product name — someone who sees this and
            wants it can type it in. Mono and lower-case, because that's what it
            is: a path, not a title. No name and no greeting anywhere on the
            card; Today's own heading is "Good evening, Omar", which is fine on
            your own machine and not something to bake into an image headed
            somewhere else. */}
        <span
          style={{
            fontFamily: '"Geist Mono", ui-monospace, monospace',
            textTransform: "none",
            letterSpacing: "0.01em",
          }}
        >
          omznc/garmin-companion
        </span>
        {content.stamp && <span>{longDate(content.stamp)}</span>}
      </div>
    </div>
  );
}
