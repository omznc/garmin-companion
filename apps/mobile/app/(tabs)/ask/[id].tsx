import { useLocalSearchParams } from "expo-router";
import { ChatScreen } from "../../../src/ask/ChatScreen";

export default function ThreadScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  return <ChatScreen routeId={id ?? ""} />;
}
