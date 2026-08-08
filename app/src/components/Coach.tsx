/**
 * The part of the app that speaks first.
 *
 * Two pieces, both on Today: the week's goals as rings, and whatever the coach
 * has to say about them. Usually it has nothing to say, and this renders the
 * rings alone — a card that always has an opinion is one you stop reading.
 *
 * Every nudge can show its working. `evidence` is one tap away rather than
 * hidden, because the whole argument for trusting a nudge is that the numbers
 * behind it are right there.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { coach, dismissNudge, type GoalRing, type Nudge, type NudgeTone } from "../lib/api";

/** Ring geometry. Small enough that five fit across a phone. */
const R = 17;
const STROKE = 3.5;
const CIRC = 2 * Math.PI * R;

const TONE: Record<NudgeTone, { colour: string; label: string }> = {
  good: { colour: "var(--acc)", label: "Going well" },
  neutral: { colour: "var(--mut)", label: "Worth knowing" },
  watch: { colour: "var(--warn)", label: "Worth acting on" },
};

function ringValue(ring: GoalRing): string {
  switch (ring.unit) {
    case "minutes":
      return `${Math.round(ring.actual)}/${Math.round(ring.target)}m`;
    case "percent":
      return `${Math.round(ring.actual)}/${Math.round(ring.target)}%`;
    case "spm":
      return `${Math.round(ring.actual)}/${Math.round(ring.target)}`;
    default:
      return `${Math.round(ring.actual)}/${Math.round(ring.target)}`;
  }
}

function Ring({ ring }: { ring: GoalRing }) {
  const size = (R + STROKE) * 2;
  return (
    <div
      style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6, width: 72 }}
      title={
        ring.thin
          ? `${ring.label}: ${ringValue(ring)} — too little data this week to lean on`
          : `${ring.label}: ${ringValue(ring)}`
      }
    >
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={R}
          fill="none"
          stroke="var(--line)"
          strokeWidth={STROKE}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={R}
          fill="none"
          stroke={ring.met ? "var(--acc)" : "var(--fg)"}
          strokeOpacity={ring.thin ? 0.35 : 1}
          strokeWidth={STROKE}
          strokeLinecap="round"
          strokeDasharray={`${CIRC * ring.fraction} ${CIRC}`}
          // Start at twelve o'clock rather than three.
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
        />
      </svg>
      <div
        style={{
          fontSize: "var(--fs-micro)",
          color: "var(--faint)",
          textAlign: "center",
          lineHeight: 1.25,
        }}
      >
        <div style={{ color: "var(--mut)" }}>{ring.label}</div>
        <span className="mono">{ringValue(ring)}</span>
        {ring.thin && <span title="Too little data this week to lean on"> ·</span>}
      </div>
    </div>
  );
}

function NudgeCard({ nudge, onDismiss }: { nudge: Nudge; onDismiss: () => void }) {
  const [showing, setShowing] = useState(false);
  const tone = TONE[nudge.tone];

  return (
    <div
      style={{
        borderLeft: `2px solid ${tone.colour}`,
        paddingLeft: 14,
        marginBottom: 18,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          gap: 12,
        }}
      >
        <span className="eyebrow" style={{ color: tone.colour }}>
          {tone.label}
          {/* Only worth saying once it has been said before — "day 1" is noise. */}
          {nudge.daysRunning > 1 && ` · day ${nudge.daysRunning}`}
        </span>
        <button
          type="button"
          className="action"
          onClick={onDismiss}
          style={{ fontSize: "var(--fs-micro)", color: "var(--faint)" }}
          title="Put this away for today. It comes back tomorrow if it's still true."
        >
          Dismiss
        </button>
      </div>

      <div style={{ fontSize: "var(--fs-lg)", margin: "4px 0 6px" }}>{nudge.title}</div>
      <div style={{ fontSize: "var(--fs-base)", color: "var(--mut)", lineHeight: 1.5 }}>
        {nudge.body}
      </div>

      <button
        type="button"
        className="action"
        onClick={() => setShowing((s) => !s)}
        style={{ fontSize: "var(--fs-micro)", color: "var(--faint)", marginTop: 8 }}
      >
        {showing ? "Hide the numbers" : "Show the numbers"}
      </button>
      {showing && (
        <ul
          style={{
            listStyle: "none",
            padding: 0,
            margin: "8px 0 0",
            fontSize: "var(--fs-caption)",
            color: "var(--faint)",
          }}
        >
          {nudge.evidence.map((e) => (
            <li key={e} className="mono" style={{ padding: "2px 0" }}>
              {e}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function CoachPanel() {
  const client = useQueryClient();
  const report = useQuery({ queryKey: ["coach"], queryFn: coach });
  const dismiss = useMutation({
    mutationFn: dismissNudge,
    // Refetch rather than patching in place: dismissing one can reveal the
    // next, and the rules decide that, not this component.
    onSuccess: () => client.invalidateQueries({ queryKey: ["coach"] }),
  });

  // A coach that can't load is not worth an error state on the day's first
  // screen — everything else on Today still works.
  if (report.isLoading || report.error || !report.data) return null;

  const { week, nudges } = report.data;
  const standing = nudges.filter((n) => !n.dismissed);
  if (!week.rings.length && !standing.length) return null;

  return (
    <section style={{ margin: "34px 0" }}>
      {week.rings.length > 0 && (
        <>
          <div className="eyebrow" style={{ marginBottom: 12 }}>
            This week
          </div>
          <div
            style={{
              display: "flex",
              gap: 10,
              flexWrap: "wrap",
              marginBottom: standing.length ? 26 : 0,
            }}
          >
            {week.rings.map((ring) => (
              <Ring key={ring.id} ring={ring} />
            ))}
          </div>
        </>
      )}

      {standing.map((n) => (
        <NudgeCard key={n.id} nudge={n} onDismiss={() => dismiss.mutate(n.id)} />
      ))}
    </section>
  );
}
