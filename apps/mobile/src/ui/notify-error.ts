import { showDialog } from "./dialog";

/** Show the failure of an action in the dialog of the app, with the core's own sentence. */
export const notifyError = (title: string, error: unknown) =>
  showDialog(
    title,
    (error instanceof Error && error.message) || "Something went wrong. Try again.",
  );
