// Insights, Fitness and the weekly report. The core builds the numbers, the sentences and
// the choice of sections (`garmin_api::insights`, `fitness`, `reports`). This file types the
// JSON and turns it into the blocks that the Body screens draw. It has no other copy.
import type { Block, BodyScreenData, EmptyNote } from "./body";

export type Figure = { label: string; value: string; accent: boolean };

export type InsightItem = {
  id: string;
  claim: string;
  detail: string;
  basis: string | null;
  figures: Figure[];
  series: { name: string; values: (number | null)[]; muted: boolean }[];
  ends: string[];
  rows: { label: string; value: string; note: string | null; accent: boolean }[];
};

export type InsightsView = {
  eyebrow: string;
  lede: string;
  empty: EmptyNote | null;
  sections: { key: string; title: string; items: InsightItem[] }[];
};

export type FitnessView = {
  empty: EmptyNote | null;
  lede: string | null;
  figures: Figure[];
  notes: string[];
  balance: {
    intro: string;
    bars: { label: string; state: "over" | "under" | "in"; text: string }[];
  } | null;
  predictions: { intro: string; rows: Figure[] } | null;
  records: { label: string; value: string; date: string }[];
};

export type ReportView = {
  empty: EmptyNote | null;
  lede: string;
  hasEarlier: boolean;
  hasLater: boolean;
  eyebrow: string;
  headline: string;
  paragraphs: string[];
  figures: Figure[];
  dailyTitle: string;
  daily: { label: string; value: number; text: string }[];
  sessions: { name: string; day: string; sport: string; distance: string; duration: string }[];
};

const tiles = (figures: Figure[]): Block[] =>
  figures.length
    ? [
        {
          kind: "tiles",
          tiles: figures.map((f) => ({ label: f.label, value: f.value, accent: f.accent })),
        },
      ]
    : [];

const note = (text: string): Block => ({ kind: "text", text, tone: "note" });
const heading = (text: string): Block => ({ kind: "text", text, tone: "heading" });

function insightBlocks(item: InsightItem): Block[] {
  const line = item.series.find((s) => !s.muted);
  const values = (line?.values ?? []).filter((v): v is number => v != null);
  return [
    { kind: "text", text: item.claim },
    note(item.detail),
    ...tiles(item.figures),
    ...(values.length > 1
      ? [
          {
            kind: "chart" as const,
            title: line?.name ?? "Trend",
            values,
            from: item.ends[0] ?? "",
            to: item.ends[1] ?? "",
          },
        ]
      : []),
    ...(item.rows.length
      ? [
          {
            kind: "rows" as const,
            title: "Detail",
            rows: item.rows.map((r) => ({
              key: r.label,
              value: r.value,
              sub: r.note ?? undefined,
            })),
          },
        ]
      : []),
    ...(item.basis ? [note(item.basis)] : []),
  ];
}

export function presentInsights(v: InsightsView): BodyScreenData {
  const title = "Insights";
  if (v.empty) return { title, blocks: [note(v.lede), { kind: "empty", ...v.empty }] };
  return {
    title,
    blocks: [
      note(v.lede),
      ...v.sections.flatMap((s) => [heading(s.title), ...s.items.flatMap(insightBlocks)]),
    ],
  };
}

const RANGE: Record<string, string> = {
  over: "above the range Garmin wants",
  under: "below the range Garmin wants",
  in: "inside the range Garmin wants",
};

export function presentFitness(v: FitnessView): BodyScreenData {
  const title = "Fitness";
  if (v.empty) return { title, blocks: [{ kind: "empty", ...v.empty }] };
  return {
    title,
    blocks: [
      ...(v.lede ? [{ kind: "text" as const, text: v.lede, tone: "lede" as const }] : []),
      ...tiles(v.figures),
      ...v.notes.map(note),
      ...(v.balance
        ? [
            heading("Load balance, this month"),
            note(v.balance.intro),
            {
              kind: "rows" as const,
              title: "This month",
              rows: v.balance.bars.map((b) => ({
                key: b.label,
                value: b.text,
                sub: RANGE[b.state],
              })),
            },
          ]
        : []),
      ...(v.predictions
        ? [heading("Race predictions"), note(v.predictions.intro), ...tiles(v.predictions.rows)]
        : []),
      ...(v.records.length
        ? [
            {
              kind: "rows" as const,
              title: "Records",
              rows: v.records.map((r) => ({
                key: r.label,
                value: r.value,
                sub: r.date || undefined,
              })),
            },
          ]
        : []),
    ],
  };
}

export function presentReport(v: ReportView): BodyScreenData {
  const title = "The Weekly";
  if (v.empty) return { title, blocks: [note(v.lede), { kind: "empty", ...v.empty }] };
  const first = v.daily[0];
  const last = v.daily[v.daily.length - 1];
  const actions = [
    ...(v.hasEarlier ? [{ id: "earlier", label: "Earlier week" }] : []),
    ...(v.hasLater ? [{ id: "later", label: "Later week" }] : []),
  ];
  return {
    title,
    blocks: [
      note(v.eyebrow),
      { kind: "text", text: v.headline, tone: "lede" },
      ...tiles(v.figures),
      ...v.paragraphs.map(
        (text, i): Block => ({ kind: "text", text, tone: i ? "note" : undefined }),
      ),
      ...(first && last
        ? [
            {
              kind: "chart" as const,
              title: v.dailyTitle,
              values: v.daily.map((d) => d.value),
              from: first.label,
              to: last.label,
            },
          ]
        : []),
      {
        kind: "rows",
        title: "Sessions",
        rows: v.sessions.map((s) => ({
          key: s.name,
          value: s.distance || s.duration,
          sub: [s.day, s.sport, s.distance ? s.duration : ""].filter(Boolean).join(", "),
        })),
      },
      ...(actions.length ? [{ kind: "actions" as const, actions }] : []),
    ],
  };
}
