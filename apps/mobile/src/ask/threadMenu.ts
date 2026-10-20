import { ActionSheetIOS, Platform } from "react-native";
import type { ThreadMeta } from "../data/chatTypes";
import { showDialog } from "../ui/dialog";
import { removed } from "../ui/haptics";
import { notifyError } from "../ui/notify-error";
import { showSnackbar } from "../ui/snackbar-store";
import { deleteThread } from "./useThreads";

function confirmDelete(thread: ThreadMeta) {
  showDialog("Delete the chat?", `"${thread.title}" and its messages go. This cannot be undone.`, [
    { text: "Cancel", style: "cancel" },
    {
      text: "Delete",
      style: "destructive",
      onPress: () =>
        void deleteThread(thread.sessionId).then(
          () => {
            removed();
            showSnackbar("Chat deleted");
          },
          (error: Error) => notifyError("The chat was not deleted", error),
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
  showDialog(thread.title, undefined, [
    { text: "Rename", onPress: onRename },
    { text: "Delete", onPress: () => confirmDelete(thread) },
    { text: "Cancel", style: "cancel" },
  ]);
}
