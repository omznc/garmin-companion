// The recorded Health and Sleep views of the made-up users, for Expo Go, the web and the dev persona switch.
import healthGolfer from "../../../../crates/garmin-api/fixtures/health.golfer.json";
import healthGym from "../../../../crates/garmin-api/fixtures/health.gym.json";
import healthOnly from "../../../../crates/garmin-api/fixtures/health.health_only.json";
import healthNewUser from "../../../../crates/garmin-api/fixtures/health.new_user.json";
import healthSample from "../../../../crates/garmin-api/fixtures/health.sample.json";
import sleepGolfer from "../../../../crates/garmin-api/fixtures/sleep.golfer.json";
import sleepGym from "../../../../crates/garmin-api/fixtures/sleep.gym.json";
import sleepOnly from "../../../../crates/garmin-api/fixtures/sleep.health_only.json";
import sleepNewUser from "../../../../crates/garmin-api/fixtures/sleep.new_user.json";
import sleepSample from "../../../../crates/garmin-api/fixtures/sleep.sample.json";

const files: Record<string, Record<string, unknown>> = {
  health_view: {
    sample: healthSample,
    golfer: healthGolfer,
    gym: healthGym,
    health_only: healthOnly,
    new_user: healthNewUser,
  },
  sleep_view: {
    sample: sleepSample,
    golfer: sleepGolfer,
    gym: sleepGym,
    health_only: sleepOnly,
    new_user: sleepNewUser,
  },
};

// Dev only: EXPO_PUBLIC_PERSONA picks the user, as it does for Today.
export const devPersona = __DEV__ ? process.env.EXPO_PUBLIC_PERSONA : undefined;

/** The recorded view of a command for a persona, or `undefined` when there is none. */
export function wellnessFixture(cmd: string, persona = "sample"): unknown {
  return files[cmd]?.[persona];
}
