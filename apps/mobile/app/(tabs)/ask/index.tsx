import { ChatScreen } from "../../../src/ask/ChatScreen";

// The root of the Ask tab: a new chat. The first message makes the thread.
export default function Screen() {
  return <ChatScreen routeId={null} root />;
}
