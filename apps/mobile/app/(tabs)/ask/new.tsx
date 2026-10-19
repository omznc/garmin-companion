import { ChatScreen } from "../../../src/ask/ChatScreen";

// A new chat from the history, so Back returns to the history.
export default function NewChatScreen() {
  return <ChatScreen routeId={null} />;
}
