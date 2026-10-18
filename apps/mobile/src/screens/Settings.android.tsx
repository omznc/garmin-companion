// Settings on Android: Material 3 cards, buttons and a segmented choice.
import { Host } from "@expo/ui";
import {
  Button,
  Card,
  Column,
  OutlinedButton,
  SegmentedButton,
  SingleChoiceSegmentedButtonRow,
  Text,
} from "@expo/ui/jetpack-compose";
import { fillMaxWidth, paddingAll } from "@expo/ui/jetpack-compose/modifiers";
import type { ReactNode } from "react";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import type { SettingsView } from "../data/types";
import { THEMES, useSettingsActions } from "../data/useSettingsActions";
import { HOST_SEED, useMd } from "../theme/md";

type Md = ReturnType<typeof useMd>;

function Section({ md, title, children }: { md: Md; title: string; children: ReactNode }) {
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column verticalArrangement={{ spacedBy: 8 }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
        <Text style={{ typography: "titleMedium" }} color={md.onSurface}>
          {title}
        </Text>
        <Card colors={{ containerColor: md.surfaceContainerLow }}>
          <Column
            verticalArrangement={{ spacedBy: 12 }}
            modifiers={[fillMaxWidth(), paddingAll(16)]}
          >
            {children}
          </Column>
        </Card>
      </Column>
    </Host>
  );
}

function Body({ md, children }: { md: Md; children: string }) {
  return (
    <Text style={{ typography: "bodyMedium" }} color={md.onSurface}>
      {children}
    </Text>
  );
}

function Garmin({
  md,
  s,
  a,
}: {
  md: Md;
  s: SettingsView;
  a: ReturnType<typeof useSettingsActions>;
}) {
  const busy = a.sync.busy;
  return (
    <Section md={md} title="Garmin">
      <Body md={md}>{s.accountLine}</Body>
      <Body md={md}>{s.cacheLine}</Body>
      {a.sync.line ? <Body md={md}>{a.sync.line}</Body> : null}
      {a.sync.error ? (
        <Text style={{ typography: "bodyMedium" }} color={md.error}>
          {a.sync.error}
        </Text>
      ) : null}
      <Button enabled={!busy} onClick={() => a.runSync({})} modifiers={[fillMaxWidth()]}>
        <Text>Sync recent</Text>
      </Button>
      <OutlinedButton
        enabled={!busy}
        onClick={() => a.runSync({ full: true, days: 365 })}
        modifiers={[fillMaxWidth()]}
      >
        <Text>Full re-sync</Text>
      </OutlinedButton>
      {s.connected ? (
        <OutlinedButton onClick={a.disconnect} modifiers={[fillMaxWidth()]}>
          <Text color={md.error}>Disconnect</Text>
        </OutlinedButton>
      ) : null}
    </Section>
  );
}

function Appearance({
  md,
  s,
  a,
}: {
  md: Md;
  s: SettingsView;
  a: ReturnType<typeof useSettingsActions>;
}) {
  return (
    <Section md={md} title="Appearance">
      <SingleChoiceSegmentedButtonRow modifiers={[fillMaxWidth()]}>
        {THEMES.map((t) => (
          <SegmentedButton
            key={t.value}
            selected={s.theme === t.value}
            onClick={() => a.chooseTheme(t.value)}
          >
            <SegmentedButton.Label>
              <Text>{t.label}</Text>
            </SegmentedButton.Label>
          </SegmentedButton>
        ))}
      </SingleChoiceSegmentedButtonRow>
    </Section>
  );
}

function SettingsBody({
  md,
  s,
  a,
}: {
  md: Md;
  s: SettingsView;
  a: ReturnType<typeof useSettingsActions>;
}) {
  return (
    <>
      <Garmin md={md} s={s} a={a} />
      <Appearance md={md} s={s} a={a} />
      <Section md={md} title="What leaves this device">
        <Body md={md}>{s.privacyLine}</Body>
      </Section>
      <Section md={md} title="About">
        <Body md={md}>{`Version ${s.version}`}</Body>
        {s.dataFolder ? <Body md={md}>{s.dataFolder}</Body> : null}
      </Section>
    </>
  );
}

export function SettingsScreen() {
  const md = useMd();
  const a = useSettingsActions();
  const s = a.settings.data;
  return (
    <LargeAppBarScreen title="Settings" settings={false}>
      {s ? <SettingsBody md={md} s={s} a={a} /> : null}
      {a.settings.error ? (
        <Section md={md} title="Settings">
          <Body md={md}>{String(a.settings.error)}</Body>
        </Section>
      ) : null}
    </LargeAppBarScreen>
  );
}
