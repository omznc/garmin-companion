// The Ask screen body, shared by Android and iOS. The platform files give the colours and the top.
// The text input is the native one. Answers stream in through `useAsk`.
import { useQuery } from "@tanstack/react-query";
import { useRef, useState } from "react";
import {
  type ColorValue,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  ScrollView,
  Text,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView, useSafeAreaInsets } from "react-native-safe-area-context";
import { blocks, inline } from "../data/markdown";
import { callJson } from "../data/source";
import {
  type CoachConfig,
  type Step,
  type Turn,
  needsSetup,
  openers,
  saveKey,
  useAsk,
} from "../data/useAsk";

export type AskTheme = {
  bg: ColorValue;
  text: ColorValue;
  muted: ColorValue;
  faint: ColorValue;
  accent: ColorValue;
  onAccent: ColorValue;
  bubble: ColorValue;
  field: ColorValue;
  line: ColorValue;
  warn: ColorValue;
};

function Spans({ line, theme, size }: { line: string; theme: AskTheme; size: number }) {
  return (
    <Text style={{ color: theme.text, fontSize: size, lineHeight: size * 1.5 }}>
      {inline(line).map((s, i) => (
        <Text
          // biome-ignore lint/suspicious/noArrayIndexKey: spans have no id and never reorder
          key={i}
          style={[
            s.bold ? { fontWeight: "600" } : null,
            s.code ? { fontFamily: "monospace", backgroundColor: theme.bubble } : null,
          ]}
        >
          {s.text}
        </Text>
      ))}
    </Text>
  );
}

function Markdown({ text, theme }: { text: string; theme: AskTheme }) {
  return (
    <View style={{ gap: 12 }}>
      {blocks(text).map((b, i) =>
        b.kind === "item" ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: blocks have no id and never reorder
          <View key={i} style={{ flexDirection: "row", gap: 8 }}>
            <Text style={{ color: theme.faint, fontSize: 16, lineHeight: 24, minWidth: 18 }}>
              {b.marker}
            </Text>
            <View style={{ flex: 1 }}>
              <Spans line={b.text} theme={theme} size={16} />
            </View>
          </View>
        ) : b.kind === "code" ? (
          <Text
            // biome-ignore lint/suspicious/noArrayIndexKey: blocks have no id and never reorder
            key={i}
            style={{
              color: theme.text,
              backgroundColor: theme.bubble,
              fontFamily: "monospace",
              fontSize: 13.5,
              padding: 12,
              borderRadius: 8,
            }}
          >
            {b.text}
          </Text>
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: blocks have no id and never reorder
          <View key={i}>
            <Spans line={b.text} theme={theme} size={b.kind === "heading" ? 18 : 16} />
          </View>
        ),
      )}
    </View>
  );
}

function StepRow({ step, theme }: { step: Step; theme: AskTheme }) {
  const dot = step.running ? theme.accent : step.ok ? theme.faint : theme.warn;
  return (
    <View style={{ flexDirection: "row", alignItems: "center", gap: 10 }}>
      <View style={{ width: 6, height: 6, borderRadius: 3, backgroundColor: dot }} />
      <Text style={{ color: step.running ? theme.muted : theme.faint, fontSize: 14 }}>
        {step.label}
      </Text>
    </View>
  );
}

function TurnView({ turn, live, theme }: { turn: Turn; live: boolean; theme: AskTheme }) {
  if (turn.who === "you") {
    return (
      <View
        style={{
          alignSelf: "flex-end",
          maxWidth: "85%",
          backgroundColor: theme.bubble,
          borderRadius: 18,
          paddingHorizontal: 15,
          paddingVertical: 10,
        }}
      >
        <Text style={{ color: theme.text, fontSize: 16, lineHeight: 24 }}>{turn.text}</Text>
      </View>
    );
  }
  return (
    <View style={{ gap: 14 }}>
      {turn.steps.length > 0 ? (
        <View style={{ gap: 6 }}>
          {turn.steps.map((s) => (
            <StepRow key={s.callId} step={s} theme={theme} />
          ))}
        </View>
      ) : null}
      {turn.text ? <Markdown text={turn.text} theme={theme} /> : null}
      {!turn.text && live && turn.steps.length === 0 && !turn.error ? (
        <Text style={{ color: theme.faint, fontSize: 14 }}>Thinking</Text>
      ) : null}
      {turn.error ? <Text style={{ color: theme.warn, fontSize: 14 }}>{turn.error}</Text> : null}
    </View>
  );
}

function Setup({ theme, onSaved }: { theme: AskTheme; onSaved: () => void }) {
  const [key, setKey] = useState("");
  const [problem, setProblem] = useState("");
  const save = async () => {
    try {
      await saveKey(key.trim());
      setKey("");
      onSaved();
    } catch (error) {
      setProblem((error as Error).message);
    }
  };
  return (
    <View style={{ gap: 10, marginTop: 28 }}>
      <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 21 }}>
        The coach needs a model. Paste your own OpenRouter key. It stays on this phone, in the app's
        private store.
      </Text>
      <TextInput
        value={key}
        onChangeText={setKey}
        placeholder="OpenRouter API key"
        placeholderTextColor={theme.faint}
        secureTextEntry
        autoCapitalize="none"
        autoCorrect={false}
        style={{
          color: theme.text,
          backgroundColor: theme.field,
          borderRadius: 12,
          paddingHorizontal: 14,
          paddingVertical: 12,
          fontSize: 16,
        }}
      />
      <Pressable
        accessibilityRole="button"
        disabled={!key.trim()}
        onPress={save}
        style={{
          alignSelf: "flex-start",
          backgroundColor: key.trim() ? theme.accent : theme.field,
          borderRadius: 20,
          paddingHorizontal: 18,
          paddingVertical: 10,
        }}
      >
        <Text style={{ color: key.trim() ? theme.onAccent : theme.faint, fontSize: 15 }}>
          Save key
        </Text>
      </Pressable>
      {problem ? <Text style={{ color: theme.warn, fontSize: 14 }}>{problem}</Text> : null}
    </View>
  );
}

function Empty({
  theme,
  setup,
  onSaved,
  onPick,
}: {
  theme: AskTheme;
  setup: boolean;
  onSaved: () => void;
  onPick: (q: string) => void;
}) {
  return (
    <View style={{ paddingTop: 8 }}>
      <Text style={{ color: theme.muted, fontSize: 16, lineHeight: 24 }}>
        Ask about your sleep, recovery, activity or weight. The coach reads your own Garmin data
        before it answers.
      </Text>
      <View style={{ marginTop: 24, gap: 10 }}>
        {openers().map((q) => (
          <Pressable
            key={q}
            accessibilityRole="button"
            onPress={() => onPick(q)}
            style={{
              borderColor: theme.line,
              borderWidth: 1,
              borderRadius: 18,
              paddingHorizontal: 16,
              paddingVertical: 12,
            }}
          >
            <Text style={{ color: theme.text, fontSize: 15 }}>{q}</Text>
          </Pressable>
        ))}
      </View>
      {setup ? <Setup theme={theme} onSaved={onSaved} /> : null}
    </View>
  );
}

export function AskView({ theme, top }: { theme: AskTheme; top: number }) {
  const insets = useSafeAreaInsets();
  const { turns, busy, send, stop } = useAsk();
  const config = useQuery({
    queryKey: ["askConfig"],
    queryFn: () => callJson<CoachConfig>("ask_config"),
    retry: false,
  });
  const [draft, setDraft] = useState("");
  const scroll = useRef<{ scrollToEnd: (o?: { animated?: boolean }) => void } | null>(null);
  const ready = !busy && draft.trim().length > 0;

  const submit = () => {
    if (busy) return stop();
    if (!ready) return;
    const text = draft;
    setDraft("");
    void send(text);
  };

  return (
    <KeyboardAvoidingView
      style={{ flex: 1, backgroundColor: theme.bg }}
      behavior="padding"
      keyboardVerticalOffset={Platform.OS === "ios" ? insets.top + 44 : 0}
    >
      <ScrollView
        ref={(node) => {
          scroll.current = node;
        }}
        keyboardShouldPersistTaps="handled"
        keyboardDismissMode="interactive"
        contentInsetAdjustmentBehavior="automatic"
        onContentSizeChange={() => busy && scroll.current?.scrollToEnd({ animated: false })}
        contentContainerStyle={{ padding: 16, paddingTop: top + 8, paddingBottom: 24, gap: 28 }}
      >
        {turns.length === 0 ? (
          <Empty
            theme={theme}
            setup={needsSetup(config.data)}
            onSaved={() => void config.refetch()}
            onPick={setDraft}
          />
        ) : (
          turns.map((t, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: turns are only appended
            <TurnView key={i} turn={t} live={busy && i === turns.length - 1} theme={theme} />
          ))
        )}
      </ScrollView>
      {/* iOS: the native tab bar floats over the screen, and only the view's own safe area
          includes it. Android: the tab bar sits below the screen and owns the system inset. */}
      <SafeAreaView
        edges={Platform.OS === "ios" ? ["bottom"] : []}
        style={{
          flexDirection: "row",
          alignItems: "flex-end",
          gap: 8,
          paddingHorizontal: 12,
          paddingTop: 8,
          paddingBottom: 12,
        }}
      >
        <TextInput
          value={draft}
          onChangeText={setDraft}
          placeholder="Ask about your sleep, training or recovery"
          placeholderTextColor={theme.faint}
          multiline
          style={{
            flex: 1,
            maxHeight: 160,
            color: theme.text,
            backgroundColor: theme.field,
            borderRadius: 22,
            paddingHorizontal: 16,
            paddingTop: 11,
            paddingBottom: 11,
            fontSize: 16,
          }}
        />
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={busy ? "Stop" : "Send"}
          disabled={!busy && !ready}
          onPress={submit}
          style={{
            width: 44,
            height: 44,
            borderRadius: 22,
            alignItems: "center",
            justifyContent: "center",
            backgroundColor: busy || ready ? theme.accent : theme.field,
          }}
        >
          <Text style={{ color: busy || ready ? theme.onAccent : theme.faint, fontSize: 20 }}>
            {busy ? "■" : "↑"}
          </Text>
        </Pressable>
      </SafeAreaView>
    </KeyboardAvoidingView>
  );
}
