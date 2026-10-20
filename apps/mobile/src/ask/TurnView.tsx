import { memo } from "react";
import { Text, View } from "react-native";
import type { Turn } from "../data/chatTypes";
import { Answer } from "./Answer";
import { Thinking } from "./Thinking";
import { ToolTrace } from "./ToolTrace";
import { UserMessage } from "./UserMessage";
import { WorkoutCard } from "./WorkoutCard";
import { useChatTheme } from "./theme";

// One turn: the question, the tools, the answer, and the end mark. Only the turn
// that streams changes, so the others do not draw again.
export const TurnView = memo(function TurnView({ turn }: { turn: Turn }) {
  const theme = useChatTheme();
  const running = turn.status === "running";
  return (
    <View style={{ gap: 12 }}>
      <UserMessage text={turn.question} />
      <ToolTrace steps={turn.steps} working={running} />
      {turn.drafts?.map((entry) => (
        <WorkoutCard key={entry.callId} turnId={turn.id} entry={entry} />
      ))}
      <Answer text={turn.answer} />
      {running && !turn.answer && turn.steps.length === 0 ? <Thinking /> : null}
      {turn.status === "error" ? (
        <Text style={{ color: theme.error, fontSize: 14, lineHeight: 20 }}>
          {turn.error ?? "The coach could not answer. Try again."}
        </Text>
      ) : null}
      {turn.status === "stopped" ? (
        <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 20 }}>Stopped</Text>
      ) : null}
    </View>
  );
});
