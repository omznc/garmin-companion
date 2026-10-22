// Export and share. The Rust core writes the file into the cache folder and
// returns the path. The system share sheet then sends the file.
import { Paths } from "expo-file-system";
import { Share } from "react-native";
import { showDialog } from "../ui/dialog";
import { notifyError } from "../ui/notify-error";
import { activitySummary } from "./activitySummary";
import type { ActivityDetailView } from "./generated/ActivityDetailView";
import { call } from "./source";

type FileKind = { mimeType: string; UTI: string };

const CSV: FileKind = { mimeType: "text/csv", UTI: "public.comma-separated-values-text" };
const PDF: FileKind = { mimeType: "application/pdf", UTI: "com.adobe.pdf" };

// The Rust core takes a plain path, not a file URI.
const cachePath = () => decodeURI(Paths.cache.uri.replace(/^file:\/\//, "").replace(/\/$/, ""));

async function shareFile(path: string, kind: FileKind) {
  // A build from before expo-sharing has no native module, so the require fails here and not at start.
  const Sharing = require("expo-sharing") as typeof import("expo-sharing");
  if (!(await Sharing.isAvailableAsync())) {
    showDialog("Sharing is not available", "This device cannot share files.");
    return;
  }
  await Sharing.shareAsync(`file://${encodeURI(path)}`, { ...kind, dialogTitle: "Share" });
}

// A failed export shows the sentence of the core. Closing the share sheet is not a failure.
async function exportAndShare(make: () => Promise<string>, kind: FileKind) {
  try {
    await shareFile(await make(), kind);
  } catch (e) {
    notifyError("Could not export", e);
  }
}

/** Every session in the cache, as one CSV file. */
export const shareActivitiesCsv = () =>
  exportAndShare(() => call("export_csv", { dir: cachePath() }), CSV);

/** One report as a PDF. `offset` 0 is the newest week, or the month of today. */
export const shareReportPdf = (period: "week" | "month", offset = 0) =>
  exportAndShare(() => call("export_report", { dir: cachePath(), period, offset }), PDF);

/** One session as text. */
export async function shareActivitySummary(d: ActivityDetailView) {
  try {
    await Share.share({ message: activitySummary(d) });
  } catch (e) {
    notifyError("Could not share", e);
  }
}

/** The weekly report: the person picks the week on screen or the month. */
export const askReportShare = (weekOffset: number) =>
  showDialog("Share the report", "Send the report as a PDF file.", [
    { text: "Cancel", style: "cancel" },
    { text: "This month", onPress: () => void shareReportPdf("month") },
    { text: "This week", onPress: () => void shareReportPdf("week", weekOffset) },
  ]);
