// The OS background task. The system starts it about every few hours: a quick sync of the recent days.
// It runs in a headless JavaScript context, so this file is imported from the entry file (index.ts).
// Importing data/source starts the Rust core with the same data folder as the app.
import * as BackgroundTask from "expo-background-task";
import * as TaskManager from "expo-task-manager";
import { File, Paths } from "expo-file-system";
import { getStatus, historyPending, lastSyncedAt } from "../data/account";
import { isSampleData } from "../data/source";
import { quickSync } from "../data/useSync";
import { type BackgroundDeps, runBackground } from "./backgroundPolicy";

export const BACKGROUND_SYNC_TASK = "garmin-background-sync";
// Minutes. Android WorkManager allows 15 at the least. iOS treats it as the earliest start only.
export const INTERVAL_MINUTES = 180;

const deps: BackgroundDeps = {
  signedIn: async () => (await getStatus()).connected,
  sample: isSampleData,
  historyPending,
  lastAt: lastSyncedAt,
  quickSync,
};

TaskManager.defineTask(BACKGROUND_SYNC_TASK, async () => {
  try {
    const ok = await runBackground(deps);
    return ok
      ? BackgroundTask.BackgroundTaskResult.Success
      : BackgroundTask.BackgroundTaskResult.Failed;
  } catch (e) {
    console.warn("background sync failed", e);
    return BackgroundTask.BackgroundTaskResult.Failed;
  }
});

// The interval that the task is registered with. The system keeps a task across app updates, so a
// new interval needs a new registration.
const intervalFile = () => new File(Paths.document, "background-interval");
const registeredInterval = (): number | null => {
  try {
    const f = intervalFile();
    const n = f.exists ? Number(f.textSync()) : Number.NaN;
    return Number.isFinite(n) ? n : null;
  } catch {
    return null;
  }
};

// Called when the app starts.
export async function registerBackgroundSync() {
  if ((await BackgroundTask.getStatusAsync()) !== BackgroundTask.BackgroundTaskStatus.Available)
    return;
  const registered = await TaskManager.isTaskRegisteredAsync(BACKGROUND_SYNC_TASK);
  if (registered && registeredInterval() === INTERVAL_MINUTES) return;
  if (registered) await BackgroundTask.unregisterTaskAsync(BACKGROUND_SYNC_TASK);
  await BackgroundTask.registerTaskAsync(BACKGROUND_SYNC_TASK, {
    minimumInterval: INTERVAL_MINUTES,
  });
  try {
    const f = intervalFile();
    if (!f.exists) f.create();
    f.writeSync(String(INTERVAL_MINUTES));
  } catch {
    // A lost marker only means one more registration at the next start.
  }
}
