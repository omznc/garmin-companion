import { Alert, type AlertButton, type AlertOptions, Platform } from "react-native";
import { enqueueDialog } from "./dialog-queue";

/**
 * The same call as `Alert.alert`. iOS uses `Alert.alert`. Android joins the
 * Material 3 queue, and `DialogHost` draws it.
 */
export const showDialog = (
  title: string,
  message?: string,
  buttons?: AlertButton[],
  options?: AlertOptions,
) => {
  if (Platform.OS === "android") {
    enqueueDialog(title, message, buttons, options);
    return;
  }
  Alert.alert(title, message, buttons, options);
};
