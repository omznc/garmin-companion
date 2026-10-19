import { useState } from "react";
import { Pressable, Text, View } from "react-native";
import { type CloudAccount, PREMIUM_LATER } from "../data/useCloud";
import { useChatTheme } from "./theme";

// The credits under the messages. Premium is not on sale in the app yet, so it only explains.
export function CreditLine({ account, onOwnKey }: { account: CloudAccount; onOwnKey: () => void }) {
  const theme = useChatTheme();
  const [info, setInfo] = useState(false);
  const colour = account.out ? theme.error : account.low ? theme.muted : theme.faint;
  const explain = !account.premium && (info || account.out);
  return (
    <View style={{ paddingHorizontal: 20, paddingTop: 6, gap: 6 }}>
      <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
        <Text style={{ color: colour, fontSize: 13, flexShrink: 1 }}>{account.creditsLine}</Text>
        {account.premium ? null : (
          <Pressable accessibilityRole="button" hitSlop={10} onPress={() => setInfo((v) => !v)}>
            <Text style={{ color: theme.accent, fontSize: 13 }}>Premium</Text>
          </Pressable>
        )}
      </View>
      {explain ? (
        <Text style={{ color: theme.muted, fontSize: 13, lineHeight: 19 }}>{PREMIUM_LATER}</Text>
      ) : null}
      {account.out ? (
        <Pressable accessibilityRole="button" hitSlop={10} onPress={onOwnKey}>
          <Text style={{ color: theme.accent, fontSize: 13 }}>Use your own key</Text>
        </Pressable>
      ) : null}
    </View>
  );
}
