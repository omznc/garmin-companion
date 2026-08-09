import { useEffect, useState } from "react";
import { useNavigate, useParams } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  activityAnalysis,
  activityCritique,
  activityTags,
  cachedActivity,
  cachedActivityCritique,
  chatConfig,
  type ActivityAnalysis,
  type ActivityLap,
  type ActivitySeries,
  type CachedActivity,
  type Highlight,
} from "../lib/api";
import {
  BackLink,
  colWidth,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  Metric,
  MetricRow,
  PageHeader,
  Rule,
  Unit,
} from "../components/ui";
import { ActivityMap } from "../components/ActivityMap";
import { ActivityChat } from "../components/ActivityChat";
import { AiMark } from "../components/AiMark";
import { SignalNote, ZoneDisagreement } from "../components/SignalNote";
import { Tags } from "../components/Tags";
import { hasData, type Point } from "../lib/chart";
import { zonePercentages, zoneTotal } from "../lib/derive";
import {
  DASH,
  duration,
  isRun,
  km,
  longDate,
  num,
  pace,
  parseLocal,
  since,
  speed,
  sportLabel,
  timeOfDay,
} from "../lib/format";

const ZONE_NAMES = ["Z1 recovery", "Z2 easy", "Z3 tempo", "Z4 threshold", "Z5 max"];

export function ActivityDetail() {
  const { activityId } = useParams({ from: "/activities/$activityId" });
  const navigate = useNavigate();
  const id = Number(activityId);

  /**
   * The moment on the timeline the pointer is over, shared by the map and every
   * chart below it. They are all views of the same sample array, so pointing at
   * a spike on the heart-rate chart marks the place on the route it happened.
   */
  const [hover, setHover] = useState<number | null>(null);

  // The cached summary paints the top of the page immediately; the analysis is
  // three Garmin requests on first open and fills in everything below it. Two
  // queries rather than one so an offline visit still shows the numbers.
  const activity = useQuery({
    queryKey: ["activity", id],
    queryFn: () => cachedActivity(id),
  });

  const analysis = useQuery({
    queryKey: ["activityAnalysis", id],
    queryFn: () => activityAnalysis(id),
    retry: false,
    staleTime: 5 * 60_000,
  });

  if (activity.isLoading) return <Loading />;
  if (activity.error) return <ErrorNote error={activity.error} />;

  const a = activity.data;
  if (!a) {
    return (
      <Empty
        title="Not in the cache."
        body="This activity isn't in the local copy. A sync may bring it in."
      />
    );
  }

  const start = parseLocal(a.startTimeLocal ?? a.localDate);
  const paced = isRun(a.typeKey) || !!a.typeKey?.match(/walk|hik/);
  const data = analysis.data;

  return (
    <div className="screen">
      <BackLink
        onClick={() => navigate({ to: "/activities" })}
        style={{ marginBottom: 26, color: "var(--mut)" }}
      >
        Activities
      </BackLink>

      <PageHeader
        eyebrow={
          <>
            {sportLabel(a.typeKey)}
            {start && ` · ${longDate(start)} · ${timeOfDay(start)}`}
          </>
        }
        title={a.name ?? "Untitled"}
        space={30}
      />

      <Critique activityId={id} ready={analysis.isSuccess} />

      <MetricRow gap={46} style={{ marginBottom: 30 }}>
        {a.distanceM != null && a.distanceM > 0 && (
          <Metric size={31} label="Distance" value={km(a.distanceM, 2)} />
        )}
        <Metric size={31} label="Duration" value={duration(a.durationS)} />
        {a.movingDurationS != null && a.movingDurationS > 0 && (
          <Metric size={31} label="Moving" value={duration(a.movingDurationS)} />
        )}
        {a.distanceM != null && a.distanceM > 0 && (
          <Metric
            size={31}
            label={paced ? "Avg pace" : "Avg speed"}
            value={
              paced ? (
                <>
                  {pace(a.distanceM, a.durationS)}
                  <Unit size={19}> /km</Unit>
                </>
              ) : (
                speed(a.distanceM, a.durationS)
              )
            }
          />
        )}
        {a.avgHr != null && <Metric size={31} label="Avg HR" value={num(a.avgHr)} />}
        {a.maxHr != null && <Metric size={31} label="Max HR" value={num(a.maxHr)} />}
        {a.avgCadence != null && <Metric size={31} label="Cadence" value={num(a.avgCadence)} />}
        {a.elevationGain != null && a.elevationGain > 0 && (
          <Metric
            size={31}
            label="Ascent"
            value={
              <>
                {num(a.elevationGain)}
                <Unit size={19}> m</Unit>
              </>
            }
          />
        )}
      </MetricRow>

      <TagRow activityId={id} />

      {/* Everything below here needs the samples. While they're on the way the
          page is already complete enough to read — which is why the wait is a
          line of text and not a spinner over the whole screen. */}
      {analysis.isLoading && (
        <div style={{ marginTop: 42 }}>
          <Loading label="Reading the session from Garmin" />
        </div>
      )}
      {analysis.error != null && (
        <p style={{ fontSize: "var(--fs-small)", color: "var(--faint)", margin: "42px 0 0" }}>
          Couldn't fetch the samples for this session — the numbers above came from the cache and
          are unaffected.
        </p>
      )}

      {data && (
        <>
          {data.highlights.length > 0 && (
            <div style={{ marginTop: 46 }}>
              <Highlights highlights={data.highlights} />
            </div>
          )}

          <div style={{ marginTop: 46 }}>
            <ActivityMap
              series={data.series}
              zones={data.zones}
              highlights={data.highlights}
              hover={hover}
              onHover={setHover}
            />
          </div>
        </>
      )}

      <div style={{ marginTop: 46 }}>
        <Zones activity={a} />
        {/* Under the split rather than above it. The numbers are the point of
            the section and a caveat that arrives first reads as a disclaimer
            on the page; arriving second, it reads as a footnote on the
            figures — which is what it is. */}
        {data && (
          <>
            <SignalNote confidence={data.hrConfidence} />
            <ZoneDisagreement maxPct={data.zones.maxDisagreementPct} />
          </>
        )}
      </div>

      {data && (
        <>
          <div style={{ marginTop: 46 }}>
            <Charts series={data.series} hover={hover} onHover={setHover} />
          </div>
          <Splits laps={data.laps} paced={paced} />
          {data.comparison && <Against comparison={data.comparison} sport={a.typeKey} />}
        </>
      )}

      <Rule m="52px 0 26px" />
      <div className="eyebrow" style={{ marginBottom: 18 }}>
        Ask about this session
      </div>
      <ActivityChat activityId={id} activityName={a.name ?? "Untitled"} />
    </div>
  );
}

/* -------------------------------------------------------------- critique --- */

/**
 * The coach's verdict on the session, written on request.
 *
 * Not a description of the run: the page below is already that, and so is the
 * memory of having done it. What this asks the model for is the part that isn't
 * on the screen — what went wrong, what to have done instead, and what to carry
 * into the next one — which is also why it is a button rather than something
 * that happens on open. An unsolicited opinion about every session you look at
 * is both a request you didn't ask for and a criticism you didn't invite.
 *
 * A critique already written is shown straight away, since that costs a read of
 * a local table. `ready` holds the button until the analysis has landed — the
 * critique is written from that exact analysis, and pressing it earlier would
 * have two requests racing to fetch the same three things from Garmin.
 */
function Critique({ activityId, ready }: { activityId: number; ready: boolean }) {
  const [run, setRun] = useState(0);
  const qc = useQueryClient();
  const config = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });
  const configured = !!(config.data?.provider && config.data.model);

  // The route reuses this component between activities, so a press on one run
  // would otherwise be carried onto the next one opened and spend a request on
  // a session nobody asked about.
  useEffect(() => setRun(0), [activityId]);

  const stored = useQuery({
    queryKey: ["activityCritique", activityId],
    queryFn: () => cachedActivityCritique(activityId),
    enabled: ready && configured,
    retry: false,
    staleTime: Infinity,
  });

  const written = useQuery({
    // `run` is in the key so pressing rewrite is a new query rather than a
    // refetch that would show the previous answer again on the way past.
    queryKey: ["activityCritiqueRun", activityId, run],
    queryFn: () => activityCritique(activityId),
    enabled: run > 0,
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  });

  // A rewrite replaces what's stored, so the read query has to be told —
  // otherwise leaving the page and coming back inside the cache window would
  // bring the superseded critique back with it.
  useEffect(() => {
    if (written.data) qc.setQueryData(["activityCritique", activityId], written.data);
  }, [written.data, activityId, qc]);

  // Nothing at all when there's no model: an empty state here would be a
  // settings prompt sitting on top of a page that works without one.
  if (!configured) return null;

  const critique = written.data ?? stored.data ?? null;

  if (!critique) {
    if (!ready) return null;

    if (written.isFetching) {
      return (
        <div style={{ marginBottom: 34 }}>
          <Loading label="Reading this session back" />
        </div>
      );
    }

    return (
      <div style={{ marginBottom: 34 }}>
        <button
          className="underlined"
          style={{ fontSize: "var(--fs-md)" }}
          onClick={() => setRun((n) => n + 1)}
        >
          Tell me what I got wrong
        </button>
        {written.isError && (
          <p
            style={{
              fontSize: "var(--fs-small)",
              color: "var(--faint)",
              margin: "10px 0 0",
              maxWidth: "62ch",
              lineHeight: 1.6,
            }}
          >
            {/* The provider's own words. "Couldn't write a critique" is true of
                an expired key, a model that no longer exists and a flat network
                alike, and tells you which of the three to fix in none of them. */}
            {errorText(written.error)}
          </p>
        )}
      </div>
    );
  }

  return (
    <div style={{ marginBottom: 34 }}>
      <AiMark label="session critique">
        <p
          className="selectable"
          style={{
            fontSize: "var(--fs-lg)",
            lineHeight: 1.75,
            margin: 0,
            maxWidth: "68ch",
            textWrap: "pretty",
          }}
        >
          {critique.text}
        </p>
      </AiMark>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 14,
          marginTop: 10,
          fontSize: "var(--fs-caption)",
          color: "var(--faint)",
        }}
      >
        <span>Written {since(critique.generatedAt)}</span>
        <button
          className="underlined"
          onClick={() => setRun((n) => n + 1)}
          disabled={written.isFetching}
        >
          {written.isFetching ? "Rewriting…" : "Rewrite"}
        </button>
        {/* A failed rewrite leaves the previous critique on the page, so the
            reason it failed has to be said next to the control that failed. */}
        {written.isError && <span>{errorText(written.error)}</span>}
      </div>
    </div>
  );
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/* ------------------------------------------------------------------ tags --- */

/**
 * The tag row reads its own list rather than waiting for the analysis.
 *
 * The tags ride along on the analysis, but that is three Garmin requests on
 * first open and the row should be editable long before it lands. This is a
 * single indexed read of a local table, so it resolves in the same frame.
 */
function TagRow({ activityId }: { activityId: number }) {
  const tags = useQuery({
    queryKey: ["activityTags", activityId],
    queryFn: () => activityTags(activityId),
  });

  return <Tags activityId={activityId} tags={tags.data ?? []} />;
}

/* ------------------------------------------------------------ highlights --- */

/**
 * What was worth noticing, computed from the samples rather than written about
 * them. The paragraph at the top is the model's reading of exactly this list,
 * so the two can't disagree — and this half survives having no model at all.
 */
function Highlights({ highlights }: { highlights: Highlight[] }) {
  return (
    <>
      <div className="eyebrow" style={{ marginBottom: 14 }}>
        Worth noticing
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
        {highlights.map((h, i) => (
          <div
            key={`${h.kind}-${i}`}
            style={{ display: "flex", gap: 14, alignItems: "flex-start" }}
          >
            {/* A tone, not a severity. Accent is the thing to look at, muted is
                context, and a good session gets a mark rather than nothing. */}
            <span
              style={{
                width: 5,
                height: 5,
                borderRadius: "50%",
                marginTop: 9,
                flex: "none",
                background:
                  h.tone === "watch"
                    ? "var(--acc)"
                    : h.tone === "good"
                      ? "var(--fg)"
                      : "var(--faint)",
              }}
            />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: "var(--fs-md)" }}>{h.title}</div>
              <p
                style={{
                  fontSize: "var(--fs-small)",
                  lineHeight: 1.65,
                  color: "var(--mut)",
                  margin: "3px 0 0",
                  maxWidth: "62ch",
                  textWrap: "pretty",
                }}
              >
                {h.detail}
              </p>
            </div>
            {h.atS != null && (
              <span
                className="mono"
                style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", flex: "none" }}
              >
                {duration(h.atS)}
              </span>
            )}
          </div>
        ))}
      </div>
    </>
  );
}

/* ----------------------------------------------------------------- zones --- */

/**
 * The zone breakdown, which is the number this whole app exists to show.
 * Rendered as stacked bars rather than a chart because the comparison that
 * matters is between the five rows, not across time.
 */
function Zones({ activity }: { activity: CachedActivity }) {
  const total = zoneTotal(activity);
  if (total <= 0) {
    return (
      <>
        <div className="eyebrow" style={{ marginBottom: 12 }}>
          Heart-rate zones
        </div>
        <p
          style={{
            fontSize: "var(--fs-base)",
            color: "var(--mut)",
            margin: "0 0 8px",
            maxWidth: "56ch",
          }}
        >
          No heart-rate data was recorded for this session, so there is no zone breakdown — not a
          session spent entirely in Z1.
        </p>
      </>
    );
  }

  const pct = zonePercentages(activity);
  const hard = pct[2] + pct[3] + pct[4];

  return (
    <>
      <div className="section-head" style={{ marginBottom: 14 }}>
        <div className="eyebrow">Heart-rate zones</div>
        <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)" }}>
          {hard.toFixed(0)}% above Z2 · {duration(total)} tracked
        </div>
      </div>

      {/* No rule under each row. The bar already separates one row from the
          next, so a hairline underneath it was a second divider for one gap. */}
      {ZONE_NAMES.map((name, i) => (
        <div key={name} style={{ padding: "10px 0 12px" }}>
          <div style={{ display: "flex", alignItems: "baseline", gap: 16 }}>
            <span style={{ flex: 1, fontSize: "var(--fs-md)" }}>{name}</span>
            <span style={{ fontSize: "var(--fs-small)", color: "var(--mut)" }}>
              {duration(activity.zoneSecs[i])}
            </span>
            <span
              className="mono"
              style={{ fontSize: "var(--fs-md)", width: 62, textAlign: "right" }}
            >
              {pct[i].toFixed(0)}%
            </span>
          </div>
          <div className="bar" style={{ marginTop: 10 }}>
            <span
              style={{
                width: `${pct[i]}%`,
                // Z1/Z2 are the target; above that reads as accent so drift is
                // visible without reading the numbers.
                background: i < 2 ? "var(--mut)" : "var(--acc)",
              }}
            />
          </div>
        </div>
      ))}
    </>
  );
}

/* ---------------------------------------------------------------- charts --- */

interface Metric3 {
  key: string;
  label: string;
  values: Point[];
  stroke: string;
  /** Faster is a smaller number, so the pace chart draws upside down. */
  invert?: boolean;
  format: (v: number) => string;
}

function Charts({
  series,
  hover,
  onHover,
}: {
  series: ActivitySeries;
  hover: number | null;
  onHover: (index: number | null) => void;
}) {
  const charts = extractSeries(series);
  const elapsed = series.elapsedS.map((t) => (t == null ? "" : `${duration(t)} in`));

  if (!charts.length) {
    return (
      <p style={{ fontSize: "var(--fs-small)", color: "var(--faint)", margin: "0 0 36px" }}>
        Garmin has no sampled series for this activity.
      </p>
    );
  }

  return (
    <>
      {charts.map((s) => (
        <div key={s.key} style={{ marginBottom: 38 }}>
          <div className="section-head" style={{ marginBottom: 10 }}>
            <div className="eyebrow">{s.label}</div>
            <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)" }}>{summarise(s)}</div>
          </div>
          <LineChart
            series={[{ values: s.values, stroke: s.stroke, invert: s.invert, format: s.format }]}
            height={92}
            labels={elapsed}
            hoverIndex={hover}
            onHoverIndex={onHover}
          />
        </div>
      ))}
    </>
  );
}

function summarise(s: Metric3): string {
  const vals = s.values.filter((v): v is number => v != null);
  if (!vals.length) return "";
  const avg = vals.reduce((a, b) => a + b, 0) / vals.length;
  // "Best" for pace is the smallest number, not the largest.
  const best = s.invert ? Math.min(...vals) : Math.max(...vals);
  return `${s.format(avg)} avg · ${s.format(best)} ${s.invert ? "best" : "max"}`;
}

/** Decimal minutes to "5:01". */
function paceLabel(minPerKm: number): string {
  const m = Math.floor(minPerKm);
  const sec = Math.round((minPerKm - m) * 60);
  return sec === 60 ? `${m + 1}:00` : `${m}:${String(sec).padStart(2, "0")}`;
}

/**
 * The three columns worth charting, in the order they answer questions in.
 *
 * The parsing that used to live here — Garmin's column-oriented details payload
 * — moved into the analysis, so the charts, the map and the written summary all
 * read one set of numbers rather than three parses of one payload.
 *
 * Elevation isn't among them. It's plotted against distance under the map,
 * where it belongs — a hill is a feature of the ground, and against time it
 * stretches out exactly where you slowed down for it.
 */
function extractSeries(series: ActivitySeries): Metric3[] {
  const wanted: Metric3[] = [
    {
      key: "hr",
      label: "Heart rate",
      values: series.hr,
      stroke: "var(--acc)",
      format: (v) => `${v.toFixed(0)} bpm`,
    },
    {
      key: "pace",
      label: "Pace",
      values: series.paceMinKm,
      stroke: "var(--fg)",
      invert: true,
      format: (v) => `${paceLabel(v)} /km`,
    },
    {
      key: "cadence",
      label: "Cadence",
      values: series.cadence,
      stroke: "var(--mut)",
      format: (v) => `${v.toFixed(0)} spm`,
    },
  ];
  return wanted.filter((s) => hasData(s.values));
}

/* ---------------------------------------------------------------- splits --- */

function Splits({ laps, paced }: { laps: ActivityLap[]; paced: boolean }) {
  if (laps.length < 2) return null;

  // Bars are scaled against the slowest lap so the fastest fills the row —
  // an absolute scale would leave every bar near-identical.
  const rates = laps.map((l) => l.paceMinKm);
  const valid = rates.filter((r): r is number => r != null);
  const slowest = valid.length ? Math.max(...valid) : 1;
  const fastest = valid.length ? Math.min(...valid) : 1;

  return (
    <>
      <div className="eyebrow" style={{ margin: "0 0 8px" }}>
        Splits
      </div>
      {laps.map((l, i) => {
        const rate = rates[i];
        const width =
          rate != null && slowest > fastest
            ? 20 + ((slowest - rate) / (slowest - fastest)) * 80
            : 60;
        return (
          <div
            key={i}
            className="cols"
            style={{
              display: "flex",
              alignItems: "center",
              gap: 16,
              padding: "8px 0",
              borderBottom: "1px solid var(--line2)",
              fontSize: "var(--fs-base)",
            }}
          >
            <span className="col-key" style={{ ...colWidth(28), color: "var(--faint)" }}>
              {l.index}
            </span>
            <span className="col-key mono" style={{ ...colWidth(76), fontSize: "var(--fs-base)" }}>
              {l.distanceM
                ? paced
                  ? pace(l.distanceM, l.durationS)
                  : speed(l.distanceM, l.durationS)
                : duration(l.durationS)}
            </span>
            <span className="bar" style={{ flex: 1, minWidth: 60 }}>
              <span style={{ width: `${width}%` }} />
            </span>
            <span className="col" style={{ ...colWidth(68), color: "var(--mut)" }}>
              {l.avgHr ? `${Math.round(l.avgHr)} bpm` : DASH}
            </span>
            <span
              className="col"
              style={{ ...colWidth(62), color: "var(--faint)", fontSize: "var(--fs-small)" }}
            >
              {l.distanceM ? km(l.distanceM, 2) : DASH}
            </span>
          </div>
        );
      })}
    </>
  );
}

/* ------------------------------------------------------------ comparison --- */

/**
 * This session against the recent ones like it.
 *
 * Only sessions that came *before* this one count, so opening an old run shows
 * what it was compared to at the time rather than being judged against months
 * of training it predates.
 */
function Against({
  comparison: c,
  sport,
}: {
  comparison: NonNullable<ActivityAnalysis["comparison"]>;
  sport: string | null;
}) {
  const rows: Array<{ label: string; average: string; delta: number | null; pacey: boolean }> = [
    {
      label: "Pace",
      average: c.avgPaceMinKm != null ? `${paceLabel(c.avgPaceMinKm)} /km` : DASH,
      delta: c.paceDelta,
      pacey: true,
    },
    {
      label: "Avg HR",
      average: c.avgHr != null ? `${c.avgHr.toFixed(0)} bpm` : DASH,
      delta: c.hrDelta,
      pacey: false,
    },
    {
      label: "Cadence",
      average: c.avgCadence != null ? `${c.avgCadence.toFixed(0)} spm` : DASH,
      delta: c.cadenceDelta,
      pacey: false,
    },
    {
      label: "Above Z2",
      average: c.avgPercentAboveZ2 != null ? `${c.avgPercentAboveZ2.toFixed(0)}%` : DASH,
      delta: c.percentAboveZ2Delta,
      pacey: false,
    },
  ];

  const shown = rows.filter((r) => r.average !== DASH);
  if (!shown.length) return null;

  return (
    <div style={{ marginTop: 46 }}>
      <div className="section-head" style={{ marginBottom: 14 }}>
        <div className="eyebrow">Against your recent {sportLabel(sport).toLowerCase()}</div>
        <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)" }}>
          {c.sessions} earlier {c.sessions === 1 ? "session" : "sessions"}
        </div>
      </div>
      {shown.map((r) => (
        <div
          key={r.label}
          style={{
            display: "flex",
            alignItems: "baseline",
            gap: 16,
            padding: "8px 0",
            borderBottom: "1px solid var(--line2)",
            fontSize: "var(--fs-base)",
          }}
        >
          <span style={{ flex: 1 }}>{r.label}</span>
          <span className="mono" style={{ color: "var(--mut)" }}>
            {r.average}
          </span>
          <span
            className="mono"
            style={{
              width: 84,
              textAlign: "right",
              color: r.delta == null ? "var(--faint)" : "var(--fg)",
            }}
          >
            {r.delta == null ? DASH : signed(r.label, r.delta, r.pacey)}
          </span>
        </div>
      ))}
    </div>
  );
}

/** A delta with its sign, in the unit the row is measured in. */
function signed(label: string, delta: number, pacey: boolean): string {
  const sign = delta > 0 ? "+" : "−";
  const size = Math.abs(delta);
  if (pacey) return `${sign}${paceLabel(size)}`;
  if (label === "Above Z2") return `${sign}${size.toFixed(0)} pts`;
  return `${sign}${size.toFixed(0)}`;
}
