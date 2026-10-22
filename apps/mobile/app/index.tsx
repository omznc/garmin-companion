import { Redirect } from "expo-router";
import { ActivityIndicator } from "react-native";
import { setupSkipped } from "../src/data/account";
import { useStatus } from "../src/data/useAccount";
import { getTabPrefs } from "../src/nav/tabPrefs";
import { startTab } from "../src/nav/tabs";

// Show Setup until the app has a Garmin sign-in. A status error goes to the start tab, which shows it.
export default function Index() {
  const { data, error } = useStatus();
  if (!data && !error) return <ActivityIndicator style={{ flex: 1 }} />;
  const ready = data?.connected || error || setupSkipped();
  return <Redirect href={ready ? `/${startTab(getTabPrefs().start).name}` : "/setup"} />;
}
