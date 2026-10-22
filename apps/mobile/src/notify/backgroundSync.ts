// The OS background task. The system starts it about every few hours: a quick sync of the recent days.
// It runs in a headless JavaScript context, so this file is imported from the entry file (index.ts).
// Importing data/source starts the Rust core with the same data folder as the app.
import * as BackgroundTask from "expo-background-task";
import * as TaskManager from "expo-task-manager";
import { getStatus, historyPending, lastSyncedAt } from "../data/account";
import { QUICK_DAYS } from "../data/importLabel";
import { isSampleData } from "../data/source";
import { startSync } from "../data/useSync";
import { shouldRunBackground } from "./backgroundPolicy";

export const BACKGROUND_SYNC_TASK = "garmin-background-sync";
// Minutes. Android WorkManager allows 15 at the least. iOS treats it as the earliest start only.
const INTERVAL_MINUTES = 180;

export async function backgroundSync(now = Date.now()): Promise<void> {
  const status = await getStatus();
  const go = shouldRunBackground({
    signedIn: status.connected,
    sample: isSampleData(),
    historyPending: historyPending(),
    lastAt: lastSyncedAt(),
    now,
  });
  // The core takes `sync.lock` in sync_all, so a run in the app and this run never overlap.
  if (go) await startSync({ days: QUICK_DAYS });
}

TaskManager.defineTask(BACKGROUND_SYNC_TASK, async () => {
  try {
    await backgroundSync();
    return BackgroundTask.BackgroundTaskResult.Success;
  } catch {
    return BackgroundTask.BackgroundTaskResult.Failed;
  }
});

// Called when the app starts. A task that is already registered stays as it is.
export async function registerBackgroundSync() {
  if ((await BackgroundTask.getStatusAsync()) !== BackgroundTask.BackgroundTaskStatus.Available)
    return;
  if (await TaskManager.isTaskRegisteredAsync(BACKGROUND_SYNC_TASK)) return;
  await BackgroundTask.registerTaskAsync(BACKGROUND_SYNC_TASK, {
    minimumInterval: INTERVAL_MINUTES,
  });
}
