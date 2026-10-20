import type { AlertButton, AlertOptions } from "react-native";

// The queue of the dialogs that Android draws. It has no React and no native
// code, so a test can run it. `DialogHost.android.tsx` draws the first dialog.

export type QueuedDialog = {
  id: number;
  title: string;
  message: string | undefined;
  /** Never empty: a call with no button gets one "OK". */
  buttons: readonly AlertButton[];
  /** A tap outside or Back closes the dialog only when this is true. */
  cancelable: boolean;
  onDismiss: (() => void) | undefined;
};

const DEFAULT_BUTTONS: readonly AlertButton[] = [{ text: "OK" }];

let queue: readonly QueuedDialog[] = [];
let nextId = 1;
const listeners = new Set<() => void>();

const publish = (next: readonly QueuedDialog[]) => {
  queue = next;
  for (const listener of listeners) listener();
};

/** Add a dialog to the end of the queue. It shows after the others close. */
export const enqueueDialog = (
  title: string,
  message?: string,
  buttons?: readonly AlertButton[],
  options?: AlertOptions,
): number => {
  const id = nextId++;
  const list = buttons && buttons.length > 0 ? buttons : DEFAULT_BUTTONS;
  publish([
    ...queue,
    {
      id,
      title,
      message: message || undefined,
      buttons: list,
      // A dialog with a Cancel button closes on Back. Otherwise the options decide.
      cancelable: options?.cancelable ?? list.some((b) => b.style === "cancel"),
      onDismiss: options?.onDismiss,
    },
  ]);
  return id;
};

const remove = (id: number): QueuedDialog | undefined => {
  const dialog = queue.find((entry) => entry.id === id);
  if (dialog) publish(queue.filter((entry) => entry.id !== id));
  return dialog;
};

/**
 * Close a dialog, then run the handler of one button. The dialog leaves the
 * queue first, so a handler that opens a second dialog queues it behind the rest.
 */
export const pressDialogButton = (id: number, index: number) => {
  remove(id)?.buttons[index]?.onPress?.();
};

/** Close a dialog by a tap outside or Back. Do nothing when it is not cancelable. */
export const dismissDialog = (id: number) => {
  const dialog = queue.find((entry) => entry.id === id);
  if (!dialog?.cancelable) return;
  remove(id);
  dialog.onDismiss?.();
};

export const currentDialog = (): QueuedDialog | undefined => queue[0];

export const subscribeDialogs = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};

/** For a test: empty the queue and the listeners. */
export const resetDialogs = () => {
  queue = [];
  listeners.clear();
};
