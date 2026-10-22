import { useState } from "react";
import { Pressable, Text, View } from "react-native";
import type { Step } from "../data/chatTypes";
import { Glyph } from "./Glyph";
import { Spinner } from "./Spinner";
import { useChatTheme } from "./theme";

// A note is a step with `kind: "note"`: what the model said between tool rounds. A kind from a
// newer build reads as a tool.
export type TraceStep = Step;

// The tools of the turn in flight: open while the turn works, and one line
// ("Used 3 tools") after it. The person can toggle it.
export function ToolTrace({ steps, working }: { steps: TraceStep[]; working: boolean }) {
  const theme = useChatTheme();
  const [manual, setManual] = useState<boolean | null>(null);
  const expanded = manual ?? working;
  if (steps.length === 0) return null;
  const done = steps.filter((step) => step.kind !== "note" && !step.running).length;
  const title = working ? "Working" : `Used ${done} ${done === 1 ? "tool" : "tools"}`;

  return (
    <View style={{ gap: 4 }}>
      <Pressable
        accessibilityRole="button"
        accessibilityState={{ expanded }}
        onPress={() => setManual((current) => !(current ?? working))}
        hitSlop={8}
        style={{ flexDirection: "row", alignItems: "center", gap: 8 }}
      >
        <Glyph name="tool" size={16} color={theme.muted} />
        <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 20, fontWeight: "500" }}>
          {title}
        </Text>
        <Glyph name={expanded ? "collapse" : "expand"} size={16} color={theme.muted} />
      </Pressable>
      {expanded ? (
        <View
          style={{
            marginLeft: 7,
            paddingLeft: 12,
            borderLeftWidth: 1,
            borderLeftColor: theme.line,
            gap: 4,
          }}
        >
          {steps.map((step) =>
            step.kind === "note" ? (
              <NoteRow key={step.callId} text={step.label} />
            ) : (
              <StepRow key={step.callId} step={step} />
            ),
          )}
        </View>
      ) : null}
    </View>
  );
}

// What the model said before it called the next tools: quiet text on the same rule.
function NoteRow({ text }: { text: string }) {
  const theme = useChatTheme();
  return <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 20 }}>{text}</Text>;
}

const stateWord = (step: TraceStep) => (step.running ? "running" : step.ok ? "done" : "failed");

function StepRow({ step }: { step: TraceStep }) {
  const theme = useChatTheme();
  return (
    <View
      accessible
      accessibilityLabel={`${step.label}, ${stateWord(step)}`}
      style={{ flexDirection: "row", alignItems: "center", gap: 8, minHeight: 24 }}
    >
      {step.running ? (
        <Spinner size={24} color={theme.muted} label="Running" />
      ) : (
        <Glyph
          name={step.ok ? "check" : "close"}
          size={16}
          color={step.ok ? theme.muted : theme.error}
        />
      )}
      <Text numberOfLines={1} style={{ flex: 1, color: theme.text, fontSize: 14, lineHeight: 20 }}>
        {step.label}
      </Text>
    </View>
  );
}
