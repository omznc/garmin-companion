// The words and the numbers of a workout card. No React here.
import type {
  DraftEnd,
  DraftExec,
  DraftKind,
  DraftSport,
  DraftTarget,
  WorkoutDraft,
} from "../data/chatTypes.ts";

export const SPORT_LABEL: Record<DraftSport, string> = {
  running: "Running",
  cycling: "Cycling",
  cardio: "Cardio",
  strength_training: "Strength",
};

export const KIND_LABEL: Record<DraftKind, string> = {
  warmup: "Warm-up",
  interval: "Interval",
  recovery: "Recovery",
  rest: "Rest",
  cooldown: "Cool-down",
};

// The steps as the watch counts them, with the repeats expanded.
export function flatCount(draft: WorkoutDraft): number {
  return draft.steps.reduce(
    (sum, step) => sum + (step.type === "exec" ? 1 : step.times * step.steps.length),
    0,
  );
}

// The total time, or null as soon as one step does not end on time.
export function estSeconds(draft: WorkoutDraft): number | null {
  let total = 0;
  for (const step of draft.steps) {
    const inner: DraftExec[] = step.type === "exec" ? [step] : step.steps;
    const times = step.type === "exec" ? 1 : step.times;
    for (const exec of inner) {
      if (exec.end.type !== "time") return null;
      total += exec.end.seconds * times;
    }
  }
  return total;
}

export function endLabel(end: DraftEnd): string {
  if (end.type === "lap_button") return "until lap press";
  if (end.type === "distance") {
    return end.metres >= 1000 && end.metres % 100 === 0
      ? `${end.metres / 1000} km`
      : `${Math.round(end.metres)} m`;
  }
  const min = Math.floor(end.seconds / 60);
  const sec = Math.round(end.seconds % 60);
  if (min === 0) return `${sec} s`;
  return sec === 0 ? `${min} min` : `${min} min ${sec} s`;
}

export function targetLabel(target: DraftTarget | undefined): string | null {
  if (!target || target.type === "none") return null;
  return target.type === "hr_zone" ? `zone ${target.zone}` : `${target.low}–${target.high} bpm`;
}

// "Running · about 45 min · 7 steps"
export function subline(draft: WorkoutDraft): string {
  const total = estSeconds(draft);
  const count = flatCount(draft);
  const parts = [SPORT_LABEL[draft.sport]];
  if (total !== null) parts.push(`about ${Math.max(1, Math.round(total / 60))} min`);
  parts.push(`${count} ${count === 1 ? "step" : "steps"}`);
  return parts.join(" · ");
}
