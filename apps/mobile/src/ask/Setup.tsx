import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native";
import { isSampleData } from "../data/source";
import { SAMPLE_KEY_NOTE, saveKey } from "../data/useAsk";
import { useChatTheme } from "./theme";

// The key form under the opening of a draft chat, while the coach has no model.
export function Setup({ onSaved }: { onSaved: () => void }) {
  const theme = useChatTheme();
  const [key, setKey] = useState("");
  const [problem, setProblem] = useState("");
  const save = async () => {
    try {
      await saveKey(key.trim());
      setKey("");
      onSaved();
    } catch (error) {
      setProblem((error as Error).message);
    }
  };
  if (isSampleData()) {
    return (
      <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 21, marginTop: 28 }}>
        {SAMPLE_KEY_NOTE}
      </Text>
    );
  }
  return (
    <View style={{ gap: 10, marginTop: 28 }}>
      <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 21 }}>
        The coach needs a model. Paste your own OpenRouter key. It stays on this phone, in the app's
        private store.
      </Text>
      <TextInput
        value={key}
        onChangeText={setKey}
        placeholder="OpenRouter API key"
        placeholderTextColor={theme.faint}
        secureTextEntry
        autoCapitalize="none"
        autoCorrect={false}
        style={{
          color: theme.text,
          backgroundColor: theme.field,
          borderRadius: 12,
          paddingHorizontal: 14,
          paddingVertical: 12,
          fontSize: 16,
        }}
      />
      <Pressable
        accessibilityRole="button"
        disabled={!key.trim()}
        onPress={save}
        style={{
          alignSelf: "flex-start",
          backgroundColor: key.trim() ? theme.accent : theme.field,
          borderRadius: 20,
          paddingHorizontal: 18,
          paddingVertical: 10,
        }}
      >
        <Text style={{ color: key.trim() ? theme.onAccent : theme.faint, fontSize: 15 }}>
          Save key
        </Text>
      </Pressable>
      {problem ? <Text style={{ color: theme.error, fontSize: 14 }}>{problem}</Text> : null}
    </View>
  );
}
