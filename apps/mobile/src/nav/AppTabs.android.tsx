// Material 3 navigation bar as the tab bar.
import { Host } from "@expo/ui";
import { Icon, NavigationBar, NavigationBarItem, Text } from "@expo/ui/jetpack-compose";
import { Tabs, useRouter } from "expo-router";
import * as Haptics from "expo-haptics";
import { View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useHostTheme, useMd } from "../theme/md";
import { TABS } from "./tabs";

const ICONS = {
  today: require("../../assets/icons/today.xml"),
  activities: require("../../assets/icons/activities.xml"),
  ask: require("../../assets/icons/ask.xml"),
  health: require("../../assets/icons/health.xml"),
  more: require("../../assets/icons/more.xml"),
};

function TabBar({ index }: { index: number }) {
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const md = useMd();
  const host = useHostTheme();
  return (
    <View style={{ paddingBottom: insets.bottom, backgroundColor: md.surfaceContainer }}>
      <Host matchContents={{ vertical: true }} {...host} style={{ width: "100%" }}>
        <NavigationBar>
          {TABS.map((tab, i) => (
            <NavigationBarItem
              key={tab.name}
              selected={index === i}
              onClick={() => {
                if (index !== i) {
                  Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
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
  return (
    <Tabs
      screenOptions={{ headerShown: false }}
      tabBar={(props) => <TabBar index={props.state.index} />}
    >
      {TABS.map((tab) => (
        <Tabs.Screen key={tab.name} name={tab.name} options={{ title: tab.title }} />
      ))}
    </Tabs>
  );
}
