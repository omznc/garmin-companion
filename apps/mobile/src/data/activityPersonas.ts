// Dev only: EXPO_PUBLIC_PERSONA shows a recorded persona, as it does for Today.
// Only a dev build loads this file, so a release build does not ship the fixtures.
import golferList from "../../../../crates/garmin-api/fixtures/activities.golfer.json";
import gymList from "../../../../crates/garmin-api/fixtures/activities.gym.json";
import healthOnlyList from "../../../../crates/garmin-api/fixtures/activities.health_only.json";
import newUserList from "../../../../crates/garmin-api/fixtures/activities.new_user.json";
import sampleList from "../../../../crates/garmin-api/fixtures/activities.sample.json";
import golferDetail from "../../../../crates/garmin-api/fixtures/activity.golfer.json";
import gymDetail from "../../../../crates/garmin-api/fixtures/activity.gym.json";
import sampleDetail from "../../../../crates/garmin-api/fixtures/activity.sample.json";

export const activityPersonas: Record<string, { list: unknown; detail: unknown }> = {
  sample: { list: sampleList, detail: sampleDetail },
  golfer: { list: golferList, detail: golferDetail },
  gym: { list: gymList, detail: gymDetail },
  health_only: { list: healthOnlyList, detail: null },
  new_user: { list: newUserList, detail: null },
};
