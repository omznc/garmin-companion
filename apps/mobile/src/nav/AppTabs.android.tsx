// Material 3 navigation bar as the tab bar.
import { Host } from "@expo/ui";
import { Icon, NavigationBar, NavigationBarItem, Text } from "@expo/ui/jetpack-compose";
import { Tabs, useRouter } from "expo-router";
import { View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useHostTheme, useMd } from "../theme/md";
import { launchTabPrefs, useTabPrefs } from "./tabPrefs";
import { orderedTabs, startTab } from "./tabs";

// The screens in the router state. The order is fixed at launch: the start tab, then the rest.
function launchScreens() {
  const first = startTab(launchTabPrefs.start);
  return [first, ...orderedTabs(launchTabPrefs.order).filter((t) => t.name !== first.name)];
}

const ICONS = {
  today: require("../../assets/icons/today.xml"),
  activities: require("../../assets/icons/activities.xml"),
  sleep: require("../../assets/icons/sleep.xml"),
  health: require("../../assets/icons/health.xml"),
  more: require("../../assets/icons/more.xml"),
};

// The bar draws the tabs in the saved order. The router state keeps the order of the first launch.
function TabBar({ current }: { current: string }) {
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const md = useMd();
  const host = useHostTheme();
  const prefs = useTabPrefs();
  return (
    <View style={{ paddingBottom: insets.bottom, backgroundColor: md.surfaceContainer }}>
      <Host matchContents={{ vertical: true }} {...host} style={{ width: "100%" }}>
        <NavigationBar>
          {orderedTabs(prefs.order).map((tab) => (
            <NavigationBarItem
              key={tab.name}
              selected={current === tab.name}
              onClick={() => {
                if (current !== tab.name) {
                  router.navigate(`/${tab.name}`);
                }
              }}
            >
              <NavigationBarItem.Icon>
                <Icon source={ICONS[tab.name]} size={24} contentDescription={tab.title} />
              </NavigationBarItem.Icon>
              <NavigationBarItem.Label>
                <Text style={{ typography: "labelMedium" }}>{tab.title}</Text>
              </NavigationBarItem.Label>
            </NavigationBarItem>
          ))}
        </NavigationBar>
      </Host>
    </View>
  );
}

export function AppTabs() {
  const md = useMd();
  return (
    <Tabs
      screenOptions={{ headerShown: false, sceneStyle: { backgroundColor: md.surface } }}
      tabBar={(props) => <TabBar current={props.state.routes[props.state.index]?.name ?? ""} />}
    >
      {/* The start tab comes first, so the back key from another tab returns to it. */}
      {launchScreens().map((tab) => (
        <Tabs.Screen key={tab.name} name={tab.name} options={{ title: tab.title }} />
      ))}
    </Tabs>
  );
}
