// The Ask stack on Android: the screens draw their own app bar, so the header is off.
import { TabStack } from "../nav/TabStack";

export function AskStack() {
  return <TabStack title="Ask" />;
}
