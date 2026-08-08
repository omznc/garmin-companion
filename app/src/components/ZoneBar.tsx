/**
 * A session's heart-rate zones as one stacked bar.
 *
 * The full per-zone table lives on the activity screen; this is the glanceable
 * form, for places that need the shape of a session rather than its numbers.
 * Z1–Z2 read as muted and Z3–Z5 as accent, so "how much of this was hard"
 * is answerable without reading a single figure — which is the whole point.
 */
import type { CachedActivity } from "../lib/api";
import { zonePercentages, zoneTotal } from "../lib/derive";

const LABELS = ["Z1", "Z2", "Z3", "Z4", "Z5"];

/**
 * Z3 up. Each step darker so the ladder is legible inside the accent block.
 *
 * Exported because the route on the activity screen is coloured by the same
 * ladder. A trace whose red meant something different from the red in this bar
 * would be two legends for one idea.
 */
export const ZONE_FILL = [
  "var(--line)",
  "var(--mut)",
  "color-mix(in srgb, var(--acc) 55%, transparent)",
  "color-mix(in srgb, var(--acc) 78%, transparent)",
  "var(--acc)",
];

export function ZoneBar({
  activity,
  height = 10,
  legend = true,
}: {
  activity: CachedActivity;
  height?: number;
  legend?: boolean;
}) {
  if (zoneTotal(activity) <= 0) {
    return (
      <div style={{ fontSize: "var(--fs-small)", color: "var(--faint)" }}>
        No heart-rate data recorded — not a session spent entirely in Z1.
      </div>
    );
  }

  const pct = zonePercentages(activity);

  return (
    <div>
      <div style={{ display: "flex", height, overflow: "hidden", borderRadius: 2 }}>
        {pct.map((p, i) =>
          p > 0 ? (
            <div
              key={i}
              title={`${LABELS[i]} · ${p.toFixed(0)}%`}
              style={{ width: `${p}%`, background: ZONE_FILL[i] }}
            />
          ) : null,
        )}
      </div>
      {legend && (
        <div
          style={{
            display: "flex",
            gap: 16,
            marginTop: 9,
            fontSize: "var(--fs-caption)",
            color: "var(--mut)",
            flexWrap: "wrap",
          }}
        >
          {pct.map((p, i) =>
            // Below 1% the label is noise, and five of them crowd out the ones
            // that matter.
            p >= 1 ? (
              <span key={i} style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
                <span
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: 1,
                    background: ZONE_FILL[i],
                    display: "inline-block",
                  }}
                />
                {LABELS[i]} {p.toFixed(0)}%
              </span>
            ) : null,
          )}
        </div>
      )}
    </div>
  );
}
