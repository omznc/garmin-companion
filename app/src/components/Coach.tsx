/**
 * The part of the app that speaks first.
 *
 * Two pieces, both on Today: the week's goals as rings, and whatever the coach
 * has to say about them. Usually it has nothing much to say, and it is supposed
 * to be allowed to — a card that always has an opinion is one you stop reading.
 *
 * What it says is [`dailyBrief`], one piece of writing shared with the evening's
 * notification: the same text, decided once. That is why a tap on the
 * notification can land here rather than merely opening the app, and why the
 * block scrolls itself into view when it was the thing that knocked.
 *
 * The rules in `garmin-core::coach` still run underneath all of this. They no
 * longer choose the words — they are the evidence handed to whoever does — and
 * when there is no model to hand it to they write the block themselves, which
 * is the `rules` branch below and is exactly what this component used to be.
 *
 * Either way the working is one tap away rather than hidden, because the whole
 * argument for trusting a nudge is that the numbers behind it are right there.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import {
  BRIEF_ID,
  chatConfig,
  coach,
  dailyBrief,
  type DailyBrief,
  dismissNudge,
  type GoalRing,
  markBriefRead,
  type Nudge,
  type NudgeTone,
} from "../lib/api";
import { clearBriefFocus, useBriefFocus } from "../lib/notificationTap";
import { since } from "../lib/format";
import { SpinnerIcon } from "../lib/icons";
import { scroller } from "../lib/scroller";
import { AiMark } from "./AiMark";

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

/**
 * How long the block stays lit after opening itself.
 *
 * Long enough to find on a phone screen you have just unlocked, short enough
 * not to become part of how the block looks.
 */
const FOCUS_MS = 2600;

/**
 * The brief: the day's writing, and the thing the notification was an extract
 * of.
 *
 * `notify` splits this in two, and the split is the whole point of letting a
 * model decide. A brief that judged the day worth interrupting for gets the
 * full treatment — tone stripe, title, evidence, somewhere to dismiss it from.
 * One that didn't gets a quiet line and nothing else, because the alternative
 * is a card announcing every day that today was unremarkable, which is how a
 * panel earns the scroll straight past it.
 */
function BriefBlock({ brief, onDismiss }: { brief: DailyBrief; onDismiss: () => void }) {
  const client = useQueryClient();
  const [showing, setShowing] = useState(false);
  const [lit, setLit] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const focus = useBriefFocus();

  const rewrite = useMutation({
    mutationFn: () => dailyBrief(true),
    onSuccess: (fresh) => client.setQueryData(["dailyBrief"], fresh),
  });

  // Open when asked to — by a tap this process heard, or by the cold-start
  // question below, which is the same request arriving from the other end.
  //
  // `notify && !read` is what covers a tap that launched the app: the event was
  // gone before anything could listen for it, but "today's brief asked to knock
  // and the block hasn't been opened since" points at the same block. It runs
  // at most once a day, and only on the days the coach judged worth
  // interrupting for — which are the minority. Marking it read is what stops it
  // happening twice.
  //
  // `opened` guards against doing it twice on one mount, which it otherwise
  // would: `clearBriefFocus` below flips `focus`, and that is a dependency, so
  // the effect re-runs — and on a cold start `unopened` is still true, because
  // nothing refetched the brief in between. One ref is cheaper than routing the
  // request through state that both paths would have to agree on.
  const unopened = brief.notify && !brief.read;
  const opened = useRef(false);
  useEffect(() => {
    if (opened.current || (!focus && !unopened)) return;
    opened.current = true;

    setShowing(true);
    setLit(true);
    void markBriefRead().catch(() => {
      // Failing to record it costs one extra scroll tomorrow morning.
    });

    // Only when it isn't already sitting on screen. Scrolling a block that the
    // reader can already see reads as the page having jumped on its own.
    const box_ = box.current;
    if (box_) {
      const { top, bottom } = box_.getBoundingClientRect();
      const height = scroller().clientHeight;
      if (top < 0 || bottom > height) {
        // Checked per call rather than cached, as `spring.ts` does: the setting
        // can change mid-session. Arriving there is the requirement; the glide
        // is only how, and reduced motion means jump.
        const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
        box_.scrollIntoView({ block: "center", behavior: reduced ? "auto" : "smooth" });
      }
    }

    clearBriefFocus();
    const off = setTimeout(() => setLit(false), FOCUS_MS);
    return () => clearTimeout(off);
  }, [focus, unopened]);

  const tone = TONE[brief.tone];
  const written = (
    <span style={{ color: "var(--faint)" }}>
      Written {since(brief.generatedAt)}
      {brief.source === "rules" && " from the rules alone"}
    </span>
  );

  // The quiet version. No stripe, no title, no dismiss — there is nothing here
  // to put away.
  if (!brief.notify) {
    return (
      <div ref={box} className={lit ? "brief-lit" : undefined}>
        <AiMark label="daily brief">
          <div style={{ fontSize: "var(--fs-base)", color: "var(--mut)", lineHeight: 1.5 }}>
            {brief.body}
          </div>
        </AiMark>
      </div>
    );
  }

  return (
    <div
      ref={box}
      className={lit ? "brief-lit" : undefined}
      style={{ borderLeft: `2px solid ${tone.colour}`, paddingLeft: 14, marginBottom: 18 }}
    >
      <AiMark label="daily brief">
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
          </span>
          <button
            type="button"
            className="action"
            onClick={onDismiss}
            style={{ fontSize: "var(--fs-micro)", color: "var(--faint)" }}
            title="Put this away for today. Tomorrow's is written fresh."
          >
            Dismiss
          </button>
        </div>

        <div style={{ fontSize: "var(--fs-lg)", margin: "4px 0 6px" }}>{brief.title}</div>
        <div style={{ fontSize: "var(--fs-base)", color: "var(--mut)", lineHeight: 1.5 }}>
          {rewrite.isPending ? (
            <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
              <SpinnerIcon size={13} className="spin" aria-hidden />
              Reading today again…
            </span>
          ) : (
            brief.body
          )}
        </div>
      </AiMark>

      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 14,
          flexWrap: "wrap",
          marginTop: 8,
          fontSize: "var(--fs-micro)",
          color: "var(--faint)",
        }}
      >
        {brief.evidence.length > 0 && (
          <button type="button" className="action" onClick={() => setShowing((s) => !s)}>
            {showing ? "Hide the numbers" : "Show the numbers"}
          </button>
        )}
        <button
          type="button"
          className="action"
          onClick={() => rewrite.mutate()}
          title="Write it again from the same data"
        >
          Rewrite
        </button>
        {written}
      </div>

      {showing && (
        <>
          <ul
            style={{
              listStyle: "none",
              padding: 0,
              margin: "8px 0 0",
              fontSize: "var(--fs-caption)",
              color: "var(--faint)",
            }}
          >
            {brief.evidence.map((e) => (
              <li key={e} className="mono" style={{ padding: "2px 0" }}>
                {e}
              </li>
            ))}
          </ul>
          {/* What the rules noticed, next to what the brief chose to say about
              it. Worth showing rather than keeping to the prompt: a brief that
              quietly walked past a real signal should be visible as having done
              that, which is the one failure a model introduces here that the
              rules never had. */}
          {brief.signals.length > 0 && (
            <div
              style={{
                fontSize: "var(--fs-caption)",
                color: "var(--faint)",
                marginTop: 10,
                lineHeight: 1.5,
              }}
            >
              Also looked at: <span className="mono">{brief.signals.join(", ")}</span>
            </div>
          )}
        </>
      )}
    </div>
  );
}

export function CoachPanel() {
  const client = useQueryClient();
  const report = useQuery({ queryKey: ["coach"], queryFn: coach });
  const config = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });

  // Its own query rather than a field on the report: this one can reach a
  // model, so it is slow in a way `coach` is not, and the rings should not wait
  // on the writing.
  //
  // `staleTime: Infinity` for the same reason the Today paragraph uses it —
  // the brief is written once a day and kept against a fingerprint, so
  // remounting the screen has nothing to gain by asking again. A sync
  // invalidates everything and this comes back with it.
  const brief = useQuery({
    queryKey: ["dailyBrief"],
    queryFn: () => dailyBrief(),
    staleTime: Infinity,
    retry: false,
  });

  const dismiss = useMutation({
    mutationFn: dismissNudge,
    // Refetch rather than patching in place: dismissing one can reveal the
    // next, and the rules decide that, not this component.
    onSuccess: () =>
      Promise.all([
        client.invalidateQueries({ queryKey: ["coach"] }),
        client.invalidateQueries({ queryKey: ["dailyBrief"] }),
      ]),
  });

  // A coach that can't load is not worth an error state on the day's first
  // screen — everything else on Today still works.
  if (report.isLoading || report.error || !report.data) return null;

  const { week, nudges } = report.data;

  // With no model, the block is the rules' own cards, individually dismissible,
  // exactly as it was before any of this. `rules` covers both ways that
  // happens — none configured, and one that couldn't be reached — and only the
  // second is worth explaining, since an app with no provider set up is not
  // failing at anything.
  const written = brief.data;
  const fallback = !written || written.source === "rules";
  const standing = fallback ? nudges.filter((n) => !n.dismissed) : [];
  const unreachable = fallback && !!config.data?.provider;

  const speaking = fallback ? standing.length > 0 : !written.dismissed;
  if (!week.rings.length && !speaking) return null;

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
              marginBottom: speaking ? 26 : 0,
            }}
          >
            {week.rings.map((ring) => (
              <Ring key={ring.id} ring={ring} />
            ))}
          </div>
        </>
      )}

      {!fallback && !written.dismissed && (
        <BriefBlock brief={written} onDismiss={() => dismiss.mutate(BRIEF_ID)} />
      )}

      {standing.map((n) => (
        <NudgeCard key={n.id} nudge={n} onDismiss={() => dismiss.mutate(n.id)} />
      ))}

      {/* Said, not hidden — the same rule the Today paragraph follows. An
          unexplained change of voice is how a model outage gets mistaken for
          the app having less to say. */}
      {unreachable && (
        <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 4 }}>
          The model couldn't be reached, so this is the plain reading.
        </div>
      )}
    </section>
  );
}
