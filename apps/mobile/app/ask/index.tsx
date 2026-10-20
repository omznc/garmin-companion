import { ChatScreen } from "../../src/ask/ChatScreen";

// The route /ask: a new chat. The first message makes the thread.
export default function Screen() {
  return <ChatScreen routeId={null} root />;
}
