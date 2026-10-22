import { TabStack } from "../../../src/nav/TabStack";

// A push into this stack from outside (a notification tap) keeps the list screen under the target.
export const unstable_settings = { anchor: "index" };

export default function Layout() {
  return <TabStack title="Activities" />;
}
