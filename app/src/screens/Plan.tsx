/**
 * The plan, as far as Garmin actually holds one.
 *
 * There is no training plan and no goal race on this account — those endpoints
 * come back empty — so the honest version of this screen is the structured
 * workouts the athlete has actually built, set against what they've been doing.
 * The gap between "a Z2 workout exists" and "it gets run" is the useful thing
 * here, so the screen measures exactly that rather than rendering a calendar
 * nobody filled in.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { cachedActivities, workouts, type Workout } from "../lib/api";
import {
  Empty,
  ErrorNote,
  Loading,
  Metric,
  MetricRow,
  PageHeader,
} from "../components/ui";
import { RefreshButton } from "../components/Refresh";
import { DASH, duration, isRun, km, sportLabel } from "../lib/format";

/** Window used to ask "is this workout actually being run?". */
const LOOKBACK = 28;

export function Plan() {
  const plan = useQuery({ queryKey: ["workouts"], queryFn: workouts });
  const acts = useQuery({
    queryKey: ["activities", 120],
    queryFn: () => cachedActivities(120),
  });

  if (plan.isLoading || acts.isLoading) return <Loading />;
  if (plan.error) return <ErrorNote error={plan.error} />;

  const saved = plan.data ?? [];
  const all = acts.data ?? [];

  if (!saved.length) {
    return (
      <div>
        <Header />
        <Empty
          title="No workouts saved on your Garmin account."
          body={
            <>
              Garmin holds no training plan and no goal race for you, so a plan
              can only come from the structured workouts you build in Connect.
              There aren't any yet. Create one and it appears here on the next
              sync.
            </>
          }
        />
      </div>
    );
  }

  // Runs in the recent window, used to judge follow-through.
  const cutoff = new Date();
  cutoff.setDate(cutoff.getDate() - LOOKBACK);
  const recent = all.filter((a) => {
    const d = a.localDate ? new Date(a.localDate) : null;
    return d != null && d >= cutoff;
  });
  const runs = recent.filter((a) => isRun(a.typeKey));
  const longestRun = runs.reduce<number>((m, a) => Math.max(m, a.durationS ?? 0), 0);

  const runWorkouts = saved.filter((w) => isRun(w.sportType));
  const target = runWorkouts.reduce<number>(
    (m, w) => Math.max(m, w.estDurationS ?? 0),
    0,
  );

  return (
    <div className="screen">
      <Header />

      <MetricRow style={{ marginBottom: 10 }}>
        <Metric value={saved.length} label="Saved workouts" />
        <Metric value={runs.length} label={`Runs in ${LOOKBACK} days`} />
        <Metric
          value={longestRun ? duration(longestRun) : DASH}
          label="Longest recent run"
        />
      </MetricRow>

      {target > 0 && (
        <p style={{ fontSize: "var(--fs-md)", lineHeight: 1.7, color: "var(--mut)", margin: "0 0 8px", maxWidth: "62ch", textWrap: "pretty" }}>
          Your longest running workout asks for {duration(target)}.{" "}
          {longestRun >= target ? (
            <>
              Your longest run in the last {LOOKBACK} days was{" "}
              {duration(longestRun)} — you're covering it.
            </>
          ) : longestRun > 0 ? (
            <>
              Your longest run in the last {LOOKBACK} days was{" "}
              {duration(longestRun)}, which is {Math.round((1 - longestRun / target) * 100)}%
              short of it. That gap is the plan.
            </>
          ) : (
            <>Nothing in the last {LOOKBACK} days has been a run.</>
          )}
        </p>
      )}

      <div className="eyebrow" style={{ margin: "66px 0 6px" }}>
        Saved workouts
      </div>
      <div>
        {saved.map((w) => (
          <WorkoutRow key={w.workoutId} workout={w} />
        ))}
      </div>

      <div className="eyebrow" style={{ margin: "66px 0 14px" }}>
        What Garmin doesn't have
      </div>
      <p style={{ fontSize: "var(--fs-md)", lineHeight: 1.7, color: "var(--mut)", margin: 0, maxWidth: "62ch", textWrap: "pretty" }}>
        No goal race and no structured week are stored on your account — the
        training-plan and goal endpoints both return nothing. Those are
        decisions rather than data, so the app won't invent them. Everything
        above is what Garmin genuinely holds.{" "}
        <Link to="/activities">Browse activities</Link> for what you've actually
        done.
      </p>
    </div>
  );
}

function Header() {
  return (
    <PageHeader
      eyebrow={`Last ${LOOKBACK} days`}
      title="Plan"
      lede="The workouts you've built, and whether your running is keeping up with them."
      action={<RefreshButton />}
      space={46}
    />
  );
}

function WorkoutRow({ workout: w }: { workout: Workout }) {
  return (
    <div className="row-static" style={{ justifyContent: "space-between", gap: 18 }}>
      <span style={{ flex: 1, minWidth: 0 }}>
        <span style={{ display: "block" }}>{w.name ?? "Untitled workout"}</span>
        {w.description && (
          <span style={{ display: "block", fontSize: "var(--fs-small)", color: "var(--faint)", marginTop: 3 }}>
            {w.description}
          </span>
        )}
      </span>
      <span style={{ width: 118, flex: "none", color: "var(--mut)", fontSize: "var(--fs-small)" }}>
        {sportLabel(w.sportType)}
      </span>
      <span className="mono" style={{ width: 82, flex: "none", textAlign: "right" }}>
        {/* Strength workouts carry a zero estimate rather than a real one. */}
        {w.estDurationS ? duration(w.estDurationS) : w.estDistanceM ? km(w.estDistanceM) : DASH}
      </span>
    </div>
  );
}
