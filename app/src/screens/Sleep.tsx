/**
 * Last night, in full.
 *
 * Sleep already appears twice in this app — as a line on Health and as one
 * input to the recovery reading on Today — and both are the same two numbers:
 * how long, and the score. That is the right amount for a chart of the year and
 * far too little for the question actually being asked most mornings, which is
 * "what happened last night, and is it a problem".
 *
 * So this screen is built the other way round from every other one here. It
 * opens on a single night at full resolution — the hypnogram, the stage mix
 * against Garmin's own target bands, the overnight heart rate — and only then
 * widens to the window behind it. The window's job is context for the night,
 * not a trend in its own right; Health is where trends live.
 *
 * It stops at what the data says. The insights near the end are derived from
 * these rows — computed in `garmin-core`, so the coach on Ask can make the same
 * claims — and where a screen like this would usually print a list of general
 * sleep advice, it hands the question to Ask instead. That advice is the same
 * for everybody and stale on second reading; the coach can give it about last
 * night specifically, which is the only version worth having.
 */
import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import {
  sleep as sleepReport,
  type ScorePart,
  type SleepInsight,
  type SleepNight,
  type SleepStage,
  type StageSlice,
} from "../lib/api";
import { RefreshButton } from "../components/Refresh";
import { ScreenActions } from "../components/Share";
import {
  ArrowRight,
  AxisLabels,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  Metric,
  MetricRow,
  PageHeader,
  Rule,
} from "../components/ui";
import { DASH, hoursMinutes, longDate, num, parseLocal } from "../lib/format";
import { IS_MOBILE } from "../lib/platform";

const RANGES = [
  { days: 7, label: "7 nights" },
  { days: 30, label: "30 nights" },
  { days: 90, label: "90 nights" },
] as const;

/**
 * The four stages, top to bottom, and how each is drawn.
 *
 * Deep at the bottom and awake at the top is the convention every sleep chart
 * uses, and it isn't arbitrary: the night reads as a descent, so the shape of
 * the first three hours — the ones that carry deep sleep — is visible as a dip
 * rather than as a colour you have to look up.
 *
 * The colour ladder runs the other way from the zone bar's on purpose. There,
 * accent means hard; here it means deep, which is the part you want more of.
 */
const STAGES: { key: SleepStage; label: string; fill: string; lane: number }[] = [
  { key: "awake", label: "Awake", fill: "var(--warn)", lane: 0 },
  { key: "rem", label: "REM", fill: "color-mix(in srgb, var(--acc) 45%, transparent)", lane: 1 },
  { key: "light", label: "Light", fill: "var(--mut)", lane: 2 },
  { key: "deep", label: "Deep", fill: "var(--acc)", lane: 3 },
  // Never drawn with a lane of its own — a slice the watch couldn't classify
  // gets the light lane's position and a hairline fill, so it leaves a visible
  // gap in the night rather than silently closing over it.
  {
    key: "unmeasurable",
    label: "Unmeasurable",
    fill: "var(--line)",
    lane: 2,
  },
];

const stageStyle = (s: SleepStage) => STAGES.find((x) => x.key === s) ?? STAGES[2];

/** `2026-08-10T00:58:42` → `00:58`. */
function clockOf(local: string | null): string {
  return local?.slice(11, 16) ?? DASH;
}

/** Garmin's shouted enums, in a sentence. `EXCELLENT` → `excellent`. */
function quiet(key: string | null | undefined): string {
  return (key ?? "").toLowerCase().replace(/_/g, " ");
}

const QUALIFIER_COLOUR: Record<string, string> = {
  EXCELLENT: "var(--acc)",
  GOOD: "var(--acc)",
  FAIR: "var(--warn)",
  POOR: "var(--warn)",
};

export function Sleep() {
  const [days, setDays] = useState<number>(30);
  const { data, isLoading, error } = useQuery({
    queryKey: ["sleep", days],
    queryFn: () => sleepReport(days),
    placeholderData: (prev) => prev,
  });

  if (isLoading) return <Loading />;
  if (error) return <ErrorNote error={error} />;

  const report = data;
  const night = report?.lastNight ?? null;

  if (!night) {
    return (
      <div className="screen">
        <PageHeader eyebrow="Sleep" title="Last night" action={<RefreshButton />} space={20} />
        <RangePicker days={days} onPick={setDays} />
        {/* Two genuinely different empty states. One is a cache that has never
            held a night; the other is this build arriving on a cache full of
            wellness rows written before nights were kept — which looks
            identical on screen and is one sync away from fixed. */}
        {report?.needsBackfill ? (
          <Empty
            title="The detail hasn't been fetched yet."
            body="Your sleep hours and scores are cached, but the stage-by-stage nights behind them aren't — they're new. The next sync fills them in going backwards, one night per day it already knows about."
          />
        ) : (
          <Empty
            title="No nights cached."
            body="Sleep detail arrives with the daily sync. Run one from Settings and this fills in with last night."
          />
        )}
      </div>
    );
  }

  const woke = parseLocal(night.date);
  const avg = report!.averages;

  return (
    <div className="screen">
      <PageHeader
        eyebrow={woke ? `Woke ${longDate(woke)}` : night.date}
        title="Last night"
        lede={ledeFor(night)}
        action={
          <ScreenActions
            name={`sleep-${night.date}`}
            share={() => ({
              eyebrow: woke ? `Woke ${longDate(woke)}` : night.date,
              title: "Last night",
              headline:
                night.score != null
                  ? {
                      value: String(Math.round(night.score)),
                      // The qualifier is Garmin's word for the number and says
                      // more than "sleep score" does — "score · excellent"
                      // needs no scale to read against.
                      caption: night.scoreQualifier
                        ? `Sleep score · ${quiet(night.scoreQualifier)}`
                        : "Sleep score",
                    }
                  : undefined,
              metrics: [
                { label: "Asleep", value: hoursMinutes(night.totalSecs) },
                {
                  label: "Efficiency",
                  value: efficiency(night) != null ? `${efficiency(night)!.toFixed(0)}%` : DASH,
                },
                { label: "Deep", value: hoursMinutes(night.deepSecs) },
                { label: "REM", value: hoursMinutes(night.remSecs) },
                {
                  label: "Overnight HRV",
                  value:
                    night.avgOvernightHrv != null
                      ? String(Math.round(night.avgOvernightHrv))
                      : DASH,
                  unit: " ms",
                },
                {
                  label: "Resting HR",
                  value: night.restingHr != null ? String(Math.round(night.restingHr)) : DASH,
                  unit: " bpm",
                },
              ],
              chart: <StageStrip night={night} />,
              chartLabel: "Deep · light · REM · awake",
            })}
          />
        }
        space={26}
      />

      <MetricRow>
        <Metric label="Asleep" value={hoursMinutes(night.totalSecs)} />
        <Metric
          label={night.scoreQualifier ? `Score · ${quiet(night.scoreQualifier)}` : "Score"}
          value={night.score != null ? Math.round(night.score) : DASH}
          accent
        />
        <Metric
          label="Efficiency"
          value={efficiency(night) != null ? `${efficiency(night)!.toFixed(0)}%` : DASH}
        />
        <Metric
          label="Overnight HRV"
          value={night.avgOvernightHrv != null ? Math.round(night.avgOvernightHrv) : DASH}
        />
        <Metric
          label="Resting HR"
          value={night.restingHr != null ? Math.round(night.restingHr) : DASH}
        />
      </MetricRow>

      <Hypnogram night={night} />

      <StageMix night={night} />

      <Vitals night={night} />

      <Rule />

      <div className="section-head">The window behind it</div>
      <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)", margin: "6px 0 20px" }}>
        {windowSentence(avg.nights, avg.totalSecs, avg.score, avg.shortNights)}
      </div>
      <RangePicker days={days} onPick={setDays} />

      <Consistency nights={report!.nights} />

      {report!.insights.length > 0 && (
        <>
          <Rule />
          <div className="section-head">What your nights say</div>
          <div style={{ marginTop: 16 }}>
            {report!.insights.map((i) => (
              <InsightNote key={i.id} insight={i} />
            ))}
          </div>
        </>
      )}

      <Rule />
      <AskAbout night={night} days={days} />
    </div>
  );
}

/**
 * The handoff to Ask.
 *
 * This screen deliberately stops at what the data says. General sleep advice —
 * caffeine timing, light in the morning, the usual nine — is true, static, and
 * the same for everybody, which makes it a page you read once and then scroll
 * past forever. The coach can say all of it and say it about last night, so the
 * screen hands the question over instead of printing the answer.
 *
 * The question travels in the URL and lands in the composer rather than being
 * sent, so it can be edited into the one actually being asked. It carries the
 * night's date and the window, which is all the model needs — the `sleep` tool
 * fetches the rest itself.
 */
function AskAbout({ night, days }: { night: SleepNight; days: number }) {
  const question = `How did I sleep on ${night.date}, and what's worth changing? Look at the last ${days} nights too.`;

  return (
    <section style={{ margin: "6px 0 0" }}>
      <div className="section-head">Ask about it</div>
      <div
        style={{
          fontSize: "var(--fs-base)",
          color: "var(--mut)",
          lineHeight: 1.6,
          margin: "8px 0 18px",
          maxWidth: "58ch",
          textWrap: "pretty",
        }}
      >
        Everything above is what the numbers say. For what to do about them — or anything this
        screen doesn't cover — put it to the coach, which can read this night alongside your
        training and recovery.
      </div>
      <Link className="cta" to="/ask" search={{ q: question }}>
        Ask about this night <ArrowRight />
      </Link>
    </section>
  );
}

function efficiency(n: SleepNight): number | null {
  if (n.totalSecs == null) return null;
  const inBed = n.totalSecs + (n.awakeSecs ?? 0);
  return inBed > 0 ? (n.totalSecs / inBed) * 100 : null;
}

/**
 * The header sentence: when you slept, and what Garmin made of it.
 *
 * Garmin's feedback enum is worth surfacing rather than paraphrasing — it's the
 * one line on this screen that isn't this app's opinion.
 */
function ledeFor(n: SleepNight): string {
  const window =
    n.startLocal && n.endLocal
      ? `${clockOf(n.startLocal)} to ${clockOf(n.endLocal)}`
      : "an unrecorded window";
  const need =
    n.needSecs != null && n.totalSecs != null
      ? ` Garmin put the need at ${hoursMinutes(n.needSecs)}, so you were ${
          n.totalSecs >= n.needSecs
            ? `${hoursMinutes(n.totalSecs - n.needSecs)} over`
            : `${hoursMinutes(n.needSecs - n.totalSecs)} short`
        }.`
      : "";
  const verdict = n.feedback ? ` Its own verdict: ${quiet(n.feedback)}.` : "";
  return `${hoursMinutes(n.totalSecs)} asleep, ${window}.${need}${verdict}`;
}

function windowSentence(
  nights: number,
  total: number | null,
  score: number | null,
  short: number,
): string {
  if (!nights) return "Nothing cached for this window yet.";
  const parts = [`${nights} ${nights === 1 ? "night" : "nights"} cached`];
  if (total != null) parts.push(`averaging ${hoursMinutes(total)}`);
  if (score != null) parts.push(`at a score of ${Math.round(score)}`);
  return `${parts.join(", ")}. ${short} of them came in under seven hours.`;
}

function RangePicker({ days, onPick }: { days: number; onPick: (days: number) => void }) {
  return (
    <div
      style={{
        display: "flex",
        flexWrap: "wrap",
        gap: 18,
        rowGap: 10,
        fontSize: "var(--fs-small)",
        color: "var(--faint)",
        marginBottom: 34,
      }}
    >
      {RANGES.map((r) => (
        <button
          key={r.days}
          onClick={() => onPick(r.days)}
          style={{
            cursor: "pointer",
            color: days === r.days ? "var(--fg)" : "var(--faint)",
            borderBottom: `1px solid ${days === r.days ? "var(--acc)" : "transparent"}`,
            paddingBottom: 2,
          }}
        >
          {r.label}
        </button>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------- hypnogram --- */

const LANES = 4;
const LANE_H = 15;
const LANE_GAP = 4;
const HYPNO_H = LANES * LANE_H + (LANES - 1) * LANE_GAP;

/**
 * The night as it happened: one bar per unbroken run in a stage, laid out on a
 * minutes axis and stacked into four lanes.
 *
 * Drawn in a `viewBox` of minutes rather than pixels, with
 * `preserveAspectRatio="none"`, so a slice's width is its true share of the
 * night at any column width. That's the one property this chart has to keep —
 * a hypnogram whose bars don't stay proportional is decoration.
 *
 * The overnight heart rate goes underneath on the same axis rather than
 * overlaid on it. Overlaid, the line crosses four lanes of fill and is legible
 * against none of them; underneath, the two still read together because a dip
 * lines up vertically with the deep block that caused it.
 */
/**
 * The night's stage mix as one stacked bar.
 *
 * The hypnogram below is the real answer and this is not a replacement for it —
 * it's the shareable form, the same relationship `ZoneBar` has to the per-zone
 * table on an activity. A hypnogram at card size is a smear; the proportions
 * survive the shrink, and "how much of that was deep" is the question a card
 * gets asked anyway.
 */
function StageStrip({ night }: { night: SleepNight }) {
  const parts = (["deep", "light", "rem", "awake"] as const)
    .map((key) => ({
      key,
      secs: night[`${key}Secs` as const] ?? 0,
      fill: stageStyle(key).fill,
    }))
    .filter((p) => p.secs > 0);

  const total = parts.reduce((a, p) => a + p.secs, 0);
  if (total <= 0) return null;

  return (
    <div style={{ display: "flex", height: 12, overflow: "hidden", borderRadius: 2 }}>
      {parts.map((p) => (
        <div key={p.key} style={{ width: `${(p.secs / total) * 100}%`, background: p.fill }} />
      ))}
    </div>
  );
}

function Hypnogram({ night }: { night: SleepNight }) {
  if (!night.stages.length) {
    return (
      <div
        style={{
          fontSize: "var(--fs-small)",
          color: "var(--faint)",
          margin: "34px 0",
        }}
      >
        No stage timeline for this night — the watch recorded a duration but not the stages behind
        it.
      </div>
    );
  }

  const end = Math.max(...night.stages.map((s) => s.fromStartMins + s.secs / 60));
  const start = night.startLocal;

  return (
    <section style={{ margin: "36px 0 30px" }}>
      <svg
        viewBox={`0 0 ${end} ${HYPNO_H}`}
        preserveAspectRatio="none"
        style={{ width: "100%", height: HYPNO_H * 2, display: "block" }}
        role="img"
        aria-label={`Sleep stages from ${clockOf(night.startLocal)} to ${clockOf(night.endLocal)}`}
      >
        {night.stages.map((s: StageSlice, i) => {
          const style = stageStyle(s.stage);
          return (
            <rect
              key={`${s.startLocal}-${i}`}
              x={s.fromStartMins}
              y={style.lane * (LANE_H + LANE_GAP)}
              width={Math.max(s.secs / 60, 0.6)}
              height={LANE_H}
              fill={style.fill}
            >
              <title>
                {style.label} · {clockOf(s.startLocal)}–{clockOf(s.endLocal)} ·{" "}
                {Math.round(s.secs / 60)} min
              </title>
            </rect>
          );
        })}
      </svg>

      {/* Hour marks, in wall-clock time. Placed as their own row rather than as
          SVG text, which the non-uniform scaling above would stretch. */}
      <AxisLabels labels={hourTicks(start, end)} />

      <div
        style={{
          display: "flex",
          gap: 16,
          flexWrap: "wrap",
          marginTop: 14,
          fontSize: "var(--fs-caption)",
          color: "var(--mut)",
        }}
      >
        {STAGES.filter((s) => s.key !== "unmeasurable").map((s) => (
          <span key={s.key} style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: 1,
                background: s.fill,
                display: "inline-block",
              }}
            />
            {s.label} {stageMinutes(night, s.key)}
          </span>
        ))}
      </div>

      <HeartRate night={night} />
    </section>
  );
}

function stageMinutes(n: SleepNight, key: SleepStage): string {
  const secs =
    key === "deep"
      ? n.deepSecs
      : key === "light"
        ? n.lightSecs
        : key === "rem"
          ? n.remSecs
          : n.awakeSecs;
  return secs != null ? hoursMinutes(secs) : DASH;
}

/**
 * Four evenly spaced wall-clock marks under the night.
 *
 * Four rather than one per hour: `AxisLabels` spreads what it's given across
 * the full width, and eight or nine times on a phone's column collide.
 */
function hourTicks(startLocal: string | null, endMins: number): string[] {
  const start = parseLocal(startLocal);
  if (!start) return [];
  return [0, 1, 2, 3].map((i) => {
    const at = new Date(start.getTime() + (i / 3) * endMins * 60_000);
    return `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
  });
}

/**
 * Overnight heart rate, on the hypnogram's axis.
 *
 * The samples arrive already thinned to five-minute buckets, and a bucket the
 * watch missed has to stay a gap rather than being interpolated over — a flat
 * segment across a missing hour would be indistinguishable from an hour of a
 * genuinely steady pulse, which is the exact thing this chart is read for.
 */
function HeartRate({ night }: { night: SleepNight }) {
  if (night.hr.length < 4) return null;

  const slots = Math.max(...night.hr.map((h) => h.fromStartMins / 5)) + 1;
  const values: (number | null)[] = Array.from({ length: Math.round(slots) }, () => null);
  for (const h of night.hr) values[Math.round(h.fromStartMins / 5)] = h.bpm;

  const real = night.hr.map((h) => h.bpm);
  const low = Math.min(...real);

  return (
    <div style={{ marginTop: 22 }}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
          fontSize: "var(--fs-caption)",
          color: "var(--mut)",
          marginBottom: 6,
        }}
      >
        <span>Heart rate through the night</span>
        <span className="mono" style={{ color: "var(--faint)" }}>
          low {Math.round(low)} bpm
        </span>
      </div>
      <LineChart
        series={[
          {
            values,
            stroke: "var(--acc)",
            width: 1.25,
            fill: true,
            format: (v) => `${Math.round(v)} bpm`,
          },
        ]}
        height={56}
        baseline={false}
      />
    </div>
  );
}

/* ------------------------------------------------------------- stage mix --- */

/** How Garmin labels each component it scores. Its own keys are camelCase. */
const PART_LABEL: Record<string, string> = {
  totalDuration: "Duration",
  deepPercentage: "Deep sleep",
  remPercentage: "REM",
  lightPercentage: "Light sleep",
  awakeCount: "Times awake",
  restlessness: "Restlessness",
  stress: "Overnight stress",
};

/**
 * Each component of the score against the band Garmin wanted it in.
 *
 * The bands are the reason this section exists rather than a row of
 * percentages. 13% deep sleep is short of the range on an eight-hour night and
 * unremarkable on a five-hour one, because Garmin scales the band with the
 * length of the night — so a fixed target printed next to the number would be
 * wrong about half the time.
 *
 * Components Garmin scores without publishing a value — restlessness, stress,
 * awake count — keep their verdict and lose the bar. A bar with no value on it
 * is a shape pretending to be a measurement.
 */
function StageMix({ night }: { night: SleepNight }) {
  if (!night.scoreParts.length) return null;

  return (
    <section style={{ margin: "32px 0" }}>
      <div className="section-head">Against Garmin's targets</div>
      <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)", margin: "6px 0 18px" }}>
        Each part of the score with the range Garmin wanted it in — and those ranges move with how
        long you slept, which is why they're printed rather than assumed.
      </div>
      {night.scoreParts.map((p) => (
        <PartRow key={p.key} part={p} />
      ))}
    </section>
  );
}

function PartRow({ part }: { part: ScorePart }) {
  const colour = QUALIFIER_COLOUR[part.qualifier ?? ""] ?? "var(--mut)";
  const label = PART_LABEL[part.key] ?? part.key;
  // Percentages are the only components with a value worth drawing against a
  // band; the rest are counts Garmin bands in units it doesn't publish.
  const bar =
    part.value != null &&
    part.optimalStart != null &&
    part.optimalEnd != null &&
    part.optimalEnd > 0;

  return (
    <div style={{ marginBottom: 15 }}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          gap: 12,
          fontSize: "var(--fs-caption)",
          marginBottom: 6,
        }}
      >
        <span style={{ color: "var(--mut)" }}>{label}</span>
        <span className="mono" style={{ color: colour }}>
          {part.value != null ? `${Math.round(part.value)}%` : quiet(part.qualifier)}
          {bar && (
            <span style={{ color: "var(--faint)" }}>
              {" "}
              / {Math.round(part.optimalStart!)}–{Math.round(part.optimalEnd!)}%
            </span>
          )}
        </span>
      </div>
      {bar && (
        <div
          style={{ position: "relative", height: 8, background: "var(--line2)", borderRadius: 2 }}
        >
          {/* Scaled so the band sits in the middle two thirds — a bar scaled to
              100% would squeeze every real value into its left quarter. */}
          <Band
            value={part.value!}
            min={part.optimalStart!}
            max={part.optimalEnd!}
            colour={colour}
          />
        </div>
      )}
    </div>
  );
}

function Band({
  value,
  min,
  max,
  colour,
}: {
  value: number;
  min: number;
  max: number;
  colour: string;
}) {
  const scale = Math.max(max * 1.3, value * 1.1);
  return (
    <>
      <div
        style={{
          position: "absolute",
          left: `${(min / scale) * 100}%`,
          width: `${((max - min) / scale) * 100}%`,
          top: 0,
          bottom: 0,
          background: "var(--line)",
          borderRadius: 2,
        }}
      />
      <div
        style={{
          position: "absolute",
          left: 0,
          width: `${Math.min((value / scale) * 100, 100)}%`,
          top: 2,
          bottom: 2,
          background: colour,
          borderRadius: 2,
        }}
      />
    </>
  );
}

/* ---------------------------------------------------------------- vitals --- */

/**
 * The overnight measurements that aren't about time.
 *
 * Every one of these is null on some real night — an older watch, a loose
 * strap, pulse ox switched off to save battery — so the row renders only what
 * came back rather than a grid of dashes.
 */
function Vitals({ night }: { night: SleepNight }) {
  const items: { label: string; value: string }[] = [];
  const push = (label: string, v: number | null, fmt: (n: number) => string) => {
    if (v != null) items.push({ label, value: fmt(v) });
  };

  push("Respiration", night.avgRespiration, (v) => `${v.toFixed(0)}/min`);
  push("Lowest SpO₂", night.lowestSpo2, (v) => `${Math.round(v)}%`);
  push("Average SpO₂", night.avgSpo2, (v) => `${Math.round(v)}%`);
  push("Overnight stress", night.avgStress, (v) => num(v));
  push("Body battery gained", night.bodyBatteryChange, (v) => `+${Math.round(v)}`);
  push("Restless moments", night.restlessCount, (v) => num(v));
  push("Times awake", night.awakeCount, (v) => num(v));
  if (!items.length) return null;

  return (
    <section style={{ margin: "30px 0 6px" }}>
      <div className="section-head">Through the night</div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: IS_MOBILE ? "1fr 1fr" : "repeat(3, 1fr)",
          gap: "12px 22px",
          marginTop: 14,
        }}
      >
        {items.map((i) => (
          <div
            key={i.label}
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "baseline",
              gap: 10,
              paddingBottom: 9,
              borderBottom: "1px solid var(--line2)",
            }}
          >
            <span style={{ fontSize: "var(--fs-caption)", color: "var(--mut)" }}>{i.label}</span>
            <span className="mono" style={{ fontSize: "var(--fs-small)" }}>
              {i.value}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

/* ----------------------------------------------------------- consistency --- */

/** The axis every night is drawn on: 18:00 through to noon the next day. */
const AXIS_START = 0;
const AXIS_END = 18 * 60;

/**
 * One row per night, each a bar from the hour you fell asleep to the hour you
 * woke.
 *
 * This is the only chart here that isn't about a single night, and it earns the
 * space because it answers a question no average can: whether the nights line
 * up with each other. A column of bars with ragged left edges *is* the
 * irregular bedtime the insight below will go on to describe in numbers, and
 * the picture lands before the sentence does.
 *
 * Every bar shares one axis — 18:00 to 12:00 — rather than being scaled to its
 * own night, which is the whole point. Scaled individually they would all be
 * the same length and the chart would say nothing.
 */
function Consistency({ nights }: { nights: SleepNight[] }) {
  const rows = nights
    .filter((n) => n.startLocal && n.endLocal)
    // Oldest at the top, so the column reads downwards into the present like
    // every other list in this app.
    .slice()
    .reverse();
  if (rows.length < 2) return null;

  return (
    <section style={{ margin: "8px 0 4px" }}>
      <div className="section-head">When you slept</div>
      <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)", margin: "6px 0 16px" }}>
        One bar per night on a shared clock. Ragged left edges are an irregular bedtime — the thing
        worth fixing before anything else on this screen.
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
        {rows.map((n) => (
          <NightBar key={n.date} night={n} />
        ))}
      </div>
      <AxisLabels labels={["18:00", "23:00", "04:00", "09:00"]} />
    </section>
  );
}

function NightBar({ night }: { night: SleepNight }) {
  const from = minsPastSix(night.startLocal);
  const to = minsPastSix(night.endLocal);
  if (from == null || to == null || to <= from) return null;

  // Clamped at both ends rather than only at the width. The axis stops at
  // noon, and a night that ran past it — an afternoon of catching up after a
  // shift — would otherwise be drawn starting off the right edge.
  const pct = (m: number) =>
    Math.min(Math.max(((m - AXIS_START) / (AXIS_END - AXIS_START)) * 100, 0), 100);
  const score = night.score;

  return (
    <div
      style={{ position: "relative", height: 9 }}
      title={`${night.date} · ${clockOf(night.startLocal)}–${clockOf(night.endLocal)} · ${hoursMinutes(
        night.totalSecs,
      )}${score != null ? ` · score ${Math.round(score)}` : ""}`}
    >
      <div
        style={{
          position: "absolute",
          left: `${pct(from)}%`,
          width: `${pct(to) - pct(from)}%`,
          top: 0,
          bottom: 0,
          borderRadius: 2,
          // Shaded by score, so a short bar that also scored badly is visibly
          // worse than a short bar that didn't.
          background:
            score == null
              ? "var(--line)"
              : `color-mix(in srgb, var(--acc) ${Math.round(Math.min(Math.max(score, 20), 100))}%, var(--line))`,
        }}
      />
    </div>
  );
}

/** `2026-08-10T00:58:42` → minutes past 18:00, wrapping like the backend's. */
function minsPastSix(local: string | null): number | null {
  const hhmm = local?.slice(11, 16);
  if (!hhmm) return null;
  const [h, m] = hhmm.split(":").map(Number);
  if (!isFinite(h) || !isFinite(m)) return null;
  const mins = h * 60 + m;
  return mins >= 18 * 60 ? mins - 18 * 60 : mins + 6 * 60;
}

/* --------------------------------------------------------------- prose --- */

const TONE: Record<SleepInsight["tone"], { colour: string; label: string }> = {
  good: { colour: "var(--acc)", label: "Going well" },
  note: { colour: "var(--mut)", label: "Worth knowing" },
  watch: { colour: "var(--warn)", label: "Worth acting on" },
};

function InsightNote({ insight }: { insight: SleepInsight }) {
  const tone = TONE[insight.tone];
  return (
    <div style={{ borderLeft: `2px solid ${tone.colour}`, paddingLeft: 14, marginBottom: 20 }}>
      <div className="eyebrow" style={{ color: tone.colour }}>
        {tone.label} · {insight.nights} nights
      </div>
      <div style={{ fontSize: "var(--fs-lg)", margin: "4px 0 6px", textWrap: "pretty" }}>
        {insight.claim}
      </div>
      <div
        style={{
          fontSize: "var(--fs-base)",
          color: "var(--mut)",
          lineHeight: 1.55,
          textWrap: "pretty",
        }}
      >
        {insight.detail}
      </div>
    </div>
  );
}
