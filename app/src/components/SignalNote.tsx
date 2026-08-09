/**
 * What a number rests on, said next to the number.
 *
 * The app reports zone splits to a decimal place, and a wrist-worn sensor does
 * not always earn that. Three regimes produce a figure that is confidently
 * wrong rather than missing — the sensor locking onto arm swing and reporting
 * step rate as pulse, optical lag flattening the peaks of a short hard effort,
 * and indoor pace estimated from arm movement. `garmin-core`'s `signal` module
 * detects them; this says so on the page.
 *
 * Deliberately not an error and not a warning triangle. Nothing has gone wrong
 * and nothing is being withheld — the session keeps its numbers and stays in
 * every total. This is the footnote that makes the difference between a figure
 * you can lean on and one you can't, and the athlete is owed it in the same
 * breath as the figure rather than in a settings screen.
 *
 * `poor` gets the accent because it is the case where the number should not
 * carry an argument on its own. `caution` stays quiet in muted text: said too
 * loudly it would train the eye to skip it, and most sessions have one.
 */
import type { HrConfidence } from "../lib/api";

export function SignalNote({ confidence }: { confidence: HrConfidence }) {
  if (confidence.level === "good" || confidence.notes.length === 0) return null;

  const poor = confidence.level === "poor";

  return (
    <div
      style={{
        margin: "14px 0 4px",
        paddingLeft: 11,
        borderLeft: `2px solid ${poor ? "var(--acc)" : "var(--line)"}`,
        maxWidth: "58ch",
      }}
    >
      <div
        className="eyebrow"
        style={{ color: poor ? "var(--acc)" : "var(--faint)", marginBottom: 5 }}
      >
        {poor ? "Read this split with care" : "Worth knowing"}
      </div>
      {confidence.notes.map((note) => (
        <p
          key={note}
          style={{
            fontSize: "var(--fs-small)",
            color: "var(--mut)",
            lineHeight: 1.55,
            margin: "0 0 6px",
          }}
        >
          {note}
        </p>
      ))}
    </div>
  );
}

/**
 * The other half: the app's own reading of the trace against Garmin's totals.
 *
 * Two independent answers to one question, and until recently nothing compared
 * them. They should agree to within rounding; when they don't, neither belongs
 * in a confident sentence, and saying which two numbers disagree is more use
 * than picking one and hoping.
 */
export function ZoneDisagreement({ maxPct }: { maxPct: number | null }) {
  // Sampling gaps and boundary rounding put a point or two between the two
  // readings on a perfectly good session. Ten is where it stops being that.
  if (maxPct == null || maxPct < 10) return null;

  return (
    <p
      style={{
        fontSize: "var(--fs-caption)",
        color: "var(--faint)",
        margin: "10px 0 0",
        maxWidth: "58ch",
        lineHeight: 1.55,
      }}
    >
      Garmin's zone totals and this app's own reading of the heart-rate trace differ by up to{" "}
      {maxPct.toFixed(0)} percentage points on one zone. That is more than rounding, so treat the
      split above as approximate.
    </p>
  );
}
