import { AppBar } from "./AppBar";

// The Ask stack is on top of the tabs, so every screen has a back arrow.
export function ChatHeader({
  title,
  onHistory,
}: {
  title: string;
  first: boolean;
  onHistory: () => void;
}) {
  return (
    <AppBar
      title={title}
      back
      action={{ icon: "history", label: "Chat history", onPress: onHistory }}
    />
  );
}
