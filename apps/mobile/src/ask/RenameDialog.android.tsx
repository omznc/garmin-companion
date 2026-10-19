import { useState } from "react";
import { KeyboardAvoidingView, Modal, Pressable, Text, TextInput, View } from "react-native";
import { useMd } from "../theme/md";

// The longest title the core keeps.
const MAX_TITLE = 200;

// A Material 3 dialog with one field, to rename a chat.
export function RenameDialog({
  initial,
  onCancel,
  onSave,
}: {
  /** The current title. Null closes the dialog. */
  initial: string | null;
  onCancel: () => void;
  /** Resolves with an error sentence, or null when the name was saved. */
  onSave: (title: string) => Promise<string | null>;
}) {
  const md = useMd();
  const [title, setTitle] = useState(initial ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = async () => {
    const next = title.trim();
    if (!next) {
      setError("Give it a name.");
      return;
    }
    setSaving(true);
    setError(await onSave(next));
    setSaving(false);
  };

  return (
    <Modal
      visible={initial !== null}
      transparent
      animationType="fade"
      onRequestClose={onCancel}
      onShow={() => {
        setTitle(initial ?? "");
        setError(null);
      }}
    >
      <KeyboardAvoidingView
        behavior="padding"
        style={{ flex: 1, justifyContent: "center", padding: 24 }}
      >
        {/* The scrim: the screen behind, dimmed. A tap on it cancels. */}
        <Pressable
          accessibilityLabel="Close"
          onPress={onCancel}
          style={{
            position: "absolute",
            top: 0,
            right: 0,
            bottom: 0,
            left: 0,
            backgroundColor: md.scrim,
            opacity: 0.32,
          }}
        />
        <View
          style={{
            gap: 16,
            padding: 24,
            borderRadius: 28,
            backgroundColor: md.surfaceContainerHigh,
          }}
        >
          <Text style={{ color: md.onSurface, fontSize: 24, lineHeight: 32 }}>Rename the chat</Text>
          <TextInput
            value={title}
            onChangeText={setTitle}
            maxLength={MAX_TITLE}
            autoFocus
            returnKeyType="done"
            onSubmitEditing={() => void save()}
            editable={!saving}
            accessibilityLabel="Name"
            cursorColor={md.primary}
            selectionColor={md.primary}
            style={{
              color: md.onSurface,
              fontSize: 16,
              paddingHorizontal: 16,
              paddingVertical: 12,
              borderRadius: 12,
              borderWidth: 1,
              borderColor: md.outline,
            }}
          />
          {error ? <Text style={{ color: md.error, fontSize: 14 }}>{error}</Text> : null}
          <View style={{ flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
            <Pressable
              accessibilityRole="button"
              onPress={onCancel}
              android_ripple={{ color: `${md.primary}33` }}
              style={{ paddingHorizontal: 12, paddingVertical: 10, borderRadius: 20 }}
            >
              <Text style={{ color: md.primary, fontSize: 14, fontWeight: "500" }}>Cancel</Text>
            </Pressable>
            <Pressable
              accessibilityRole="button"
              disabled={saving || !title.trim()}
              onPress={() => void save()}
              android_ripple={{ color: `${md.primary}33` }}
              style={{ paddingHorizontal: 12, paddingVertical: 10, borderRadius: 20 }}
            >
              <Text
                style={{
                  color: saving || !title.trim() ? md.outline : md.primary,
                  fontSize: 14,
                  fontWeight: "500",
                }}
              >
                Save
              </Text>
            </Pressable>
          </View>
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}
