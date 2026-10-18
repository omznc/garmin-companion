// Turn the core's Today numbers into the labels that the screens draw.
import type { Activity, LastSession, Metric, Today, TodayView } from "./types";

// "treadmill_running" becomes "Treadmill running". The core may send a label later.
const humanize = (key: string | null) =>
  key ? key.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase()) : "Activity";

const pad = (n: number) => String(n).padStart(2, "0");
const parseDay = (s: string) => new Date(`${s.slice(0, 10)}T12:00:00`);

export function hoursMinutes(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.round((secs % 3600) / 60);
  return `${h}h ${pad(m)}m`;
}

export function clock(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.round(secs % 60);
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

function greeting(now: Date): string {
  const h = now.getHours();
  if (h < 5) return "Good night";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

function metrics(v: TodayView): Metric[] {
  const num = (l: { value: number }) => String(Math.round(l.value));
  const rhr = v.restingHr;
  const delta = rhr ? Math.round(rhr.delta) : 0;
  const balance = v.fuel.balance;
  // A tile with no data is left out. Each section stands alone.
  const all: (Metric | null)[] = [
    v.sleep ? { id: "sleep", label: "Sleep", value: hoursMinutes(v.sleep.value) } : null,
    v.hrv ? { id: "hrv", label: "HRV", value: num(v.hrv), unit: "ms" } : null,
    rhr
      ? {
          id: "rhr",
          label: `Resting HR, ${delta > 0 ? "+" : ""}${delta}`,
          value: num(rhr),
          unit: "bpm",
          accent: rhr.accent,
        }
      : null,
    v.bodyBattery ? { id: "battery", label: "Body battery", value: num(v.bodyBattery) } : null,
    v.readiness ? { id: "readiness", label: "Readiness", value: num(v.readiness) } : null,
    balance == null
      ? null
      : {
          id: "fuel",
          label: "Fuel balance",
          value: `${balance > 0 ? "+" : ""}${Math.round(balance)}`,
          unit: "kcal",
          accent: v.fuel.accent,
        },
  ];
  return all.filter((m): m is Metric => m !== null);
}

function sessionStats(a: Activity): LastSession["stats"] {
  const secs = a.movingDurationS ?? a.durationS;
  const stats: [string, string | null][] = [
    ["Distance", a.distanceM ? `${(a.distanceM / 1000).toFixed(1)} km` : null],
    ["Time", secs ? clock(secs) : null],
    ["Avg HR", a.avgHr ? String(Math.round(a.avgHr)) : null],
    ["Max HR", a.maxHr ? String(Math.round(a.maxHr)) : null],
    ["Aerobic TE", a.aerobicTe ? a.aerobicTe.toFixed(1) : null],
  ];
  return stats.flatMap(([label, value]) => (value ? [{ label, value }] : []));
}

function zonePercents(a: Activity): number[] {
  const secs = a.zoneSecs ?? [];
  const total = secs.reduce((x, y) => x + y, 0);
  return total > 0 ? secs.map((s) => (s / total) * 100) : [];
}

const shortDate = (s: string) =>
  parseDay(s).toLocaleDateString("en-GB", { day: "2-digit", month: "short" });

function lastSession(v: TodayView): LastSession | null {
  if (!v.lastSession) return null;
  const a = v.lastSession.activity;
  const sport = humanize(a.typeKey);
  return {
    name: a.name ?? sport,
    sport,
    when: shortDate(a.startTimeLocal ?? a.localDate),
    aboveZ2Pct: v.lastSession.hasZoneData ? Math.round(v.lastSession.aboveZ2Pct) : null,
    zones: zonePercents(a),
    stats: sessionStats(a),
  };
}

function weekLabels(today: Date): string[] {
  return Array.from({ length: 7 }, (_, i) => {
    const d = new Date(today);
    d.setDate(d.getDate() - (6 - i));
    return d.toLocaleDateString("en-GB", { weekday: "short" });
  });
}

export function presentToday(v: TodayView, now = new Date()): Today {
  const drift = v.zoneDrift;
  const day = parseDay(v.date);
  return {
    dateLabel: day.toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" }),
    greeting: greeting(now),
    lede: v.empty ? "Nothing cached yet. Run a sync to pull your Garmin history." : v.lede,
    metrics: v.empty ? [] : metrics(v),
    fuelLine: [v.fuelSentence, v.hydrationSentence].filter(Boolean).join(" "),
    attention: v.attention,
    lastSession: lastSession(v),
    zoneDrift:
      drift.runs.length >= 2 && drift.recentAvgPct != null
        ? {
            summary: `${Math.round(drift.recentAvgPct)}% above Z2 lately, ${Math.round(drift.targetPct)}% is the target`,
            runs: drift.runs.slice(-6).map((r) => ({
              id: String(r.activityId),
              label: r.date ? shortDate(r.date) : "",
              aboveZ2Pct: r.aboveZ2Pct,
            })),
          }
        : null,
    week:
      v.week.sessions > 0 || v.week.totalKm > 0
        ? { labels: weekLabels(day), km: v.week.dailyKm, summary: v.week.sentence }
        : null,
  };
}
