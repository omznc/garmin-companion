// Native iOS tab bar. UNVERIFIED until it runs on the iOS Simulator.
import { NativeTabs } from "expo-router/native-tabs";
import { TABS } from "./tabs";

export function AppTabs() {
  return (
    <NativeTabs>
      {TABS.map((tab) => (
        <NativeTabs.Trigger key={tab.name} name={tab.name}>
          <NativeTabs.Trigger.Label>{tab.title}</NativeTabs.Trigger.Label>
          <NativeTabs.Trigger.Icon sf={{ default: tab.sf, selected: tab.sfSelected }} />
        </NativeTabs.Trigger>
      ))}
    </NativeTabs>
  );
}
