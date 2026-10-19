import { type ComponentRef, useRef, useState } from "react";
import { Platform, Pressable, Text, TextInput, View } from "react-native";
import { Glyph } from "./Glyph";
import { useChatTheme } from "./theme";

// The server refuses a longer message.
const MAX_LENGTH = 10_000;

// The field at the foot of the chat. It clears on send and takes the text
// back when the message did not arrive. While a turn runs, the button stops it.
export function Composer({
  onSend,
  onStop,
  streaming,
  locked = false,
  lockedReason,
  bottomInset = 12,
  placeholder,
  autoFocus = false,
}: {
  /** Resolves true when the message arrived. On false the text comes back. */
  onSend: (text: string) => Promise<boolean>;
  onStop: () => void;
  streaming: boolean;
  /** The field is off, for example while an approval waits. */
  locked?: boolean;
  /** A short line above the field that tells why it is off. */
  lockedReason?: string;
  /** The space under the field: the safe area, or less under the keyboard. */
  bottomInset?: number;
  placeholder: string;
  autoFocus?: boolean;
}) {
  const theme = useChatTheme();
  const [text, setText] = useState("");
  const input = useRef<ComponentRef<typeof TextInput>>(null);
  const canSend = text.trim() !== "" && !streaming && !locked;

  const send = async () => {
    const message = text.trim();
    if (!message || streaming || locked) return;
    setText("");
    const landed = await onSend(message);
    if (!landed) {
      setText((current) => (current === "" ? message : current));
      input.current?.focus();
    }
  };

  return (
    <View
      style={{
        backgroundColor: theme.bg,
        borderTopWidth: Platform.OS === "ios" ? 0.5 : 0,
        borderTopColor: theme.line,
        paddingTop: 8,
        paddingHorizontal: 12,
        paddingBottom: bottomInset,
        gap: 6,
      }}
    >
      {locked && lockedReason ? (
        <Text style={{ color: theme.muted, fontSize: 13, lineHeight: 18, paddingHorizontal: 4 }}>
          {lockedReason}
        </Text>
      ) : null}
      <View style={{ flexDirection: "row", alignItems: "flex-end", gap: 8 }}>
        <TextInput
          ref={input}
          value={text}
          onChangeText={setText}
          placeholder={placeholder}
          placeholderTextColor={theme.muted}
          selectionColor={Platform.OS === "android" ? theme.accent : undefined}
          cursorColor={theme.accent}
          editable={!locked}
          autoFocus={autoFocus}
          multiline
          maxLength={MAX_LENGTH}
          accessibilityLabel="Message"
          style={{
            flex: 1,
            minHeight: 44,
            maxHeight: 160,
            paddingHorizontal: 16,
            paddingVertical: Platform.OS === "ios" ? 11 : 8,
            borderRadius: 22,
            fontSize: 16,
            lineHeight: 24,
            color: theme.text,
            backgroundColor: theme.field,
            textAlignVertical: "center",
          }}
        />
        {streaming ? (
          <StopButton onPress={onStop} />
        ) : (
          <SendButton canSend={canSend} onPress={() => void send()} />
        )}
      </View>
    </View>
  );
}

const BUTTON = {
  width: 44,
  height: 44,
  borderRadius: 22,
  alignItems: "center",
  justifyContent: "center",
} as const;

function StopButton({ onPress }: { onPress: () => void }) {
  const theme = useChatTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel="Stop the answer"
      onPress={onPress}
      hitSlop={6}
      style={({ pressed }) => ({
        ...BUTTON,
        backgroundColor: theme.stop,
        opacity: pressed ? 0.7 : 1,
      })}
    >
      <Glyph name="stop" size={20} color={theme.onStop} />
    </Pressable>
  );
}

function SendButton({ canSend, onPress }: { canSend: boolean; onPress: () => void }) {
  const theme = useChatTheme();
  // iOS dims the tinted button when it is off, as the system does.
  // Android draws a grey one.
  const tinted = canSend || Platform.OS === "ios";
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel="Send"
      accessibilityState={{ disabled: !canSend }}
      disabled={!canSend}
      onPress={onPress}
      hitSlop={6}
      style={({ pressed }) => ({
        ...BUTTON,
        backgroundColor: tinted ? theme.accent : theme.field,
        opacity: !canSend && Platform.OS === "ios" ? 0.3 : pressed ? 0.7 : 1,
      })}
    >
      <Glyph name="send" size={22} color={tinted ? theme.onAccent : theme.muted} />
    </Pressable>
  );
}
