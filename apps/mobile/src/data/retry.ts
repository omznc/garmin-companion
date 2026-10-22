// The retry policy of the queries. The Rust core already retries a GET 3 times
// for a transport error, a 429 and a 5xx. The app must not repeat that work.
// shortcut: the kind is "internal" for most core faults, so we read the message.
// Only "offline" is a typed transport kind.

const NO_RETRY_KINDS = new Set(["not_connected", "bad_arguments", "unknown_command"]);
// "request to <path> failed" is the core text for a transport error.
const TRANSPORT = /^request to .+ failed/;
// An HTTP status or a dead sign-in: a second try gives the same result.
const REFUSED = /returned (4\d\d|5\d\d)|token refresh failed|AuthFailed/i;

const MAX_RETRIES = 1;

/** True for a transport error that the core did not retry in the same call. */
export function isTransient(error: unknown): boolean {
  if (!(error instanceof Error)) return false;
  const kind = (error as { kind?: string }).kind;
  if (kind === "offline") return true;
  if (kind && NO_RETRY_KINDS.has(kind)) return false;
  const message = error.message;
  return !REFUSED.test(message) && TRANSPORT.test(message.replace(/^internal: /, ""));
}

/** The `retry` option of the queries. */
export const shouldRetry = (failures: number, error: unknown) =>
  failures < MAX_RETRIES && isTransient(error);
