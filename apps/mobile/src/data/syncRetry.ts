// Android 15 blocks the network of an app in the background. A sync that fails offline after the app
// left the foreground is worth one more run when the app comes back. A sync that never ran in the
// foreground (the OS background task) does not wait, because the app may never come back.
export function shouldRetryOffline(error: unknown, states: readonly string[]): boolean {
  const offline = (error as { kind?: string } | null)?.kind === "offline";
  return offline && states[0] === "active" && states.some((s) => s !== "active");
}
