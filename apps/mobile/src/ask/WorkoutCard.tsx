import { useRouter } from "expo-router";
import { useState } from "react";
import { Platform, Pressable, Text, type TextStyle, View } from "react-native";
import type { DraftEntry, DraftExec } from "../data/chatTypes";
import { queryClient } from "../data/queryClient";
import { callJson } from "../data/source";
import { committed } from "../ui/haptics";
import { showSnackbar } from "../ui/snackbar-store";
import { Glyph } from "./Glyph";
import { Spinner } from "./Spinner";
import { SportGlyph } from "./SportGlyph";
import { threadKey } from "./runs";
import { type ChatTheme, useChatTheme } from "./theme";
import { endLabel, KIND_LABEL, subline, targetLabel } from "./workout";

const ANDROID = Platform.OS === "android";
// Android: a large tonal container and a full-radius button. iOS: an inset grouped radius.
const CARD_RADIUS = ANDROID ? 28 : 12;
const BUTTON_RADIUS = ANDROID ? 999 : 12;
const SEMIBOLD: TextStyle["fontWeight"] = ANDROID ? "500" : "600";
const FOLDED_ROWS = 8;

type Row = { key: string; indent: boolean; text: string; note?: string; head?: boolean };

// One line for a step: "Interval  3 min  zone 4".
function execRow(key: string, step: DraftExec, indent: boolean): Row {
  const detail = [endLabel(step.end), targetLabel(step.target)].filter(Boolean).join(" · ");
  return { key, indent, text: `${KIND_LABEL[step.kind]}  ${detail}`, note: step.note };
}

function rowsOf(entry: DraftEntry): Row[] {
  return entry.draft.steps.flatMap((step, i): Row[] => {
    if (step.type === "exec") return [execRow(`${i}`, step, false)];
    const head: Row = { key: `${i}`, indent: false, text: `${step.times} ×`, head: true };
    return [head, ...step.steps.map((s, j) => execRow(`${i}.${j}`, s, true))];
  });
}

type Save = {
  workoutId: number | null;
  saving: boolean;
  error: string | null;
  signIn: boolean;
  save: () => void;
};

// The Save press. The core keeps the saved id, so a second press cannot make a second workout.
function useSave(turnId: string, entry: DraftEntry): Save {
  const [workoutId, setWorkoutId] = useState(entry.savedWorkoutId);
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<{ message: string; kind?: string } | null>(null);
  const saved = workoutId ?? entry.savedWorkoutId;

  const save = () => {
    if (saving || saved !== null) return;
    setSaving(true);
    setFailure(null);
    // The id of a turn is "<session>:<place>".
    const sessionId = turnId.slice(0, turnId.lastIndexOf(":"));
    callJson<{ workoutId: number }>("workout_create", {
      sessionId,
      turnId,
      callId: entry.callId,
    })
      .then((result) => {
        setWorkoutId(result.workoutId);
        committed();
        showSnackbar("Workout saved to Garmin");
        void queryClient.invalidateQueries({ queryKey: threadKey(sessionId) });
        // A saved workout can show in the plan screen.
        void queryClient.invalidateQueries({ queryKey: ["body"] });
      })
      .catch((error: Error & { kind?: string }) =>
        setFailure({ message: error.message, kind: error.kind }),
      )
      .finally(() => setSaving(false));
  };
  const signIn = failure?.kind === "not_connected";
  const error = signIn
    ? "Sign in to Garmin first."
    : failure
      ? "Could not save the workout."
      : null;
  return { workoutId: saved, saving, error, signIn, save };
}

// A workout that the coach proposed, with the button that saves it to Garmin.
export function WorkoutCard({ turnId, entry }: { turnId: string; entry: DraftEntry }) {
  const theme = useChatTheme();
  const { draft } = entry;
  const state = useSave(turnId, entry);
  return (
    <View
      style={{
        backgroundColor: theme.field,
        borderRadius: CARD_RADIUS,
        padding: ANDROID ? 20 : 16,
        gap: 12,
      }}
    >
      <Header entry={entry} theme={theme} />
      {draft.description ? (
        <Text numberOfLines={2} style={{ color: theme.muted, fontSize: 14, lineHeight: 20 }}>
          {draft.description}
        </Text>
      ) : null}
      <Steps entry={entry} theme={theme} />
      <Action state={state} theme={theme} name={draft.name} />
    </View>
  );
}

function Header({ entry, theme }: { entry: DraftEntry; theme: ChatTheme }) {
  const { draft } = entry;
  return (
    <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
      <View
        style={{
          width: 40,
          height: 40,
          borderRadius: ANDROID ? 20 : 10,
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: ANDROID ? theme.bubble : theme.header,
        }}
      >
        <SportGlyph sport={draft.sport} size={22} color={ANDROID ? theme.onBubble : theme.accent} />
      </View>
      <View style={{ flex: 1 }} accessible accessibilityRole="header">
        <Text
          style={{
            color: theme.text,
            fontSize: ANDROID ? 20 : 17,
            lineHeight: ANDROID ? 26 : 22,
            fontWeight: SEMIBOLD,
          }}
        >
          {draft.name}
        </Text>
        <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 20 }}>{subline(draft)}</Text>
      </View>
    </View>
  );
}

function Steps({ entry, theme }: { entry: DraftEntry; theme: ChatTheme }) {
  const [all, setAll] = useState(false);
  const rows = rowsOf(entry);
  const folded = rows.length > FOLDED_ROWS;
  const shown = folded && !all ? rows.slice(0, FOLDED_ROWS) : rows;
  return (
    <View style={{ gap: 6 }}>
      {shown.map((row) => (
        <View key={row.key} style={{ marginLeft: row.indent ? 16 : 0 }}>
          <Text
            style={{
              color: theme.text,
              fontSize: 14,
              lineHeight: 20,
              fontWeight: row.head ? SEMIBOLD : "400",
            }}
          >
            {row.text}
          </Text>
          {row.note ? (
            <Text style={{ color: theme.muted, fontSize: 13, lineHeight: 18 }}>{row.note}</Text>
          ) : null}
        </View>
      ))}
      {folded ? (
        <Pressable
          accessibilityRole="button"
          accessibilityState={{ expanded: all }}
          onPress={() => setAll((current) => !current)}
          style={{ minHeight: 48, justifyContent: "center" }}
        >
          <Text style={{ color: theme.accent, fontSize: 14, lineHeight: 20, fontWeight: SEMIBOLD }}>
            {all ? "Show fewer steps" : "Show all steps"}
          </Text>
        </Pressable>
      ) : null}
    </View>
  );
}

function Action({ state, theme, name }: { state: Save; theme: ChatTheme; name: string }) {
  const router = useRouter();
  if (state.workoutId !== null) {
    return (
      <View
        accessible
        accessibilityLabel={`${name} saved to Garmin`}
        style={{ flexDirection: "row", alignItems: "center", gap: 8, minHeight: 48 }}
      >
        <Glyph name="check" size={20} color={theme.accent} />
        <Text style={{ color: theme.text, fontSize: 15, fontWeight: SEMIBOLD }}>
          Saved to Garmin
        </Text>
      </View>
    );
  }
  return (
    <View style={{ gap: 8 }}>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={`Save ${name} to Garmin`}
        accessibilityState={{ busy: state.saving, disabled: state.saving }}
        disabled={state.saving}
        onPress={state.save}
        style={({ pressed }) => ({
          minHeight: 48,
          borderRadius: BUTTON_RADIUS,
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: theme.accent,
          opacity: state.saving || pressed ? 0.7 : 1,
        })}
      >
        {state.saving ? (
          <Spinner size={24} color={theme.onAccent} label="Saving" />
        ) : (
          <Text style={{ color: theme.onAccent, fontSize: 16, fontWeight: SEMIBOLD }}>
            Save to Garmin
          </Text>
        )}
      </Pressable>
      {state.error ? (
        <Text
          accessibilityRole="alert"
          style={{ color: theme.error, fontSize: 14, lineHeight: 20 }}
        >
          {state.error}
          {state.signIn ? (
            <Text
              accessibilityRole="link"
              onPress={() => router.push("/sign-in")}
              style={{ color: theme.accent, fontWeight: SEMIBOLD }}
            >
              {" Sign in"}
            </Text>
          ) : null}
        </Text>
      ) : null}
    </View>
  );
}
