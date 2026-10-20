import { useSyncExternalStore } from "react";
import { Modal, Pressable, Text, useWindowDimensions, View } from "react-native";
import { useReduceMotion } from "../theme/motion";
import { useMd } from "../theme/md";
import { currentDialog, dismissDialog, pressDialogButton, subscribeDialogs } from "./dialog-queue";

// The Material 3 dialog that `showDialog` queues, one at a time. A button that
// opens a second dialog makes it wait in the queue until this one closes.
export function DialogHost() {
  const dialog = useSyncExternalStore(subscribeDialogs, currentDialog);
  const md = useMd();
  const reduceMotion = useReduceMotion();
  const { width } = useWindowDimensions();
  if (!dialog) return null;
  const close = () => dismissDialog(dialog.id);
  return (
    <Modal
      // A new key gives each dialog its own window.
      key={dialog.id}
      visible
      transparent
      statusBarTranslucent
      animationType={reduceMotion ? "none" : "fade"}
      onRequestClose={close}
    >
      <View style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
        {/* The scrim. A tap on it closes a cancelable dialog. */}
        <Pressable
          accessibilityLabel="Close"
          onPress={close}
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
          accessibilityViewIsModal
          style={{
            // 24 dp of margin each side, from 280 dp to 560 dp (Material 3).
            width: Math.max(280, Math.min(width - 48, 560)),
            padding: 24,
            gap: 16,
            borderRadius: 28,
            backgroundColor: md.surfaceContainerHigh,
          }}
        >
          <Text
            accessibilityRole="header"
            style={{ color: md.onSurface, fontSize: 24, lineHeight: 32 }}
          >
            {dialog.title}
          </Text>
          {dialog.message ? (
            <Text style={{ color: md.onSurfaceVariant, fontSize: 14, lineHeight: 20 }}>
              {dialog.message}
            </Text>
          ) : null}
          <View
            style={{ flexDirection: "row", flexWrap: "wrap", justifyContent: "flex-end", gap: 8 }}
          >
            {dialog.buttons.map((button, index) => {
              const color = button.style === "destructive" ? md.error : md.primary;
              return (
                <Pressable
                  // The buttons are fixed while the dialog shows, and two may share a text.
                  key={`${index}:${button.text}`}
                  accessibilityRole="button"
                  onPress={() => pressDialogButton(dialog.id, index)}
                  android_ripple={{ color: `${color}33` }}
                  style={{
                    minHeight: 40,
                    justifyContent: "center",
                    paddingHorizontal: 12,
                    borderRadius: 20,
                  }}
                >
                  <Text style={{ color, fontSize: 14, fontWeight: "500" }}>{button.text}</Text>
                </Pressable>
              );
            })}
          </View>
        </View>
      </View>
    </Modal>
  );
}
