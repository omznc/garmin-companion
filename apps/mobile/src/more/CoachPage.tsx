// More > Coach: where the coach model runs, the cloud balance and the email sign-in.
// The same page on both platforms. The list pieces come from the platform Kit.
// Premium purchase comes with the stores. Until then the page explains it and sells nothing.
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { type KeyboardTypeOptions, Platform, TextInput, View } from "react-native";
import { needsSetup, saveKey, useCoachConfig } from "../data/useAsk";
import {
  type CloudAccount,
  PREMIUM_LATER,
  chooseProvider,
  sendCode,
  sentence,
  signOutCloud,
  useCloudAccount,
  verifyCode,
} from "../data/useCloud";
import { Item, Note, Page, Section, useColors } from "./Kit";

const WHERE = {
  cloud:
    "No setup. Free use gives 30 credits a day. Only the rows that a question needs leave this phone.",
  openrouter: "OpenRouter bills you. The key stays on this phone, in the app's private store.",
  ollama: "Ollama runs on a computer, not on a phone.",
} as const;

function Field(props: {
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
  secure?: boolean;
  keyboard?: KeyboardTypeOptions;
}) {
  const c = useColors();
  return (
    <View
      style={{
        marginHorizontal: Platform.OS === "android" ? 0 : 16,
        marginTop: Platform.OS === "android" ? 12 : 24,
      }}
    >
      <TextInput
        value={props.value}
        onChangeText={props.onChange}
        placeholder={props.placeholder}
        placeholderTextColor={c.muted}
        secureTextEntry={props.secure}
        keyboardType={props.keyboard}
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

// Run a job with a busy flag and a readable problem.
function useJob() {
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState("");
  const run = async (job: () => Promise<void>) => {
    setBusy(true);
    setProblem("");
    try {
      await job();
    } catch (error) {
      setProblem(sentence(error));
    } finally {
      setBusy(false);
    }
  };
  return { busy, problem, run };
}

function SignIn({ busy, run }: { busy: boolean; run: ReturnType<typeof useJob>["run"] }) {
  const queryClient = useQueryClient();
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const [sentTo, setSentTo] = useState<string | null>(null);
  const send = () =>
    run(async () => {
      await sendCode(email.trim());
      setSentTo(email.trim());
    });
  const verify = () =>
    run(async () => {
      await verifyCode(sentTo ?? "", code.trim());
      setCode("");
      setEmail("");
      setSentTo(null);
      await queryClient.invalidateQueries({ queryKey: ["cloudAccount"] });
    });
  return (
    <>
      <Note>Sign in by email only if you have premium. Free use needs no account.</Note>
      {sentTo ? (
        <>
          <Note>{`Enter the code that went to ${sentTo}.`}</Note>
          <Field value={code} onChange={setCode} placeholder="6-digit code" keyboard="number-pad" />
          <Section>
            <Item
              title="Verify code"
              tone="action"
              disabled={busy || !code.trim()}
              onPress={verify}
            />
            <Item title="Use another email" disabled={busy} onPress={() => setSentTo(null)} />
          </Section>
        </>
      ) : (
        <>
          <Field
            value={email}
            onChange={setEmail}
            placeholder="you@example.com"
            keyboard="email-address"
          />
          <Section>
            <Item
              title="Send code"
              tone="action"
              disabled={busy || !email.includes("@")}
              onPress={send}
            />
          </Section>
        </>
      )}
    </>
  );
}

function CloudSection({ account, error }: { account: CloudAccount | undefined; error: unknown }) {
  const queryClient = useQueryClient();
  const { busy, problem, run } = useJob();
  const signOut = () =>
    run(async () => {
      await signOutCloud();
      await queryClient.invalidateQueries({ queryKey: ["cloudAccount"] });
    });
  return (
    <>
      <Section title="Account">
        <Item title={account?.creditsLine ?? "Reading the balance."} />
        {account ? <Item title={account.planLine} /> : null}
        {account?.email ? (
          <Item title="Sign out" tone="danger" disabled={busy} onPress={signOut} />
        ) : null}
      </Section>
      {account?.email ? null : <SignIn busy={busy} run={run} />}
      {account?.premium ? null : <Note>{PREMIUM_LATER}</Note>}
      {problem || error ? <Note>{problem || sentence(error)}</Note> : null}
    </>
  );
}

function KeySection({ hasKey, onSaved }: { hasKey: boolean; onSaved: () => void }) {
  const [key, setKey] = useState("");
  const { busy, problem, run } = useJob();
  const save = () =>
    run(async () => {
      await saveKey(key.trim());
      setKey("");
      onSaved();
    });
  return (
    <>
      <Section>
        <Item title="OpenRouter key" value={hasKey ? "Saved" : "Not set"} />
      </Section>
      <Field value={key} onChange={setKey} placeholder="OpenRouter API key" secure />
      <Section>
        <Item title="Save key" tone="action" disabled={busy || !key.trim()} onPress={save} />
      </Section>
      {problem ? <Note>{problem}</Note> : null}
    </>
  );
}

export function CoachPage() {
  const config = useCoachConfig();
  const c = config.data;
  const provider = c?.provider === "openrouter" || c?.provider === "ollama" ? c.provider : "cloud";
  const cloud = useCloudAccount(provider === "cloud", true);
  const { problem, run } = useJob();
  const choose = (p: "cloud" | "openrouter") =>
    run(async () => {
      await chooseProvider(p);
      await config.refetch();
    });
  return (
    <Page title="Coach">
      <Section title="Where the coach runs">
        <Item
          title="Garmin Companion cloud"
          checked={provider === "cloud"}
          onPress={() => choose("cloud")}
        />
        <Item
          title="Your own OpenRouter key"
          checked={provider === "openrouter"}
          onPress={() => choose("openrouter")}
        />
      </Section>
      <Note>{WHERE[provider]}</Note>
      {provider === "cloud" ? (
        <CloudSection account={cloud.data ?? undefined} error={cloud.error} />
      ) : null}
      {provider === "openrouter" ? (
        <KeySection hasKey={c?.hasKey === true} onSaved={() => void config.refetch()} />
      ) : null}
      {needsSetup(c) && provider === "openrouter" && !c?.hasKey ? (
        <Note>The coach needs a key. Paste your own OpenRouter key.</Note>
      ) : null}
      {problem ? <Note>{problem}</Note> : null}
    </Page>
  );
}
