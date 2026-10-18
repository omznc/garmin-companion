// Turn the core's Today view into the sections that the screens draw.
// The core sends the order and the sentences. This file only formats numbers.
import type { Activity, Hero, LastSession, Metric, Section, Today, TodayView } from "./types";

const pad = (n: number) => String(n).padStart(2, "0");
const parseDay = (s: string) => new Date(`${s.slice(0, 10)}T12:00:00`);

export function hoursMinutes(secs: number): string {
  const total = Math.round(secs / 60);
  return `${Math.floor(total / 60)}h ${pad(total % 60)}m`;
}

export function clock(secs: number): string {
  const total = Math.round(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  return h > 0 ? `${h}:${pad(m)}:${pad(total % 60)}` : `${m}:${pad(total % 60)}`;
}

function greeting(now: Date): string {
  const h = now.getHours();
  if (h < 5) return "Good night";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

function healthMetrics(v: TodayView): Metric[] {
  const whole = (l: { value: number }) => Math.round(l.value).toLocaleString("en-US");
  const rhr = v.restingHr;
  const delta = rhr ? Math.round(rhr.delta) : 0;
  // A tile with no data is left out.
  const all: (Metric | null)[] = [
    v.sleep ? { id: "sleep", label: "Sleep", value: hoursMinutes(v.sleep.value) } : null,
    v.hrv ? { id: "hrv", label: "HRV", value: whole(v.hrv), unit: "ms" } : null,
    rhr
      ? {
          id: "rhr",
          label: `Resting HR, ${delta > 0 ? "+" : ""}${delta}`,
          value: whole(rhr),
          unit: "bpm",
          accent: rhr.accent,
        }
      : null,
    v.bodyBattery ? { id: "battery", label: "Body battery", value: whole(v.bodyBattery) } : null,
    v.readiness ? { id: "readiness", label: "Readiness", value: whole(v.readiness) } : null,
    v.steps ? { id: "steps", label: "Steps", value: whole(v.steps) } : null,
    v.stress ? { id: "stress", label: "Stress", value: whole(v.stress) } : null,
  ];
  return all.filter((m): m is Metric => m !== null);
}

function sessionStats(a: Activity): LastSession["stats"] {
  const secs = a.movingDurationS ?? a.durationS;
  const stats: [string, string | null][] = [
    ["Distance", a.distanceM ? `${(a.distanceM / 1000).toFixed(1)} km` : null],
    ["Time", secs ? clock(secs) : null],
    ["Steps", a.steps ? Math.round(a.steps).toLocaleString("en-US") : null],
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

function shortDate(s: string | null): string {
  return s ? parseDay(s).toLocaleDateString("en-GB", { day: "2-digit", month: "short" }) : "";
}

function lastSession(v: TodayView): LastSession | null {
  const last = v.lastSession;
  if (!last) return null;
  const a = last.activity;
  const percents = zonePercents(a);
  return {
    name: a.name?.trim() || last.sportLabel,
    sport: last.sportLabel,
    typeKey: a.typeKey,
    when: shortDate(a.startTimeLocal ?? a.localDate),
    zones:
      last.showZoneVerdict && last.hasZoneData && percents.length > 0
        ? { aboveZ2Pct: Math.round(last.aboveZ2Pct), percents }
        : null,
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

function driftSection(v: TodayView): Section | null {
  const drift = v.zoneDrift;
  if (drift.runs.length < 2 || drift.recentAvgPct == null) return null;
  return {
    kind: "drift",
    summary: `${Math.round(drift.recentAvgPct)}% above Z2 lately, ${Math.round(drift.targetPct)}% is the target`,
    runs: drift.runs.slice(-6).map((r) => ({
      id: String(r.activityId),
      label: shortDate(r.date),
      aboveZ2Pct: r.aboveZ2Pct,
    })),
  };
}

function buildSection(v: TodayView, key: string, day: Date): Section | null {
  switch (key) {
    case "health_tiles": {
      const metrics = healthMetrics(v);
      return metrics.length ? { kind: "health", metrics } : null;
    }
    case "attention":
      return v.attention.length ? { kind: "attention", notes: v.attention } : null;
    case "fuel": {
      const text = [v.fuelSentence, v.hydrationSentence].filter(Boolean).join(" ");
      return text ? { kind: "text", key: "fuel", text } : null;
    }
    case "weight": {
      return v.weight ? { kind: "text", key: "weight", text: v.weight.sentence } : null;
    }
    case "last_session": {
      const session = lastSession(v);
      return session ? { kind: "session", session } : null;
    }
    case "week":
      return {
        kind: "week",
        labels: weekLabels(day),
        minutes: v.week.dailyMinutes,
        summary: v.week.sentence,
      };
    case "zone_drift":
      return driftSection(v);
    default:
      // A section from a newer core is skipped, not an error.
      return null;
  }
}

function hero(v: TodayView): Hero | null {
  const score = (id: string, label: string, l: { value: number }): Hero => ({
    id,
    label,
    text: String(Math.round(l.value)),
    pct: Math.max(0, Math.min(100, l.value)),
  });
  if (v.readiness) return score("readiness", "Readiness", v.readiness);
  if (v.bodyBattery) return score("battery", "Body battery", v.bodyBattery);
  if (v.sleep) return { id: "sleep", label: "Slept", text: hoursMinutes(v.sleep.value), pct: null };
  return null;
}

export function presentToday(v: TodayView, now = new Date()): Today {
  const day = parseDay(v.date);
  return {
    hero: v.empty ? null : hero(v),
    dateLabel: day.toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" }),
    greeting: greeting(now),
    lede: v.empty ? "Nothing cached yet. Run a sync to pull your Garmin history." : v.lede,
    sections: v.sections.flatMap((key) => buildSection(v, key, day) ?? []),
  };
}
