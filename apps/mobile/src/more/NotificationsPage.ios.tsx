// Notifications on iOS: a master switch and one switch for each kind.
import { DENIED_NOTE, LOCAL_NOTE, useNotifySettings } from "../notify/useNotifySettings";
import { Item, Note, Page, Section } from "./Kit";

export function NotificationsPage() {
  const { prefs, denied, toggleMaster, setKind } = useNotifySettings();
  return (
    <Page title="Notifications">
      <Section>
        <Item title="Notifications" on={prefs.enabled} onPress={toggleMaster} />
      </Section>
      {prefs.enabled ? (
        <Section title="Tell me about">
          <Item
            title="New activities"
            on={prefs.activities}
            onPress={() => setKind({ activities: !prefs.activities })}
          />
          <Item
            title="Notes for today"
            on={prefs.nudges}
            onPress={() => setKind({ nudges: !prefs.nudges })}
          />
        </Section>
      ) : null}
      <Note>{denied ? DENIED_NOTE : LOCAL_NOTE}</Note>
    </Page>
  );
}
