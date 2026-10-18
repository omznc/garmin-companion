// The pages of More: one for each settings area. The core builds the sentences.
import { useRouter } from "expo-router";
import { useState } from "react";
import { Linking, Platform, TextInput, View } from "react-native";
import { needsSetup, saveKey, useCoachConfig } from "../data/useAsk";
import { THEMES, useSettingsActions } from "../data/useSettingsActions";
import { Item, Note, Page, Section, useColors } from "./Kit";

const REPO = "https://github.com/omznc/garmin-companion";

export function providerName(provider: string | null): string {
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
              <Item title="Sign out" tone="danger" disabled={a.sync.busy} onPress={a.disconnect} />
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
  const busy = a.sync.busy;
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

function KeyField({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const c = useColors();
  return (
    <View
      style={{
        marginHorizontal: Platform.OS === "android" ? 0 : 16,
        marginTop: Platform.OS === "android" ? 12 : 24,
      }}
    >
      <TextInput
        value={value}
        onChangeText={onChange}
        placeholder="OpenRouter API key"
        placeholderTextColor={c.muted}
        secureTextEntry
        autoCapitalize="none"
        autoCorrect={false}
        style={{
          color: c.text,
          backgroundColor: c.field,
          borderRadius: 12,
          paddingHorizontal: 14,
          paddingVertical: 12,
          fontSize: 16,
        }}
      />
    </View>
  );
}

export function CoachPage() {
  const config = useCoachConfig();
  const c = config.data;
  const [key, setKey] = useState("");
  const [problem, setProblem] = useState("");
  const save = async () => {
    try {
      await saveKey(key.trim());
      setKey("");
      setProblem("");
      await config.refetch();
    } catch (error) {
      setProblem((error as Error).message);
    }
  };
  return (
    <Page title="Coach">
      <Section title="Model">
        <Item title="Provider" value={providerName(c?.provider ?? null)} />
        {c?.model ? <Item title="Model" value={c.model} /> : null}
        <Item title="OpenRouter key" value={c?.hasKey ? "Saved" : "Not set"} />
      </Section>
      {needsSetup(c) ? (
        <Note>
          The coach needs a model. Paste your own OpenRouter key. It stays on this phone, in the
          app's private store.
        </Note>
      ) : null}
      <KeyField value={key} onChange={setKey} />
      <Section>
        <Item title="Save key" tone="action" disabled={!key.trim()} onPress={save} />
      </Section>
      {problem ? <Note>{problem}</Note> : null}
      <Note>Ollama runs on a computer, not on a phone. Use OpenRouter here.</Note>
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
          <Item title="Version" value={s.version} />
          <Item title="Open source" chevron onPress={() => Linking.openURL(REPO)} />
          {s.dataFolder ? <Item title="Data folder" value={s.dataFolder} /> : null}
        </Section>
      ) : null}
      <Problem message={a.settings.error} />
    </Page>
  );
}
