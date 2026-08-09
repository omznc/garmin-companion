import type { ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  cachedActivitiesSince,
  cachedDaily,
  findings as fetchFindings,
  type ApiFinding,
  type ApiFindingRow,
  type CachedActivity,
  type FindingUnit,
} from "../lib/api";
import { acuteChronic, dailySeries, easyHardSplit, insights } from "../lib/derive";
import { loadShape, nullChecks, SECTIONS, type LoadShape, type Section } from "../lib/analysis";
import {
  AxisLabels,
  Bullet,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  Metric,
  PageHeader,
  type Series,
} from "../components/ui";
import { RefreshButton } from "../components/Refresh";
import { daysAgo, isRun, num } from "../lib/format";
import { IS_MOBILE } from "../lib/platform";

/**
 * How each charted series writes its own numbers in the hover readout.
 *
 * The findings now arrive from Rust, and a formatting closure can't cross that
 * boundary — the series carries a unit name and this maps it back to the same
 * formatters the screen always used.
 */
const UNIT_FORMAT: Record<FindingUnit, (v: number) => string> = {
  spm: (v) => `${v.toFixed(0)} spm`,
  score: (v) => v.toFixed(0),
  pct: (v) => `${v.toFixed(0)}%`,
  pace: (v) => `${paceText(v)} /km`,
  perBeat: (v) => `${v.toFixed(2)} m/beat`,
  load: (v) => `${v.toFixed(0)} TRIMP`,
};

/** "8:34", from decimal minutes per kilometre. */
function paceText(minPerKm: number): string {
  const m = Math.floor(minPerKm);
  const s = Math.round((minPerKm - m) * 60);
  return s >= 60 ? `${m + 1}:00` : `${m}:${String(s).padStart(2, "0")}`;
}

export function Insights() {
  const daily = useQuery({ queryKey: ["daily", 365], queryFn: () => cachedDaily(365) });
  const acts = useQuery({
    queryKey: ["activitiesSince", 365],
    queryFn: () => cachedActivitiesSince(daysAgo(365)),
  });
  // Computed in Rust rather than here, so the coach reads the same findings.
  const deepQ = useQuery({ queryKey: ["findings", 365], queryFn: () => fetchFindings(365) });

  if (daily.isLoading || acts.isLoading || deepQ.isLoading)
    return <Loading label="Crunching the cache" />;
  if (daily.error) return <ErrorNote error={daily.error} />;
  if (acts.error) return <ErrorNote error={acts.error} />;
  if (deepQ.error) return <ErrorNote error={deepQ.error} />;

  const rows = dailySeries(daily.data ?? [], 365);
  const activities = acts.data ?? [];
  const deep = deepQ.data ?? [];
  const found = insights(rows, activities);
  const load = acuteChronic(activities);
  const shape = loadShape(activities);
  const flags = riskFlags(activities);
  const nulls = nullChecks(rows, activities);

  /**
   * Every section is conditional, so the gap between two of them can't be a
   * fixed top margin on each — whichever one survives first has to sit tight
   * under the header, and on a thin cache that could be any of them. They are
   * collected first and spaced by position instead.
   */
  const blocks: Array<{ key: string; head: string; body: ReactNode }> = [];

  for (const section of SECTIONS) {
    const items = deep.filter((f) => f.section === section);
    if (!items.length) continue;
    blocks.push({
      key: section,
      head: SECTION_LABELS[section],
      body: items.map((f) => <FindingBlock key={f.kind} finding={f} />),
    });
  }

  if (found.length) {
    blocks.push({
      key: "correlations",
      head: "Correlations",
      body: found.map((i, idx) => (
        <div key={idx} style={{ marginBottom: 54 }}>
          <Claim>{i.claim}</Claim>
          <Split
            chart={
              <LineChart
                series={[
                  { ...i.a, stroke: "var(--acc)" },
                  { ...i.b, stroke: "var(--faint)", width: 1, dashed: true },
                ]}
                height={70}
                viewWidth={260}
                pad={6}
              />
            }
          >
            {i.detail}
          </Split>
          <Basis>{i.basis}</Basis>
        </div>
      )),
    });
  }

  if (load) {
    blocks.push({
      key: "load",
      head: "Training load",
      body: <LoadBlock load={load} shape={shape} />,
    });
  }

  if (flags.length) {
    blocks.push({
      key: "flags",
      head: "Risk flags",
      body: (
        <div style={{ display: "flex", flexDirection: "column", gap: 13 }}>
          {flags.map((f, i) => (
            <Bullet key={i} accent={f.accent}>
              {f.text}
            </Bullet>
          ))}
        </div>
      ),
    });
  }

  if (nulls.length) {
    blocks.push({
      key: "nulls",
      head: "Asked, and found nothing",
      body: (
        <>
          <p
            style={{
              fontSize: "var(--fs-base)",
              color: "var(--mut)",
              lineHeight: 1.65,
              maxWidth: "62ch",
              margin: "0 0 28px",
              textWrap: "pretty",
            }}
          >
            Comparisons that came back empty. They are here because a screen that only ever prints
            the ones that worked is quietly implying the others were never run — and because a
            plausible belief you can stop holding is worth as much as a new one.
          </p>
          <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
            {nulls.map((n, i) => (
              <div key={i}>
                <div style={{ fontSize: "var(--fs-md)", lineHeight: 1.5 }}>{n.question}</div>
                <div
                  style={{
                    fontSize: "var(--fs-base)",
                    color: "var(--mut)",
                    lineHeight: 1.6,
                    marginTop: 6,
                    maxWidth: "62ch",
                    textWrap: "pretty",
                  }}
                >
                  {n.verdict}
                </div>
                <Basis>{n.basis}</Basis>
              </div>
            ))}
          </div>
        </>
      ),
    });
  }

  return (
    <div className="screen">
      <PageHeader
        eyebrow="Last 365 days"
        title="Insights"
        lede="What a year of your own cached data says once it has been asked properly. Correlations, not causes — each states the sample it was computed over so you can judge how much to believe it, and the questions that came back with nothing are at the bottom rather than left out."
        action={<RefreshButton />}
        space={54}
      />

      {!blocks.length && (
        <Empty
          title="Not enough history yet."
          body="These are computed from paired daily metrics and activities. A few more weeks of synced data and the first ones will appear — nothing here is shown until the arithmetic supports it."
        />
      )}

      {blocks.map((b, i) => (
        <section key={b.key} style={{ marginTop: i === 0 ? 0 : 62 }}>
          <div className="eyebrow" style={{ margin: "0 0 26px" }}>
            {b.head}
          </div>
          {b.body}
        </section>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------------ parts --- */

/**
 * Section headings say what the section is *for*, not what it files. "Fitness"
 * over a chart of pace at a fixed heart rate is a filing label; naming the
 * thing that number stands in for is the whole point of the section.
 */
const SECTION_LABELS: Record<Section, string> = {
  Fitness: "Fitness — the number Garmin won't give you",
  Recovery: "Recovery — what a session actually costs",
  Patterns: "Patterns — the shape of your year",
};

/** The sentence a finding is, set large. One per block, and never hedged —
 *  the hedge is the basis line under it. */
function Claim({ children }: { children: ReactNode }) {
  return (
    <div
      className="serif"
      style={{
        fontSize: 27,
        lineHeight: 1.35,
        letterSpacing: "-0.005em",
        marginBottom: 14,
        maxWidth: "24ch",
      }}
    >
      {children}
    </div>
  );
}

function Basis({ children }: { children: ReactNode }) {
  return (
    <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 10 }}>
      {children}
    </div>
  );
}

/**
 * Prose beside a chart on a desktop, stacked under it on a phone.
 *
 * The chart is a fixed 260px next to the sentence that explains it. That would
 * leave 28px for the sentence in a phone's column, so there the two stack and
 * the chart takes the width instead — which is also the order they're read in.
 */
function Split({ children, chart }: { children: ReactNode; chart?: ReactNode }) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: IS_MOBILE ? "column" : "row",
        gap: IS_MOBILE ? 18 : 32,
        alignItems: IS_MOBILE ? "stretch" : "flex-end",
      }}
    >
      <div
        style={{
          flex: 1,
          fontSize: "var(--fs-md)",
          lineHeight: 1.65,
          color: "var(--mut)",
          textWrap: "pretty",
        }}
      >
        {children}
      </div>
      {chart && <div style={IS_MOBILE ? undefined : { width: 260, flex: "none" }}>{chart}</div>}
    </div>
  );
}

/**
 * The table some findings carry instead of a second chart.
 *
 * Deliberately not a `<table>`: every row is a label, one figure, and the
 * sample that figure came from, and the last of those has to be free to wrap
 * under the label in a phone's column rather than holding a column of its own
 * open at four characters wide.
 */
function Rows({ rows }: { rows: ApiFindingRow[] }) {
  return (
    <div style={{ marginTop: 24 }}>
      {rows.map((r, i) => (
        <div
          key={i}
          style={{
            display: "flex",
            gap: 16,
            alignItems: "baseline",
            justifyContent: "space-between",
            padding: "11px 0",
            borderTop: i === 0 ? undefined : "1px solid var(--line2)",
          }}
        >
          <div style={{ minWidth: 0 }}>
            <div style={{ fontSize: "var(--fs-base)", color: r.accent ? "var(--acc)" : undefined }}>
              {r.label}
            </div>
            {r.note && (
              <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 4 }}>
                {r.note}
              </div>
            )}
          </div>
          <div
            className="mono"
            style={{
              fontSize: "var(--fs-md)",
              flex: "none",
              color: r.accent ? "var(--acc)" : undefined,
            }}
          >
            {r.value}
          </div>
        </div>
      ))}
    </div>
  );
}

function FindingBlock({ finding }: { finding: ApiFinding }) {
  const solo = finding.series?.length === 1;
  const series: Series[] | undefined = finding.series?.map((s) => ({
    values: s.values,
    name: s.name,
    format: UNIT_FORMAT[s.format],
    invert: s.invert,
    stroke: s.muted ? "var(--faint)" : "var(--acc)",
    width: s.muted ? 1 : undefined,
    dashed: s.muted,
    fill: solo,
  }));

  // Only the ends are labelled. One point per run or per month means the axis
  // is a span rather than a scale, and a specific date is what hovering the
  // line is for.
  const ends =
    finding.labels && finding.labels.length > 1
      ? [finding.labels[0], finding.labels[finding.labels.length - 1]]
      : null;

  return (
    <div style={{ marginBottom: 54 }}>
      <Claim>{finding.claim}</Claim>
      <Split
        chart={
          series && (
            <>
              <LineChart
                series={series}
                labels={finding.labels}
                height={70}
                viewWidth={260}
                pad={6}
              />
              {ends && <AxisLabels labels={ends} />}
            </>
          )
        }
      >
        {finding.detail}
      </Split>
      {finding.rows && <Rows rows={finding.rows} />}
      <Basis>{finding.basis}</Basis>
    </div>
  );
}

/* ------------------------------------------------------------------- load --- */

function LoadBlock({
  load,
  shape,
}: {
  load: { acute: number; chronic: number; ratio: number };
  shape: LoadShape | null;
}) {
  return (
    <>
      {/* Wraps because the status is a word rather than a figure, and
          "Ramping up" at 42px is wider than a phone's column on its own. */}
      <div
        style={{
          display: "flex",
          gap: IS_MOBILE ? "22px 28px" : 48,
          alignItems: "flex-end",
          flexWrap: "wrap",
          marginBottom: 16,
        }}
      >
        <Metric
          size={34}
          label="Acute / chronic"
          value={load.ratio.toFixed(2)}
          accent={load.ratio > 1.5}
        />
        {shape?.latest.monotony != null && (
          <Metric
            size={34}
            label="Monotony"
            value={shape.latest.monotony.toFixed(2)}
            accent={shape.latest.monotony >= 2}
          />
        )}
        <div>
          <div className="serif" style={{ fontSize: 42, lineHeight: 1 }}>
            {loadStatus(load.ratio)}
          </div>
          <div
            style={{
              font: "400 var(--fs-micro)/1 'Instrument Sans', sans-serif",
              letterSpacing: "0.1em",
              textTransform: "uppercase",
              color: "var(--mut)",
              marginTop: 9,
            }}
          >
            Status
          </div>
        </div>
      </div>

      <div style={{ fontSize: "var(--fs-base)", color: "var(--mut)", lineHeight: 1.6 }}>
        {load.acute.toFixed(1)} h of training in the last seven days against a 28-day weekly average
        of {load.chronic.toFixed(1)} h.{" "}
        {load.ratio > 1.5
          ? "That's a sharp step up — the range where injury risk climbs."
          : load.ratio < 0.8
            ? "You're training less than your recent norm, which is what a down week looks like."
            : "That's a steady ratio."}
      </div>

      {shape && (
        <>
          <div style={{ marginTop: 28 }}>
            <LineChart
              series={[
                {
                  values: shape.weeks.map((w) => w.trimp),
                  stroke: "var(--acc)",
                  fill: true,
                  name: "Weekly load",
                  format: (v) => `${v.toFixed(0)} au`,
                },
              ]}
              height={92}
              labels={shape.weeks.map((w) => `Week of ${w.start}`)}
            />
            <AxisLabels
              labels={[
                shape.weeks[0].start.slice(5),
                shape.weeks[shape.weeks.length - 1].start.slice(5),
              ]}
            />
          </div>
          <div
            style={{
              fontSize: "var(--fs-base)",
              color: "var(--mut)",
              lineHeight: 1.6,
              marginTop: 20,
              maxWidth: "62ch",
              textWrap: "pretty",
            }}
          >
            Weekly load in Edwards' training impulse — minutes in each heart-rate zone, weighted one
            through five, so an hour of Z2 and an hour of Z4 aren't counted as the same hour.
            Monotony is that week's mean daily load over its own standard deviation:{" "}
            {shape.latest.monotony == null
              ? "undefined this week, because the load all landed on one day."
              : shape.latest.monotony >= 2
                ? `at ${shape.latest.monotony.toFixed(2)} the week was spread evenly with no real recovery day in it, which is the pattern associated with grinding rather than building.`
                : `at ${shape.latest.monotony.toFixed(2)} it sits under the 2.0 usually flagged, meaning your hard days and your easy days are genuinely different from each other.`}{" "}
            {shape.monotonous > 0 &&
              `${shape.monotonous} of the last ${shape.weeks.length} weeks cleared 2.0.`}
          </div>
        </>
      )}
    </>
  );
}

/* ------------------------------------------------------------------ flags --- */

function loadStatus(ratio: number): string {
  if (ratio > 1.5) return "Spiking";
  if (ratio > 1.15) return "Building";
  if (ratio > 0.85) return "Steady";
  return "Easing";
}

function riskFlags(activities: CachedActivity[]): Array<{ accent: boolean; text: string }> {
  const out: Array<{ accent: boolean; text: string }> = [];
  const runs = activities.filter((a) => isRun(a.typeKey));

  const recent = runs.slice(0, 10);
  const split = easyHardSplit(recent);
  if (split && split.hardPct > 50) {
    out.push({
      accent: true,
      text: `Across your last ${split.counted} runs with heart-rate data, ${split.hardPct.toFixed(0)}% of the time was above Z2. Short and hard is fine as a choice; it stops being one when every run is.`,
    });
  }

  const short = recent.filter((a) => (a.durationS ?? 0) < 15 * 60);
  if (recent.length >= 4 && short.length >= recent.length - 1) {
    out.push({
      accent: true,
      text: `${short.length} of your last ${recent.length} runs were under fifteen minutes. Aerobic base is built by duration — one longer easy run a week is the lever here.`,
    });
  }

  const cadences = recent.map((a) => a.avgCadence).filter((c): c is number => c != null);
  if (cadences.length >= 3) {
    const avg = cadences.reduce((a, b) => a + b, 0) / cadences.length;
    if (avg < 160) {
      out.push({
        accent: false,
        text: `Average cadence across recent runs is ${num(avg)} spm. Nearer 170 means shorter, lighter contacts — which matters more the heavier you are.`,
      });
    }
  }

  return out;
}
