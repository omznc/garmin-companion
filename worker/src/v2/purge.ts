/**
 * The cron job. It deletes rows that nothing can use any more, so the tables do
 * not grow for ever. Nothing that is still valid is touched.
 */
import type { V2Env } from "./env";

const DAY_MS = 86_400_000;
/** A webhook id only blocks a replay. The provider stops its retries long before this. */
const WEBHOOK_KEEP = 30 * DAY_MS;

export async function purgeExpired(env: Pick<V2Env, "DB">, now = Date.now()): Promise<void> {
  await env.DB.batch([
    env.DB.prepare("DELETE FROM login_code WHERE expires_at <= ?").bind(now),
    env.DB.prepare("DELETE FROM session WHERE expires_at <= ?").bind(now),
    env.DB.prepare("DELETE FROM webhook_event WHERE received_at < ?").bind(now - WEBHOOK_KEEP),
  ]);
}
