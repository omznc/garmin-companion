// The recorded Plan, Routes and Gear views of the made-up users, for Expo Go, the web and the dev persona switch.
import golfer from "../../../../crates/garmin-api/fixtures/places.golfer.json";
import gym from "../../../../crates/garmin-api/fixtures/places.gym.json";
import healthOnly from "../../../../crates/garmin-api/fixtures/places.health_only.json";
import newUser from "../../../../crates/garmin-api/fixtures/places.new_user.json";
import sample from "../../../../crates/garmin-api/fixtures/places.sample.json";

const personas = { sample, golfer, gym, health_only: healthOnly, new_user: newUser };
const commands = { plan_view: "plan", routes_view: "routes", gear_view: "gear" } as const;

/** The recorded view of a command for a persona, or `undefined` when there is none. */
export function placesFixture(cmd: string, persona = "sample"): unknown {
  const part = commands[cmd as keyof typeof commands];
  const views = personas[persona as keyof typeof personas] ?? sample;
  return part ? views[part] : undefined;
}
