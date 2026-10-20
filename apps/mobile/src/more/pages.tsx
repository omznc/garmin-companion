// The pages of More: one for each settings area. The core builds the sentences.
import Constants from "expo-constants";
import { useRouter } from "expo-router";
import { Linking } from "react-native";
import { THEMES, useSettingsActions } from "../data/useSettingsActions";
import { Item, Note, Page, Section } from "./Kit";

const REPO = "https://github.com/omznc/garmin-companion";

export function providerName(provider: string | null): string {
  if (provider === "cloud") return "Cloud";
  if (provider === "openrouter") return "OpenRouter";
  if (provider === "ollama") return "Ollama";
  return "Not set";
}

function Problem({ message }: { message: unknown }) {
  return message ? (
    <Section title="Problem">
      <Item title={String(message)} />
    </Section>
  ) : null;
}

export function GarminPage() {
  const router = useRouter();
  const a = useSettingsActions();
  const s = a.settings.data;
  return (
    <Page title="Garmin account">
      {s ? (
        <>
          <Section>
            <Item title={s.accountLine} />
            {s.connected ? (
              <Item
                title="Sign out"
                tone="danger"
                disabled={a.sync.busy || a.sync.importing}
                onPress={a.disconnect}
              />
            ) : (
              <Item title="Sign in" tone="action" onPress={() => router.push("/sign-in")} />
            )}
          </Section>
          <Note>{s.privacyLine}</Note>
        </>
      ) : null}
      <Problem message={a.settings.error} />
    </Page>
  );
}

export function SyncPage() {
  const a = useSettingsActions();
  const s = a.settings.data;
  const busy = a.sync.busy || a.sync.importing;
  return (
    <Page title="Sync">
      <Section>
        {s ? <Item title={s.cacheLine} /> : null}
        {a.sync.line ? <Item title={a.sync.line} /> : null}
        {a.sync.error ? <Item title={a.sync.error} tone="danger" /> : null}
      </Section>
      <Section>
        <Item title="Sync recent" tone="action" disabled={busy} onPress={() => a.runSync({})} />
        <Item
          title="Full re-sync"
          tone="action"
          disabled={busy}
          onPress={() => a.runSync({ full: true, days: 365 })}
        />
      </Section>
      <Problem message={a.settings.error} />
    </Page>
  );
}

export function AppearancePage() {
  const a = useSettingsActions();
  const theme = a.settings.data?.theme;
  return (
    <Page title="Appearance">
      <Section title="Theme">
        {THEMES.map((t) => (
          <Item
            key={t.value}
            title={t.label}
            checked={theme === t.value}
            onPress={() => a.chooseTheme(t.value)}
          />
        ))}
      </Section>
    </Page>
  );
}

export function AboutPage() {
  const a = useSettingsActions();
  const s = a.settings.data;
  return (
    <Page title="About">
      {s ? (
        <Section>
          {/* The app version, as the stores and the updater see it. s.version is the Rust crate. */}
          <Item title="Version" value={Constants.expoConfig?.version ?? s.version} />
          <Item title="Open source" chevron onPress={() => Linking.openURL(REPO)} />
          {s.dataFolder ? <Item title="Data folder" value={s.dataFolder} /> : null}
        </Section>
      ) : null}
      <Problem message={a.settings.error} />
    </Page>
  );
}
