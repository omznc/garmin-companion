import { ActionSheetIOS, Alert, Platform } from "react-native";
import type { ThreadMeta } from "../data/chatTypes";
import { deleteThread } from "./useThreads";

function confirmDelete(thread: ThreadMeta) {
  Alert.alert("Delete the chat?", `"${thread.title}" and its messages go. This cannot be undone.`, [
    { text: "Cancel", style: "cancel" },
    {
      text: "Delete",
      style: "destructive",
      onPress: () =>
        void deleteThread(thread.sessionId).catch((error: Error) =>
          Alert.alert("The chat was not deleted", error.message),
        ),
    },
  ]);
}

// The long press on a row: rename or delete. iOS draws the action sheet, Android a dialog.
export function showThreadMenu(thread: ThreadMeta, onRename: () => void) {
  if (Platform.OS === "ios") {
    ActionSheetIOS.showActionSheetWithOptions(
      {
        title: thread.title,
        options: ["Rename", "Delete", "Cancel"],
        destructiveButtonIndex: 1,
        cancelButtonIndex: 2,
      },
      (index) => {
        if (index === 0) onRename();
        else if (index === 1) confirmDelete(thread);
      },
    );
    return;
  }
  Alert.alert(thread.title, undefined, [
    { text: "Rename", onPress: onRename },
    { text: "Delete", onPress: () => confirmDelete(thread) },
    { text: "Cancel", style: "cancel" },
  ]);
}
