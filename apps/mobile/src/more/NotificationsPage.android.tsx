// Notifications on Android: a master switch and one switch for each kind.
// Android 13 and later asks for the permission when the master switch goes on.
import { Switch } from "react-native";
import { ListRow } from "../components/MdKit";
import { DENIED_NOTE, LOCAL_NOTE, useNotifySettings } from "../notify/useNotifySettings";
import { useMd } from "../theme/md";
import { Note, Page, Section } from "./Kit";

function SwitchRow({
  icon,
  title,
  supporting,
  on,
  onToggle,
}: {
  icon: "notifications" | "directions-run" | "lightbulb";
  title: string;
  supporting: string;
  on: boolean;
  onToggle: () => void;
}) {
  const md = useMd();
  return (
    <ListRow
      icon={icon}
      title={title}
      supporting={supporting}
      onPress={onToggle}
      trailing={
        // The Android switch flips itself, so it must report the change.
        <Switch
          value={on}
          onValueChange={onToggle}
          trackColor={{ false: md.surfaceContainerHighest, true: md.primary }}
          thumbColor={on ? md.onPrimary : md.outline}
        />
      }
    />
  );
}

export function NotificationsPage() {
  const { prefs, denied, toggleMaster, setKind } = useNotifySettings();
  return (
    <Page title="Notifications">
      <Section>
        <SwitchRow
          icon="notifications"
          title="Notifications"
          supporting="Off until you turn them on"
          on={prefs.enabled}
          onToggle={toggleMaster}
        />
      </Section>
      {prefs.enabled ? (
        <Section title="Tell me about">
          <SwitchRow
            icon="directions-run"
            title="New activities"
            supporting="One summary after a sync"
            on={prefs.activities}
            onToggle={() => setKind({ activities: !prefs.activities })}
          />
          <SwitchRow
            icon="lightbulb"
            title="Notes for today"
            supporting="The notes that Today marks as worth acting on"
            on={prefs.nudges}
            onToggle={() => setKind({ nudges: !prefs.nudges })}
          />
        </Section>
      ) : null}
      <Note>{denied ? DENIED_NOTE : LOCAL_NOTE}</Note>
    </Page>
  );
}
