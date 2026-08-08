/**
 * Strength sessions, set by set.
 *
 * This screen is shaped by what the watch actually records, which is less than
 * a lifting log and more than nothing. Reps, set durations, rest and order are
 * real measurements. The load is not recorded at all — so there is no volume
 * here, no tonnage, and no per-lift progression, and the screen says so once
 * rather than leaving the absence to be noticed.
 *
 * Exercise names are the watch guessing from wrist motion. They appear only
 * where it was confident and unambiguous, always marked as a guess, and most
 * sets have none.
 */
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  strengthSession,
  strengthSessions,
  type ExerciseSet,
  type StrengthSession,
} from "../lib/api";
import { RefreshButton } from "../components/Refresh";
import { Empty, ErrorNote, Loading, Metric, MetricRow, PageHeader, Unit } from "../components/ui";
import { DASH, num, parseLocal, shortDate } from "../lib/format";

/** `BENCH_PRESS` reads better as `Bench press`. */
function niceExercise(key: string): string {
  const words = key.toLowerCase().replace(/_/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** "06 Aug", or a dash when the cache has no date for the session. */
function sessionDate(date: string | null): string {
  const d = parseLocal(date);
  return d ? shortDate(d) : DASH;
}

function mmss(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = Math.round(seconds % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

/**
 * The set-by-set timeline for one session.
 *
 * Work sets get a bar whose width is the reps and rest gets the gap, so the
 * shape of the session — long rests, a fading rep count — is visible before any
 * of the numbers are read.
 */
function Timeline({ sets }: { sets: ExerciseSet[] }) {
  const work = sets.filter((s) => s.active);
  const maxReps = Math.max(...work.map((s) => s.reps ?? 0), 1);

  return (
    <div style={{ marginTop: 16 }}>
      {sets.map((s) => {
        if (!s.active) {
          return (
            <div
              key={s.setIndex}
              style={{
                fontSize: "var(--fs-micro)",
                color: "var(--faint)",
                padding: "3px 0 3px 2px",
              }}
            >
              rest {mmss(s.durationS ?? 0)}
            </div>
          );
        }
        const reps = s.reps ?? 0;
        return (
          <div
            key={s.setIndex}
            style={{ display: "flex", alignItems: "center", gap: 10, padding: "3px 0" }}
          >
            <div
              style={{
                height: 14,
                width: `${Math.max((reps / maxReps) * 100, 4)}%`,
                maxWidth: 220,
                background: "var(--fg)",
                opacity: 0.82,
                borderRadius: 2,
              }}
            />
            <span className="mono" style={{ fontSize: "var(--fs-caption)" }}>
              {reps} reps
            </span>
            <span style={{ fontSize: "var(--fs-micro)", color: "var(--faint)" }}>
              {mmss(s.durationS ?? 0)}
              {s.exercise && (
                <>
                  {" · "}
                  <span title={`The watch was ${Math.round(s.exerciseConfidence ?? 0)}% sure`}>
                    {niceExercise(s.exercise)}?
                  </span>
                </>
              )}
            </span>
          </div>
        );
      })}
    </div>
  );
}

function SessionRow({ session }: { session: StrengthSession }) {
  const [open, setOpen] = useState(false);
  const detail = useQuery({
    queryKey: ["strengthSession", session.activityId],
    queryFn: () => strengthSession(session.activityId),
    enabled: open,
  });

  return (
    <div style={{ borderTop: "1px solid var(--line2)", padding: "16px 0" }}>
      <button
        type="button"
        className="action"
        onClick={() => setOpen((o) => !o)}
        style={{ display: "block", width: "100%", textAlign: "left" }}
      >
        <div style={{ display: "flex", justifyContent: "space-between", gap: 16 }}>
          <span style={{ fontSize: "var(--fs-md)" }}>{sessionDate(session.date)}</span>
          <span className="mono" style={{ fontSize: "var(--fs-caption)", color: "var(--mut)" }}>
            {session.workSets} sets · {session.totalReps} reps ·{" "}
            {session.medianRestS ? `${mmss(session.medianRestS)} rest` : DASH}
          </span>
        </div>
        {session.guessedExercises.length > 0 && (
          <div style={{ fontSize: "var(--fs-micro)", color: "var(--faint)", marginTop: 4 }}>
            probably {session.guessedExercises.map((e) => niceExercise(e.exercise)).join(", ")}
            {session.unlabelledSets > 0 && ` · ${session.unlabelledSets} unidentified`}
          </div>
        )}
      </button>

      {open && (
        <>
          {detail.isLoading && <Loading label="Reading the sets" />}
          {detail.data && <Timeline sets={detail.data[1]} />}
          {detail.data === null && (
            <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 10 }}>
              The sets for this one haven't been synced yet.
            </div>
          )}
        </>
      )}
    </div>
  );
}

export function Strength() {
  const report = useQuery({ queryKey: ["strength", 30], queryFn: () => strengthSessions(30) });

  if (report.isLoading) return <Loading />;
  if (report.error) return <ErrorNote error={report.error} />;

  const data = report.data;
  if (!data || !data.sessions.length) {
    return (
      <div className="screen">
        <PageHeader eyebrow="Strength" title="Lifting" action={<RefreshButton />} />
        <Empty
          title="No strength sessions with set data yet."
          body={
            <>
              Sets arrive with a sync, and only for sessions the watch recorded as strength
              training. If you've lifted recently and this is still empty, the sync hasn't reached
              those sessions — it fetches them oldest-backlog-first, a batch at a time.
            </>
          }
        />
      </div>
    );
  }

  return (
    <div className="screen">
      <PageHeader
        eyebrow="Strength"
        title="Lifting"
        lede={
          <>
            Reps, time under tension and rest, across {data.sessionsExamined}{" "}
            {data.sessionsExamined === 1 ? "session" : "sessions"}. The watch doesn't record the
            weight on the bar, so there's no volume here and no progression by load — what it does
            record well is how much work you did and how long you rested between it.
          </>
        }
        action={<RefreshButton />}
      />

      <MetricRow>
        <Metric
          label="Sets per session"
          value={data.avgWorkSets != null ? num(data.avgWorkSets) : DASH}
        />
        <Metric label="Reps per session" value={data.avgReps != null ? num(data.avgReps) : DASH} />
        <Metric
          label="Typical rest"
          value={
            data.medianRestS != null ? (
              <>
                {mmss(data.medianRestS)}
                <Unit>min</Unit>
              </>
            ) : (
              DASH
            )
          }
        />
        <Metric
          label="Sets identified"
          value={
            <>
              {Math.round(
                (data.labelledSets / Math.max(data.labelledSets + data.unlabelledSets, 1)) * 100,
              )}
              <Unit>%</Unit>
            </>
          }
        />
      </MetricRow>

      <div
        style={{
          fontSize: "var(--fs-caption)",
          color: "var(--faint)",
          margin: "18px 0 6px",
          lineHeight: 1.5,
        }}
      >
        Exercise names are the watch's guess from wrist motion, shown with a question mark and only
        where it was confident and not torn between two movements. The rest are left blank rather
        than guessed at.
      </div>

      <div style={{ marginTop: 20 }}>
        {data.sessions.map((s) => (
          <SessionRow key={s.activityId} session={s} />
        ))}
      </div>
    </div>
  );
}
