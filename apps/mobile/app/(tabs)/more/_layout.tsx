import { Stack } from "expo-router";
import { TabStack } from "../../../src/nav/TabStack";

// Each sub-page takes its title here, before the page mounts. It uses the large title of the stack.
const pages = {
  garmin: "Garmin account",
  sync: "Sync",
  notifications: "Notifications",
  appearance: "Appearance",
  coach: "Coach",
  about: "About",
};

export default function Layout() {
  return (
    <TabStack title="More">
      {Object.entries(pages).map(([name, title]) => (
        <Stack.Screen key={name} name={name} options={{ title }} />
      ))}
    </TabStack>
  );
}
