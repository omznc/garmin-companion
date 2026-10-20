import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import {
  currentDialog,
  dismissDialog,
  enqueueDialog,
  pressDialogButton,
  resetDialogs,
} from "./dialog-queue.ts";

beforeEach(resetDialogs);

test("gives one OK button when there is no button", () => {
  enqueueDialog("Saved");
  assert.deepEqual(
    currentDialog()?.buttons.map((b) => b.text),
    ["OK"],
  );
});

test("shows the dialogs in the order of the calls", () => {
  enqueueDialog("One");
  enqueueDialog("Two");
  assert.equal(currentDialog()?.title, "One");
  pressDialogButton(currentDialog()?.id ?? 0, 0);
  assert.equal(currentDialog()?.title, "Two");
});

test("a handler that opens a dialog queues it behind the rest", () => {
  enqueueDialog("One", undefined, [{ text: "Go", onPress: () => enqueueDialog("Three") }]);
  enqueueDialog("Two");
  pressDialogButton(currentDialog()?.id ?? 0, 0);
  assert.equal(currentDialog()?.title, "Two");
});

test("dismiss works only for a cancelable dialog", () => {
  enqueueDialog("Fixed");
  dismissDialog(currentDialog()?.id ?? 0);
  assert.equal(currentDialog()?.title, "Fixed");
  resetDialogs();
  let dismissed = false;
  enqueueDialog("Open", undefined, undefined, {
    cancelable: true,
    onDismiss: () => {
      dismissed = true;
    },
  });
  dismissDialog(currentDialog()?.id ?? 0);
  assert.equal(currentDialog(), undefined);
  assert.equal(dismissed, true);
});
