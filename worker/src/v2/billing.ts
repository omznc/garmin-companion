/**
 * Premium entitlement and the Polar integration.
 *
 * The `entitlement` table has one row per subscription at one source (polar,
 * app_store, play). Only Polar is wired up. A store source later writes the same
 * rows and `isPremium` needs no change.
 */
import type { V2Env } from "./env";
import { fail, json } from "./http";

const DAY_MS = 86_400_000;
const PAST_DUE_GRACE = 7 * DAY_MS;
const POLAR = "polar";

interface Row {
  status: string;
  current_period_end: number | null;
  past_due_since: number | null;
}

function rowIsPremium(r: Row, now: number): boolean {
  switch (r.status) {
    case "active":
    case "trialing":
      return true;
    case "canceled":
      return r.current_period_end !== null && now < r.current_period_end;
    case "past_due":
      return r.past_due_since !== null && now < r.past_due_since + PAST_DUE_GRACE;
    default:
      return false; // revoked, refunded, unpaid, incomplete
  }
}

export async function entitlementOf(env: V2Env, userId: string, now = Date.now()) {
  const { results } = await env.DB.prepare(
    "SELECT source, status, current_period_end, cancel_at_period_end, past_due_since FROM entitlement WHERE user_id = ?",
  )
    .bind(userId)
    .all<Row & { source: string; cancel_at_period_end: number }>();
  const live = results.filter((r) => rowIsPremium(r, now));
  return {
    premium: live.length > 0,
    source: live[0]?.source ?? null,
    renews_at: live[0]?.current_period_end
      ? new Date(live[0].current_period_end).toISOString()
      : null,
    cancels_at_period_end: live[0] ? live[0].cancel_at_period_end === 1 : false,
  };
}

/* -------------------------------------------------------- Standard Webhooks --- */

const b64 = (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));

/**
 * Standard Webhooks signature check: HMAC-SHA256 over `id.timestamp.body`, sent
 * as space-separated `v1,<base64>` values. A `whsec_` secret is base64. Polar's
 * secret is used as plain text bytes.
 */
export async function verifyWebhook(
  secret: string,
  headers: Headers,
  body: string,
  nowSec = Math.floor(Date.now() / 1000),
): Promise<boolean> {
  const id = headers.get("webhook-id");
  const ts = headers.get("webhook-timestamp");
  const sigs = headers.get("webhook-signature");
  if (!id || !ts || !sigs || !/^\d+$/.test(ts)) return false;
  if (Math.abs(nowSec - Number(ts)) > 300) return false;

  try {
    const keyBytes = secret.startsWith("whsec_")
      ? b64(secret.slice(6))
      : new TextEncoder().encode(secret);
    const key = await crypto.subtle.importKey(
      "raw",
      keyBytes,
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["verify"],
    );
    const signed = new TextEncoder().encode(`${id}.${ts}.${body}`);
    for (const part of sigs.split(" ")) {
      const [version, value] = part.split(",");
      if (version !== "v1" || !value) continue;
      // `verify` compares in constant time.
      if (await crypto.subtle.verify("HMAC", key, b64(value), signed)) return true;
    }
  } catch {
    // A malformed signature is a failed one.
  }
  return false;
}

/* --------------------------------------------------------------- the webhook --- */

const SUBSCRIPTION_EVENTS = new Set([
  "subscription.created",
  "subscription.active",
  "subscription.updated",
  "subscription.canceled",
  "subscription.uncanceled",
  "subscription.revoked",
]);

interface PolarEvent {
  type?: string;
  timestamp?: string;
  data?: Record<string, any>;
}

type Applied = "applied" | "ignored" | "unknown_customer";

async function applySubscription(env: V2Env, e: PolarEvent, at: number): Promise<Applied> {
  const d = e.data ?? {};
  const userId = d.customer?.external_id;
  if (typeof d.id !== "string" || typeof userId !== "string") return "unknown_customer";
  const user = await env.DB.prepare("SELECT id FROM user WHERE id = ?").bind(userId).first();
  if (!user) {
    // Not a customer this worker created. Nobody becomes premium from it.
    console.warn("polar webhook: unknown customer");
    return "unknown_customer";
  }
  const status = e.type === "subscription.revoked" ? "revoked" : String(d.status);
  const end = d.current_period_end ? Date.parse(d.current_period_end) : null;
  const now = Date.now();
  // Older than what is stored: skipped. The provider can deliver out of order.
  await env.DB.prepare(
    `INSERT INTO entitlement
       (source, external_id, user_id, status, current_period_end, cancel_at_period_end, past_due_since, last_event_at, updated_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, CASE WHEN ?4 = 'past_due' THEN ?7 END, ?7, ?8)
     ON CONFLICT (source, external_id) DO UPDATE SET
       status = excluded.status,
       current_period_end = excluded.current_period_end,
       cancel_at_period_end = excluded.cancel_at_period_end,
       past_due_since = CASE WHEN excluded.status = 'past_due'
         THEN COALESCE(CASE WHEN entitlement.status = 'past_due' THEN entitlement.past_due_since END, excluded.past_due_since)
         END,
       last_event_at = excluded.last_event_at,
       updated_at = excluded.updated_at
     WHERE excluded.last_event_at >= entitlement.last_event_at`,
  )
    .bind(POLAR, d.id, userId, status, end, d.cancel_at_period_end ? 1 : 0, at, now)
    .run();
  return "applied";
}

async function applyRefund(env: V2Env, e: PolarEvent, at: number): Promise<Applied> {
  const d = e.data ?? {};
  // A partial refund keeps the subscription. Only a full one ends premium.
  if (d.status !== "refunded" || typeof d.subscription_id !== "string") return "ignored";
  const r = await env.DB.prepare(
    "UPDATE entitlement SET status = 'refunded', last_event_at = ?, updated_at = ? WHERE source = ? AND external_id = ? AND last_event_at <= ?",
  )
    .bind(at, Date.now(), POLAR, d.subscription_id, at)
    .run();
  return r.meta.changes ? "applied" : "ignored";
}

export async function handlePolarWebhook(request: Request, env: V2Env): Promise<Response> {
  if (!env.POLAR_WEBHOOK_SECRET) return fail(503, "webhook_not_configured");
  // The raw text, because the signature covers the exact bytes.
  const raw = await request.text();
  if (!(await verifyWebhook(env.POLAR_WEBHOOK_SECRET, request.headers, raw))) {
    return fail(400, "bad_signature");
  }
  let event: PolarEvent;
  try {
    event = JSON.parse(raw);
  } catch {
    return fail(400, "bad_json");
  }
  const eventId = request.headers.get("webhook-id") as string;
  const type = String(event.type ?? "");
  const stamp = Date.parse(event.timestamp ?? "");
  const at = Number.isFinite(stamp)
    ? stamp
    : Number(request.headers.get("webhook-timestamp")) * 1000;

  const first = await env.DB.prepare(
    "INSERT OR IGNORE INTO webhook_event (source, event_id, type, received_at) VALUES (?, ?, ?, ?)",
  )
    .bind(POLAR, eventId, type, Date.now())
    .run();
  if (!first.meta.changes) return json({ ok: true, duplicate: true });

  try {
    let result: Applied = "ignored";
    if (SUBSCRIPTION_EVENTS.has(type)) result = await applySubscription(env, event, at);
    else if (type === "order.refunded") result = await applyRefund(env, event, at);
    return json({ ok: true, result });
  } catch (err) {
    // Forget the event id so the provider's retry is applied, not skipped.
    await env.DB.prepare("DELETE FROM webhook_event WHERE source = ? AND event_id = ?")
      .bind(POLAR, eventId)
      .run();
    console.error("polar webhook failed", err instanceof Error ? err.message : "unknown");
    return fail(500, "apply_failed");
  }
}

/* ---------------------------------------------------------------- Polar API --- */

async function polar(env: V2Env, path: string, body: unknown): Promise<Response> {
  return fetch(`${env.POLAR_API_BASE ?? "https://api.polar.sh"}${path}`, {
    method: "POST",
    headers: { Authorization: `Bearer ${env.POLAR_TOKEN}`, "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export async function createCheckout(env: V2Env, user: { id: string; email: string }) {
  if (!env.POLAR_TOKEN || !env.POLAR_PRODUCT_ID) return fail(503, "billing_unavailable");
  if ((await entitlementOf(env, user.id)).premium) return fail(409, "already_premium");
  const r = await polar(env, "/v1/checkouts/", {
    products: [env.POLAR_PRODUCT_ID],
    external_customer_id: user.id,
    customer_email: user.email,
    ...(env.CHECKOUT_RETURN_URL && { success_url: env.CHECKOUT_RETURN_URL }),
  });
  const out = (await r.json().catch(() => ({}))) as { url?: string };
  if (!r.ok || !out.url) return fail(502, "billing_failed");
  return json({ url: out.url });
}

export async function createPortal(env: V2Env, user: { id: string }) {
  if (!env.POLAR_TOKEN) return fail(503, "billing_unavailable");
  const r = await polar(env, "/v1/customer-sessions/", { external_customer_id: user.id });
  const out = (await r.json().catch(() => ({}))) as { customer_portal_url?: string };
  // Polar answers 404 or 422 for a customer that has never bought.
  if (r.status === 404 || r.status === 422) return fail(404, "no_customer");
  if (!r.ok || !out.customer_portal_url) return fail(502, "billing_failed");
  return json({ url: out.customer_portal_url });
}
