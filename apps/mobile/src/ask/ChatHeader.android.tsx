import { AppBar } from "./AppBar";

export function ChatHeader({
  title,
  back,
  onHistory,
}: {
  title: string;
  back: boolean;
  onHistory: () => void;
}) {
  return (
    <AppBar
      title={title}
      back={back}
      action={{ icon: "history", label: "Chat history", onPress: onHistory }}
    />
  );
}
