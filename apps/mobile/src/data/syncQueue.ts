export type SyncArgs = { days?: number; full?: boolean };

const keyOf = (args: SyncArgs) => `${args.days ?? ""}/${args.full ?? false}`;

// One run at a time. A call with the same args as the last call in the queue gets that run.
// A call with other args waits for the runs before it, so a full import is never met by a quick sync.
// `onBusy(false)` fires when the queue is empty.
export function createSyncQueue<T>(
  run: (args: SyncArgs) => Promise<T>,
  onBusy: (b: boolean) => void,
) {
  let last: { key: string; promise: Promise<T> } | null = null;
  return (args: SyncArgs = {}): Promise<T> => {
    const key = keyOf(args);
    if (last?.key === key) return last.promise;
    const before = last ? last.promise.catch(() => {}) : Promise.resolve();
    onBusy(true);
    const entry = { key, promise: before.then(() => run(args)) };
    const settle = () => {
      if (last !== entry) return;
      last = null;
      onBusy(false);
    };
    entry.promise.then(settle, settle);
    last = entry;
    return entry.promise;
  };
}
