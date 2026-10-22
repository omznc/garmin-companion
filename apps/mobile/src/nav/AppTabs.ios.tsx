// Native iOS tab bar. A native bar cannot reorder at run time, so it uses the order of the
// launch. A new order shows the next time the app opens.
import { NativeTabs } from "expo-router/native-tabs";
import { launchTabPrefs } from "./tabPrefs";
import { orderedTabs } from "./tabs";

export function AppTabs() {
  return (
    <NativeTabs>
      {orderedTabs(launchTabPrefs.order).map((tab) => (
        <NativeTabs.Trigger key={tab.name} name={tab.name}>
          <NativeTabs.Trigger.Label>{tab.title}</NativeTabs.Trigger.Label>
          <NativeTabs.Trigger.Icon sf={{ default: tab.sf, selected: tab.sfSelected }} />
        </NativeTabs.Trigger>
      ))}
    </NativeTabs>
  );
}
