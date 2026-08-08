import { useState } from "react";
import {
  createWorkout,
  type DraftStep,
  type EndCondition,
  type ExecStep,
  type StepKind,
  type StepTarget,
  type WorkoutDraft,
} from "../lib/api";
import { DeleteIcon, DoneIcon, NewIcon, SpinnerIcon } from "../lib/icons";
import { duration, sportLabel } from "../lib/format";

/**
 * A workout the model proposed, laid out as something you can change and then
 * send.
 *
 * This is the whole reason `draft_workout` doesn't write anything. The model
 * produces a structure; the athlete reads it, fixes the two things it got
 * wrong, and presses a button — and that press is what reaches Garmin. Nothing
 * in the chat path can save a workout, so "it suggested a session" and "a
 * session appeared on my watch" stay separate events.
 *
 * The editor is deliberately a purpose-built form rather than a generic one
 * rendered from a schema. A workout is a short list of steps with four fields
 * each and one level of grouping; a form that knows that can put a duration
 * next to its unit and a zone next to its meaning, and a form derived from JSON
 * cannot.
 */
export function WorkoutCard({
  draft,
  onSaved,
}: {
  draft: WorkoutDraft;
  /** Called with the new Garmin id, so the conversation can record it. */
  onSaved: (workoutId: number) => void;
}) {
  // The draft is copied into local state on mount: everything below edits it
  // freely, and the version in the conversation only changes if it's sent.
  const [w, setW] = useState<WorkoutDraft>(draft);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Held here as well as read from the prop. `onSaved` writes the id into the
  // conversation, and the conversation is where it survives a reload — but a
  // card offered mid-turn has no message to be written into yet, and one
  // sent workout must never leave a live button behind it.
  const [sent, setSent] = useState(false);

  const saved = sent || draft.savedWorkoutId != null;
  const total = totalSeconds(w.steps);

  async function send() {
    if (busy || saved) return;
    setBusy(true);
    setError(null);
    try {
      const workoutId = await createWorkout(w);
      setSent(true);
      onSaved(workoutId);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  /** Replace one top-level step, leaving the rest alone. */
  const put = (i: number, step: DraftStep) =>
    setW({ ...w, steps: w.steps.map((s, j) => (j === i ? step : s)) });

  const drop = (i: number) => setW({ ...w, steps: w.steps.filter((_, j) => j !== i) });

  return (
    <div
      style={{
        border: "1px solid var(--line)",
        borderRadius: 6,
        padding: "18px 20px 20px",
        marginTop: 20,
        maxWidth: "62ch",
      }}
    >
      <div className="section-head" style={{ marginBottom: 12 }}>
        <div className="eyebrow">{saved ? "Sent to Garmin" : "Proposed workout"}</div>
        <div className="mono" style={{ fontSize: "var(--fs-caption)", color: "var(--faint)" }}>
          {[
            sportLabel(w.sport),
            total != null ? duration(total) : null,
            `${flatCount(w.steps)} steps`,
          ]
            .filter(Boolean)
            .join(" · ")}
        </div>
      </div>

      <input
        className="input-bare"
        style={{ fontSize: 23, marginBottom: w.description ? 8 : 18 }}
        value={w.name}
        disabled={saved}
        aria-label="Workout name"
        onChange={(e) => setW({ ...w, name: e.target.value })}
      />
      {w.description && (
        <div
          style={{
            fontSize: "var(--fs-small)",
            color: "var(--mut)",
            margin: "10px 0 18px",
            maxWidth: "52ch",
          }}
        >
          {w.description}
        </div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
        {w.steps.map((step, i) =>
          step.type === "repeat" ? (
            <RepeatRow
              key={i}
              step={step}
              locked={saved}
              onChange={(s) => put(i, s)}
              onRemove={() => drop(i)}
            />
          ) : (
            <StepRow
              key={i}
              step={step}
              locked={saved}
              onChange={(s) => put(i, { type: "exec", ...s })}
              onRemove={() => drop(i)}
            />
          ),
        )}
      </div>

      {!saved && (
        <div style={{ display: "flex", gap: 18, marginTop: 14 }}>
          <AddButton
            label="Step"
            onClick={() => setW({ ...w, steps: [...w.steps, { type: "exec", ...blank() }] })}
          />
          <AddButton
            label="Repeat"
            onClick={() =>
              setW({
                ...w,
                steps: [...w.steps, { type: "repeat", times: 4, steps: [blank()] }],
              })
            }
          />
        </div>
      )}

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 14,
          marginTop: 22,
          paddingTop: 18,
          borderTop: "1px solid var(--line2)",
        }}
      >
        {saved ? (
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 7,
              fontSize: "var(--fs-small)",
              color: "var(--mut)",
            }}
          >
            <DoneIcon size={14} style={{ flex: "none" }} aria-hidden />
            Saved to your Garmin account.
          </span>
        ) : (
          <button
            className="cta"
            onClick={() => void send()}
            disabled={busy || !w.name.trim() || w.steps.length === 0}
            style={{ display: "inline-flex", alignItems: "center", gap: 8 }}
          >
            {busy && (
              <SpinnerIcon size={14} className="spin" style={{ flex: "none" }} aria-hidden />
            )}
            {busy ? "Sending" : "Send to Garmin"}
          </button>
        )}
        {!saved && (
          <span style={{ fontSize: "var(--fs-caption)", color: "var(--faint)" }}>
            Nothing is saved until you press this.
          </span>
        )}
      </div>

      {/* Garmin's own rejection, verbatim. It names the field it didn't like,
          which is the only thing that makes a bad payload fixable. */}
      {error && (
        <div
          style={{
            fontSize: "var(--fs-small)",
            color: "var(--acc)",
            marginTop: 12,
            maxWidth: "52ch",
          }}
        >
          {error}
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ rows --- */

/** One executable step: what it is, how long, and what to hold. */
function StepRow({
  step,
  locked,
  nested = false,
  onChange,
  onRemove,
}: {
  step: ExecStep;
  locked: boolean;
  nested?: boolean;
  onChange: (s: ExecStep) => void;
  onRemove: () => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "7px 0",
        borderBottom: nested ? "none" : "1px solid var(--line2)",
      }}
    >
      <Select
        value={step.kind}
        disabled={locked}
        aria-label="Step type"
        width={104}
        options={[
          ["warmup", "Warm up"],
          ["interval", "Interval"],
          ["recovery", "Recovery"],
          ["rest", "Rest"],
          ["cooldown", "Cool down"],
        ]}
        onChange={(v) => onChange({ ...step, kind: v as StepKind })}
      />

      <EndField end={step.end} locked={locked} onChange={(end) => onChange({ ...step, end })} />

      <TargetField
        target={step.target ?? { type: "none" }}
        locked={locked}
        onChange={(target) => onChange({ ...step, target })}
      />

      {/* The note is what a coach would actually say, so it gets the leftover
          width rather than a fixed box. */}
      <input
        className="input"
        style={{ flex: 1, minWidth: 60, fontSize: "var(--fs-small)", padding: "5px 8px" }}
        value={step.note ?? ""}
        disabled={locked}
        placeholder="note"
        aria-label="Step note"
        onChange={(e) => onChange({ ...step, note: e.target.value || undefined })}
      />

      <IconButton label="Remove this step" onClick={onRemove} hidden={locked}>
        <DeleteIcon size={13} aria-hidden />
      </IconButton>
    </div>
  );
}

/**
 * A repeated block: the count, then its steps indented under it.
 *
 * The rule against nesting is enforced in Rust, and shows up here as the
 * absence of a "Repeat" button inside a repeat — there is no way to build one
 * that the backend would then reject.
 */
function RepeatRow({
  step,
  locked,
  onChange,
  onRemove,
}: {
  step: { type: "repeat"; times: number; steps: ExecStep[] };
  locked: boolean;
  onChange: (s: DraftStep) => void;
  onRemove: () => void;
}) {
  const put = (i: number, s: ExecStep) =>
    onChange({ ...step, steps: step.steps.map((x, j) => (j === i ? s : x)) });

  return (
    <div style={{ borderBottom: "1px solid var(--line2)", padding: "7px 0" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ fontSize: "var(--fs-small)", color: "var(--mut)" }}>Repeat</span>
        <Num
          value={step.times}
          disabled={locked}
          aria-label="Repeat count"
          width={44}
          min={2}
          onChange={(v) => onChange({ ...step, times: Math.max(2, Math.round(v)) })}
        />
        <span style={{ fontSize: "var(--fs-small)", color: "var(--mut)", flex: 1 }}>×</span>
        <IconButton label="Remove this repeat" onClick={onRemove} hidden={locked}>
          <DeleteIcon size={13} aria-hidden />
        </IconButton>
      </div>

      <div style={{ marginLeft: 14, paddingLeft: 14, borderLeft: "1px solid var(--line)" }}>
        {step.steps.map((s, i) => (
          <StepRow
            key={i}
            step={s}
            locked={locked}
            nested
            onChange={(next) => put(i, next)}
            onRemove={() => onChange({ ...step, steps: step.steps.filter((_, j) => j !== i) })}
          />
        ))}
        {!locked && (
          <div style={{ padding: "4px 0 6px" }}>
            <AddButton
              label="Step"
              onClick={() => onChange({ ...step, steps: [...step.steps, blank()] })}
            />
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * When the step ends: the condition, and the number it needs.
 *
 * Time is entered in minutes and stored in seconds. Seconds are the unit
 * Garmin takes and minutes are the unit intervals are spoken in — typing 180
 * for three minutes is a small papercut on every single row.
 */
function EndField({
  end,
  locked,
  onChange,
}: {
  end: EndCondition;
  locked: boolean;
  onChange: (e: EndCondition) => void;
}) {
  return (
    <>
      <Select
        value={end.type}
        disabled={locked}
        aria-label="Ends after"
        width={78}
        options={[
          ["time", "for"],
          ["distance", "for"],
          ["lap_button", "lap press"],
        ]}
        onChange={(v) =>
          onChange(
            v === "time"
              ? { type: "time", seconds: 300 }
              : v === "distance"
                ? { type: "distance", metres: 1000 }
                : { type: "lap_button" },
          )
        }
      />
      {end.type === "time" && (
        <Unit suffix="min">
          <Num
            value={round(end.seconds / 60, 2)}
            disabled={locked}
            aria-label="Minutes"
            width={48}
            min={0}
            step={0.5}
            onChange={(v) => onChange({ type: "time", seconds: Math.round(v * 60) })}
          />
        </Unit>
      )}
      {end.type === "distance" && (
        <Unit suffix="m">
          <Num
            value={end.metres}
            disabled={locked}
            aria-label="Metres"
            width={62}
            min={0}
            step={50}
            onChange={(v) => onChange({ type: "distance", metres: Math.round(v) })}
          />
        </Unit>
      )}
    </>
  );
}

/**
 * What to hold: a zone, an explicit bpm range, or nothing.
 *
 * Zones are listed with their purpose rather than as bare numbers — "Z2 easy"
 * is the thing being chosen, and the whole point of this athlete's plan is
 * knowing which one that is. The bpm ranges alongside them are deliberately
 * absent: they live on the Garmin account and this app would be quoting a copy.
 */
function TargetField({
  target,
  locked,
  onChange,
}: {
  target: StepTarget;
  locked: boolean;
  onChange: (t: StepTarget) => void;
}) {
  const value = target.type === "hr_zone" ? `z${target.zone}` : target.type;

  return (
    <>
      <Select
        value={value}
        disabled={locked}
        aria-label="Target"
        width={98}
        options={[
          ["none", "no target"],
          ["z1", "Z1 recovery"],
          ["z2", "Z2 easy"],
          ["z3", "Z3 tempo"],
          ["z4", "Z4 threshold"],
          ["z5", "Z5 max"],
          ["bpm", "bpm range"],
        ]}
        onChange={(v) =>
          onChange(
            v === "none"
              ? { type: "none" }
              : v === "bpm"
                ? { type: "bpm", low: 120, high: 140 }
                : { type: "hr_zone", zone: Number(v.slice(1)) },
          )
        }
      />
      {target.type === "bpm" && (
        <Unit suffix="bpm">
          <Num
            value={target.low}
            disabled={locked}
            aria-label="Lowest bpm"
            width={44}
            onChange={(low) => onChange({ ...target, low: Math.round(low) })}
          />
          <span style={{ color: "var(--faint)", fontSize: "var(--fs-caption)" }}>–</span>
          <Num
            value={target.high}
            disabled={locked}
            aria-label="Highest bpm"
            width={44}
            onChange={(high) => onChange({ ...target, high: Math.round(high) })}
          />
        </Unit>
      )}
    </>
  );
}

/* --------------------------------------------------------------- controls --- */

/** The app has no select styling of its own; this is `.input` at row scale. */
function Select({
  value,
  options,
  width,
  disabled,
  onChange,
  ...rest
}: {
  value: string;
  options: [string, string][];
  width: number;
  disabled?: boolean;
  onChange: (v: string) => void;
  "aria-label": string;
}) {
  return (
    <select
      {...rest}
      className="input"
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
      style={{
        width,
        flex: "none",
        fontSize: "var(--fs-small)",
        padding: "5px 6px",
        // Native select arrows are a platform grey box in the middle of all
        // this paper; the value alone reads as the editorial rest of the app.
        appearance: "none",
        cursor: disabled ? "default" : "pointer",
      }}
    >
      {options.map(([v, label]) => (
        <option key={v} value={v}>
          {label}
        </option>
      ))}
    </select>
  );
}

function Num({
  value,
  width,
  disabled,
  min,
  step,
  onChange,
  ...rest
}: {
  value: number;
  width: number;
  disabled?: boolean;
  min?: number;
  step?: number;
  onChange: (v: number) => void;
  "aria-label": string;
}) {
  return (
    <input
      {...rest}
      className="input mono"
      type="number"
      value={value}
      disabled={disabled}
      min={min}
      step={step}
      // A half-typed number is NaN for one keystroke; ignoring it leaves the
      // field showing the last good value instead of collapsing to zero.
      onChange={(e) => {
        const v = e.target.valueAsNumber;
        if (!Number.isNaN(v)) onChange(v);
      }}
      style={{
        width,
        flex: "none",
        fontSize: "var(--fs-small)",
        padding: "5px 7px",
        textAlign: "right",
      }}
    />
  );
}

/** A number and its unit, kept together so the unit never wraps away from it. */
function Unit({ suffix, children }: { suffix: string; children: React.ReactNode }) {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 5, flex: "none" }}>
      {children}
      <span style={{ fontSize: "var(--fs-caption)", color: "var(--faint)" }}>{suffix}</span>
    </span>
  );
}

function AddButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      className="quiet"
      onClick={onClick}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 5,
        fontSize: "var(--fs-caption)",
      }}
    >
      <NewIcon size={12} style={{ flex: "none" }} aria-hidden />
      {label}
    </button>
  );
}

/** The slot stays in the layout when hidden, so rows keep a straight edge. */
function IconButton({
  label,
  hidden,
  onClick,
  children,
}: {
  label: string;
  hidden: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      className="quiet"
      title={label}
      aria-label={label}
      onClick={onClick}
      style={{
        flex: "none",
        display: "grid",
        placeItems: "center",
        width: 20,
        color: "var(--faint)",
        visibility: hidden ? "hidden" : "visible",
      }}
    >
      {children}
    </button>
  );
}

/* ------------------------------------------------------------------ maths --- */

/** A new step, shaped like the most common one someone adds. */
const blank = (): ExecStep => ({
  kind: "interval",
  end: { type: "time", seconds: 300 },
  target: { type: "none" },
});

const flatCount = (steps: DraftStep[]): number =>
  steps.reduce((n, s) => n + (s.type === "repeat" ? s.times * s.steps.length : 1), 0);

/**
 * Total seconds, or null as soon as one step isn't measured in time.
 *
 * Same rule as the Rust side: a workout with a distance step has no duration
 * until it's run, and inferring one from an assumed pace would put a number on
 * the card that nothing measured.
 */
function totalSeconds(steps: DraftStep[]): number | null {
  let total = 0;
  for (const s of steps) {
    if (s.type === "repeat") {
      const inner = totalSeconds(s.steps.map((e) => ({ type: "exec", ...e }) as DraftStep));
      if (inner == null) return null;
      total += inner * s.times;
    } else {
      if (s.end.type !== "time") return null;
      total += s.end.seconds;
    }
  }
  return total;
}

const round = (n: number, places: number) => Math.round(n * 10 ** places) / 10 ** places;
