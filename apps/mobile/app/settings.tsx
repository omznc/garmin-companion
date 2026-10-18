import { BridgeCheck } from "../src/components/BridgeCheck";
import { Placeholder } from "../src/screens/Placeholder";

export default function Settings() {
  return (
    <Placeholder title="Settings" settings={false}>
      <BridgeCheck />
    </Placeholder>
  );
}
