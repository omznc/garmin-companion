import { useQuery } from "@tanstack/react-query";
import {
  cachedActivitiesSince,
  cachedDaily,
  type CachedActivity,
} from "../lib/api";
import { acuteChronic, dailySeries, easyHardSplit, insights } from "../lib/derive";
import {
  Bullet,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  Metric,
  PageHeader,
} from "../components/ui";
import { RefreshButton } from "../components/Refresh";
import { daysAgo, isRun, num } from "../lib/format";

export function Insights() {
  const daily = useQuery({ queryKey: ["daily", 365], queryFn: () => cachedDaily(365) });
  const acts = useQuery({
    queryKey: ["activitiesSince", 365],
    queryFn: () => cachedActivitiesSince(daysAgo(365)),
  });

  if (daily.isLoading || acts.isLoading) return <Loading label="Crunching the cache" />;
  if (daily.error) return <ErrorNote error={daily.error} />;
  if (acts.error) return <ErrorNote error={acts.error} />;

  const rows = dailySeries(daily.data ?? [], 365);
  const activities = acts.data ?? [];
  const found = insights(rows, activities);
  const load = acuteChronic(activities);
  const flags = riskFlags(activities);

  const nothing = !found.length && !load && !flags.length;

  return (
    <div className="screen">
      <PageHeader
        eyebrow="Last 365 days"
        title="Insights"
        lede="Correlations found in your own cached data. Correlations, not causes — each states its sample size so you can judge how much to believe it."
        action={<RefreshButton />}
        space={54}
      />

      {nothing && (
        <Empty
          title="Not enough history yet."
          body="These are computed from paired daily metrics and activities. A few more weeks of synced data and the first ones will appear — nothing here is shown until the arithmetic supports it."
        />
      )}

      {found.map((i, idx) => (
        <div key={idx} style={{ marginBottom: 54 }}>
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
            {i.claim}
          </div>
          <div style={{ display: "flex", gap: 32, alignItems: "flex-end" }}>
            <div
              style={{ flex: 1, fontSize: "var(--fs-md)", lineHeight: 1.65, color: "var(--mut)", textWrap: "pretty" }}
            >
              {i.detail}
            </div>
            <div style={{ width: 260, flex: "none" }}>
              <LineChart
                series={[
                  { ...i.a, stroke: "var(--acc)" },
                  { ...i.b, stroke: "var(--faint)", width: 1, dashed: true },
                ]}
                height={70}
                viewWidth={260}
                pad={6}
              />
            </div>
          </div>
          <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 10 }}>
            {i.basis}
          </div>
        </div>
      ))}

      {load && (
        <>
          <div className="eyebrow" style={{ marginBottom: 20 }}>
            Training load
          </div>
          <div style={{ display: "flex", gap: 48, alignItems: "flex-end", marginBottom: 16 }}>
            <Metric
              size={34}
              label="Acute / chronic"
              value={load.ratio.toFixed(2)}
              accent={load.ratio > 1.5}
            />
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
            {load.acute.toFixed(1)} h of training in the last seven days against a
            28-day weekly average of {load.chronic.toFixed(1)} h.{" "}
            {load.ratio > 1.5
              ? "That's a sharp step up — the range where injury risk climbs."
              : load.ratio < 0.8
                ? "You're training less than your recent norm, which is what a down week looks like."
                : "That's a steady ratio."}
          </div>
        </>
      )}

      {flags.length > 0 && (
        <>
          <div className="eyebrow" style={{ margin: "60px 0 16px" }}>
            Risk flags
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 13 }}>
            {flags.map((f, i) => (
              <Bullet key={i} accent={f.accent}>
                {f.text}
              </Bullet>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

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

  const cadences = recent
    .map((a) => a.avgCadence)
    .filter((c): c is number => c != null);
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
