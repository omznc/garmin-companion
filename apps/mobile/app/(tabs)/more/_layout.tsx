import { Stack } from "expo-router";
import { TabStack } from "../../../src/nav/TabStack";

// iOS Settings shows a small inline title on a sub-page. Set it here, before the page mounts.
const pages = {
  garmin: "Garmin account",
  sync: "Sync",
  appearance: "Appearance",
  coach: "Coach",
  about: "About",
};

export default function Layout() {
  return (
    <TabStack title="More">
      {Object.entries(pages).map(([name, title]) => (
        <Stack.Screen key={name} name={name} options={{ title, headerLargeTitle: false }} />
      ))}
    </TabStack>
  );
}
