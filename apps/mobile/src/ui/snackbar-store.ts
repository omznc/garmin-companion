// The one snackbar of the app. A new message replaces the one on screen.
export type Notice = { id: number; message: string };

let notice: Notice | null = null;
let nextId = 1;
const listeners = new Set<() => void>();

const publish = (next: Notice | null) => {
  notice = next;
  for (const listener of listeners) listener();
};

/** Show a short message about a change that just happened. */
export const showSnackbar = (message: string) => publish({ id: nextId++, message });

export const hideSnackbar = () => publish(null);

export const currentNotice = () => notice;

export const subscribeNotice = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
