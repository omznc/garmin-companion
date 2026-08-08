/**
 * Garmin's own verdict, and the records.
 *
 * Everything on this screen is Garmin's arithmetic rather than this app's,
 * which is the point of keeping it apart from Insights: when the two agree
 * about the aerobic/anaerobic balance that agreement means something, and it
 * only means something if they were computed independently.
 *
 * The load balance is the part worth reading. Garmin scores a month of work into
 * three buckets and publishes the range it wants each one in, so "too much hard
 * running" stops being a judgement call and becomes a number against a ceiling.
 */
import { useQuery } from "@tanstack/react-query";
import { fitness, personalRecords, type PersonalRecord, type TrainingStatus } from "../lib/api";
import { RefreshButton } from "../components/Refresh";
import { Empty, ErrorNote, Loading, Metric, MetricRow, PageHeader, Rule } from "../components/ui";
import { DASH, km, parseLocal, shortDate } from "../lib/format";

/** `ANAEROBIC_FOCUS` → `Anaerobic focus`. Garmin's phrases are shouted enums. */
function nicePhrase(key: string): string {
  const words = key
    .toLowerCase()
    .replace(/_/g, " ")
    .replace(/\s+\d+$/, "");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

function hms(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.round(seconds % 60);
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${m}:${ss}`;
}

/** "06 Aug", or a dash for a record the cache has no date for. */
function setDate(date: string | null): string {
  const d = parseLocal(date);
  return d ? shortDate(d) : DASH;
}

function recordValue(r: PersonalRecord): string {
  switch (r.unit) {
    case "seconds":
      return hms(r.value);
    case "metres":
      return km(r.value);
    case "days":
      return `${Math.round(r.value)} days`;
    default:
      return Math.round(r.value).toLocaleString();
  }
}

/**
 * One load bucket against the range Garmin wants it in.
 *
 * Drawn as a track with the target band marked, because "473" means nothing
 * and "473 against a ceiling of 400" means everything.
 */
function LoadBar({
  label,
  value,
  min,
  max,
}: {
  label: string;
  value: number | null;
  min: number | null;
  max: number | null;
}) {
  if (value == null || min == null || max == null) return null;
  // Leave headroom so a value over the ceiling is visibly over it.
  const scale = Math.max(max * 1.25, value * 1.05);
  const over = value > max;
  const under = value < min;

  return (
    <div style={{ marginBottom: 16 }}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          fontSize: "var(--fs-caption)",
          marginBottom: 6,
        }}
      >
        <span style={{ color: "var(--mut)" }}>{label}</span>
        <span className="mono" style={{ color: over ? "var(--warn)" : "var(--mut)" }}>
          {Math.round(value)}
          <span style={{ color: "var(--faint)" }}>
            {" "}
            / {Math.round(min)}–{Math.round(max)}
          </span>
        </span>
      </div>
      <div style={{ position: "relative", height: 8, background: "var(--line2)", borderRadius: 2 }}>
        {/* The band Garmin is aiming for. */}
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
            background: over ? "var(--warn)" : under ? "var(--mut)" : "var(--acc)",
            borderRadius: 2,
          }}
        />
      </div>
    </div>
  );
}

function Balance({ status }: { status: TrainingStatus }) {
  const anything =
    status.aerobicLow != null || status.aerobicHigh != null || status.anaerobic != null;
  if (!anything) return null;

  return (
    <section style={{ margin: "34px 0" }}>
      <div className="section-head">Load balance, this month</div>
      <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)", margin: "6px 0 18px" }}>
        Garmin's split of the last month's work, each against the range it wants.
        {status.balancePhrase && <> Its verdict: {nicePhrase(status.balancePhrase)}.</>}
      </div>
      <LoadBar
        label="Low aerobic — easy work"
        value={status.aerobicLow}
        min={status.aerobicLowTargetMin}
        max={status.aerobicLowTargetMax}
      />
      <LoadBar
        label="High aerobic — tempo and threshold"
        value={status.aerobicHigh}
        min={status.aerobicHighTargetMin}
        max={status.aerobicHighTargetMax}
      />
      <LoadBar
        label="Anaerobic — hard and short"
        value={status.anaerobic}
        min={status.anaerobicTargetMin}
        max={status.anaerobicTargetMax}
      />
    </section>
  );
}

export function Fitness() {
  const report = useQuery({ queryKey: ["fitness", 90], queryFn: () => fitness(90) });
  const records = useQuery({ queryKey: ["personalRecords"], queryFn: personalRecords });

  if (report.isLoading) return <Loading />;
  if (report.error) return <ErrorNote error={report.error} />;

  const latest = report.data?.latest ?? null;
  // Records with no label are ones this build doesn't recognise. Kept in the
  // cache, not rendered: a number with no idea what it measures is noise.
  const named = (records.data ?? []).filter((r) => r.label);

  if (!latest && !named.length) {
    return (
      <div className="screen">
        <PageHeader eyebrow="Fitness" title="What Garmin makes of it" action={<RefreshButton />} />
        <Empty
          title="Nothing synced yet."
          body="Training status, load balance and records all arrive with a sync."
        />
      </div>
    );
  }

  const s = latest?.status;
  const p = latest?.predictions;

  return (
    <div className="screen">
      <PageHeader
        eyebrow="Fitness"
        title="What Garmin makes of it"
        lede={
          s?.statusPhrase
            ? `Garmin currently calls your training ${nicePhrase(s.statusPhrase).toLowerCase()}.`
            : undefined
        }
        action={<RefreshButton />}
      />

      <MetricRow>
        <Metric label="Acute load" value={s?.acuteLoad != null ? Math.round(s.acuteLoad) : DASH} />
        <Metric
          label="Chronic load"
          value={s?.chronicLoad != null ? Math.round(s.chronicLoad) : DASH}
        />
        <Metric
          label={
            s?.acwrStatus
              ? `Acute : chronic · ${nicePhrase(s.acwrStatus).toLowerCase()}`
              : "Acute : chronic"
          }
          value={s?.acwr != null ? s.acwr.toFixed(2) : DASH}
        />
        <Metric label="VO2 max" value={s?.vo2max != null ? Math.round(s.vo2max) : DASH} />
      </MetricRow>

      {report.data?.vo2maxMissing && (
        <div
          style={{
            fontSize: "var(--fs-small)",
            color: "var(--mut)",
            marginTop: 14,
            lineHeight: 1.5,
          }}
        >
          VO2 max is blank because Garmin only computes it from outdoor GPS runs, and every run on
          this account has been indoors. It isn't a comment on your fitness — one easy twenty
          minutes outside starts it.
        </div>
      )}

      {s && <Balance status={s} />}

      {p && (p.time5kS || p.time10kS) && (
        <>
          <Rule />
          <div className="section-head">Race predictions</div>
          <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)", margin: "6px 0 16px" }}>
            Garmin extrapolates these from heart rate and pace, so they exist even without a VO2 max
            — but every input behind them is a treadmill run, and none is longer than four
            kilometres. Read them as a direction, not a time to plan around.
          </div>
          <MetricRow>
            <Metric label="5K" value={p.time5kS ? hms(p.time5kS) : DASH} />
            <Metric label="10K" value={p.time10kS ? hms(p.time10kS) : DASH} />
            <Metric label="Half" value={p.timeHalfS ? hms(p.timeHalfS) : DASH} />
            <Metric label="Marathon" value={p.timeMarathonS ? hms(p.timeMarathonS) : DASH} />
          </MetricRow>
        </>
      )}

      {named.length > 0 && (
        <>
          <Rule />
          <div className="section-head">Records</div>
          <div style={{ marginTop: 14 }}>
            {named.map((r) => (
              <div
                key={r.recordId}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "baseline",
                  gap: 16,
                  padding: "11px 0",
                  borderTop: "1px solid var(--line2)",
                }}
              >
                <span style={{ fontSize: "var(--fs-base)" }}>{r.label}</span>
                <span style={{ display: "flex", gap: 14, alignItems: "baseline" }}>
                  <span className="mono" style={{ fontSize: "var(--fs-md)" }}>
                    {recordValue(r)}
                  </span>
                  <span
                    style={{
                      fontSize: "var(--fs-micro)",
                      color: "var(--faint)",
                      minWidth: 62,
                      textAlign: "right",
                    }}
                  >
                    {setDate(r.setOn)}
                  </span>
                </span>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
