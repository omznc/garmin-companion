// The recorded Insights, Fitness and Reports views of the made-up users, for Expo Go and the web.
import golfer from "../../../../crates/garmin-api/fixtures/training.golfer.json";
import gym from "../../../../crates/garmin-api/fixtures/training.gym.json";
import healthOnly from "../../../../crates/garmin-api/fixtures/training.health_only.json";
import newUser from "../../../../crates/garmin-api/fixtures/training.new_user.json";
import sample from "../../../../crates/garmin-api/fixtures/training.sample.json";

const files: Record<string, unknown> = {
  sample,
  golfer,
  gym,
  health_only: healthOnly,
  new_user: newUser,
};

type Views = { insights: unknown; fitness: unknown; reports: unknown[] };

// Dev only: EXPO_PUBLIC_PERSONA picks the user, as it does for Today.
const persona = (__DEV__ ? process.env.EXPO_PUBLIC_PERSONA : undefined) ?? "sample";

/** The recorded view for a command, or `undefined` when the command is not one of these. */
export function trainingFixture(cmd: string, args?: unknown): unknown {
  const views = (files[persona] ?? sample) as Views;
  if (cmd === "insights_view") return views.insights;
  if (cmd === "fitness_view") return views.fitness;
  if (cmd !== "report_view") return undefined;
  const offset = (args as { offset?: number } | undefined)?.offset ?? 0;
  return views.reports[Math.min(offset, views.reports.length - 1)];
}
