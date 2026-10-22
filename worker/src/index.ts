/**
 * The hosted coach worker.
 *
 * The paid coach lives in ./v2. This file only routes: the health check, the
 * cron cleanup, the /v2 dispatch, and a 410 for the removed /v1 routes.
 *
 * Nothing here logs a message, and `cloudflare.config.ts` has no observability
 * block. The request bodies are health data.
 */

import { handleV2 } from "./v2";
import type { V2Env } from "./v2/env";
import { purgeExpired } from "./v2/purge";

export { Limits, Wallet } from "./v2";

export default {
  /** The cron trigger. It cleans the /v2 tables. */
  async scheduled(_event: ScheduledController, env: V2Env): Promise<void> {
    await purgeExpired(env);
  },

  async fetch(request: Request, env: V2Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === "/health") return new Response("ok");
    if (url.pathname.startsWith("/v2/")) return handleV2(request, env, ctx);

    // The old v0.9.1 app. A clear answer keeps it from retrying or hanging.
    if (url.pathname.startsWith("/v1/")) return gone();

    return new Response("not found", { status: 404 });
  },
} satisfies ExportedHandler<V2Env>;

/** The OpenAI error shape, so the old app shows the message. It has no final period: the old app adds one. */
function gone(): Response {
  const message = "This version of Garmin Companion is no longer supported. Update the app";
  return new Response(JSON.stringify({ error: { message, code: 410, type: "gone" } }), {
    status: 410,
    headers: { "Content-Type": "application/json" },
  });
}
