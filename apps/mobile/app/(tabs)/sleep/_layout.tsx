import { TabStack } from "../../../src/nav/TabStack";

// The list screen is the root of the tab, also after a push from outside.
export const unstable_settings = { anchor: "index" };

export default function Layout() {
  return <TabStack title="Sleep" />;
}
