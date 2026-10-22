// A check of the Rust bridge: one sync call, one async call and its events.
import { Button, Column, Host } from "@expo/ui";
import { useRef, useState } from "react";
import { Text, View } from "react-native";
import { bridgeVersion, call, sourceKind, subscribe } from "../data/source";

export function BridgeCheck() {
  const [lines, setLines] = useState<string[]>([]);
  const off = useRef<(() => void) | null>(null);
  const push = (line: string) => setLines((all) => [...all, line]);

  const run = async () => {
    setLines([`source: ${sourceKind}`, `sync call, version(): ${bridgeVersion()}`]);
    off.current?.();
    off.current = subscribe((e) => push(`event ${e.kind} ${JSON.stringify(e.payload)}`));
    try {
      const out = await call("sync", { steps: 3 });
      push(`async call, sync: ${JSON.stringify(out)}`);
    } catch (error) {
      push(`error: ${String(error)}`);
    } finally {
      off.current?.();
      off.current = null;
    }
  };

  return (
    <View style={{ padding: 16, gap: 12 }}>
      <Host matchContents>
        <Column>
          <Button label="Check the Rust bridge" onPress={run} />
        </Column>
      </Host>
      {lines.map((line) => (
        <Text key={line} selectable style={{ fontFamily: "monospace", fontSize: 13 }}>
          {line}
        </Text>
      ))}
    </View>
  );
}
