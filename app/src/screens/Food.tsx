/**
 * Intake against expenditure.
 *
 * Garmin carries the eaten side on the same daily summary that already feeds
 * the Health screen — whatever food log the account is connected to writes
 * `consumedKilocalories` there. Days with no log come back null, and that
 * distinction drives this whole screen: a day nobody logged is blank, never
 * zero, because a zero would read as a day of fasting and a balance computed
 * against it would be pure fiction.
 */
import { useQuery } from "@tanstack/react-query";
import { nutrition, type NutritionDay } from "../lib/api";
import { hasData, type Point } from "../lib/chart";
import {
  AxisLabels,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  Metric,
  MetricRow,
  PageTitle,
  Rule,
} from "../components/ui";
import { DASH, num, parseLocal, shortDate } from "../lib/format";

const DAYS = 30;

/** Signed kcal, with the sign carrying the meaning rather than a colour alone. */
function balance(v: number | null): string {
  if (v == null) return DASH;
  const r = Math.round(v);
  return r > 0 ? `+${num(r)}` : num(r);
}

function litres(ml: number | null | undefined): string {
  return ml == null ? DASH : `${(ml / 1000).toFixed(2)} L`;
}

function dayLabel(d: NutritionDay | undefined): string {
  const parsed = d?.date ? parseLocal(d.date) : null;
  return parsed ? shortDate(parsed) : "";
}

export function Food() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["nutrition", DAYS],
    queryFn: () => nutrition(DAYS),
  });

  if (isLoading) return <Loading />;
  if (error) return <ErrorNote error={error} />;

  const report = data;
  const days = report?.days ?? [];
  // Oldest-first for anything drawn left to right.
  const chrono = [...days].reverse();

  if (!report || report.daysLogged === 0) {
    return (
      <div>
        <PageTitle>Food</PageTitle>
        <Lede />
        <Empty
          title="No food logged in the last 30 days."
          body={
            <>
              Garmin returns the eaten side of this screen only when something
              writes it — the Connect app's own log, or an integration like
              MyFitnessPal. The burn side is already here from your device;
              it's intake that's missing. Log a day in Garmin Connect and it
              appears here on the next sync.
            </>
          }
        />
      </div>
    );
  }

  const logged = days.filter((d) => d.logged);
  const newest = logged[0];
  const consumed: Point[] = chrono.map((d) => d.consumedKcal);
  const burned: Point[] = chrono.map((d) => d.totalBurnKcal);
  const goal = newest?.netCalorieGoal ?? null;

  return (
    <div>
      <PageTitle>Food</PageTitle>
      <Lede />

      <MetricRow style={{ marginBottom: 8 }}>
        <Metric
          value={report.avgConsumedKcal != null ? num(Math.round(report.avgConsumedKcal)) : DASH}
          label="Avg eaten"
        />
        <Metric
          value={report.avgBurnKcal != null ? num(Math.round(report.avgBurnKcal)) : DASH}
          label="Avg burned"
        />
        <Metric
          value={balance(report.avgBalanceKcal)}
          label="Avg balance"
          accent={(report.avgBalanceKcal ?? 0) > 0}
        />
      </MetricRow>
      <p style={{ fontSize: 13, color: "var(--faint)", margin: "0 0 8px", maxWidth: "60ch" }}>
        Averaged over the {report.daysLogged}{" "}
        {report.daysLogged === 1 ? "day" : "days"} with a food log, not over all{" "}
        {days.length}. Balance is eaten minus burned — negative is a deficit.
        {goal != null && ` Your Garmin net calorie goal is ${num(goal)}.`}
      </p>

      {hasData(consumed) && (
        <>
          <Rule m="44px 0 20px" />
          <div className="eyebrow" style={{ marginBottom: 16 }}>
            Eaten against burned
          </div>
          {/* Shared scale: the gap between the two lines is the entire point,
              so scaling them independently would draw a false story. */}
          <LineChart
            shareScale
            series={[
              { values: consumed, stroke: "var(--acc)", width: 1.6 },
              { values: burned, stroke: "var(--mut)", width: 1.2, dashed: true },
            ]}
          />
          <AxisLabels labels={[dayLabel(chrono[0]), dayLabel(chrono[chrono.length - 1])]} />
          <div style={{ display: "flex", gap: 20, fontSize: 12, color: "var(--faint)", marginTop: 10 }}>
            <span style={{ color: "var(--acc)" }}>— Eaten</span>
            <span>‑ ‑ Burned</span>
          </div>
        </>
      )}

      <Rule m="46px 0 20px" />
      <div className="eyebrow" style={{ marginBottom: 6 }}>
        By day
      </div>
      <div>
        {days.map((d) => (
          <DayRow key={d.date} day={d} />
        ))}
      </div>
    </div>
  );
}

function Lede() {
  return (
    <p
      style={{
        fontSize: 16,
        lineHeight: 1.7,
        color: "var(--mut)",
        margin: "0 0 44px",
        maxWidth: "62ch",
        textWrap: "pretty",
      }}
    >
      Intake against expenditure, matched to what Garmin says you burned.
    </p>
  );
}

function DayRow({ day }: { day: NutritionDay }) {
  const d = parseLocal(day.date);
  const deficit = (day.balanceKcal ?? 0) < 0;

  return (
    <div className="row-static" style={{ justifyContent: "space-between" }}>
      <span style={{ width: 96, flex: "none", color: day.logged ? undefined : "var(--faint)" }}>
        {d ? shortDate(d) : day.date}
      </span>

      {day.logged ? (
        <>
          <span className="mono" style={{ width: 74, textAlign: "right" }}>
            {num(day.consumedKcal)}
          </span>
          <span className="mono" style={{ width: 74, textAlign: "right", color: "var(--mut)" }}>
            {num(day.totalBurnKcal)}
          </span>
          <span
            className="mono"
            style={{
              width: 82,
              textAlign: "right",
              color: deficit ? "var(--mut)" : "var(--acc)",
            }}
          >
            {balance(day.balanceKcal)}
          </span>
        </>
      ) : (
        // Burn is still known on unlogged days; showing it makes clear that
        // it's the food log that's missing, not the whole day.
        <>
          <span style={{ flex: 1, color: "var(--faint)", fontSize: 13 }}>
            no food logged
          </span>
          <span className="mono" style={{ width: 74, textAlign: "right", color: "var(--faint)" }}>
            {num(day.totalBurnKcal)}
          </span>
          <span style={{ width: 82 }} />
        </>
      )}

      <span
        className="mono"
        style={{ width: 78, textAlign: "right", color: "var(--faint)", fontSize: 13 }}
        title={day.sweatLossMl != null ? `${num(day.sweatLossMl)} ml sweat loss` : undefined}
      >
        {litres(day.hydrationMl)}
      </span>
    </div>
  );
}
