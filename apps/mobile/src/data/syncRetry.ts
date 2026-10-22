// Android 15 blocks the network of an app in the background. An offline failure of a run that was
// in the background at any time is a cut-off run: the app runs it again in the foreground. An offline
// failure with the app in the foreground the whole time is a real loss of network, and shows its error.
// The states start with the AppState at the start of the run.
export function shouldRetryOffline(error: unknown, states: readonly string[]): boolean {
  const offline = (error as { kind?: string } | null)?.kind === "offline";
  return offline && states.some((s) => s !== "active");
}
