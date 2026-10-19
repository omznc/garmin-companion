/**
 * The /v2 routes: the paid cloud coach. See worker/V2.md for the contract.
 *
 * The limits here are meant to hold even though the app binary is public:
 * credits are decided on the server, every request holds an upper-bound estimate
 * before it reaches the model, and the OpenRouter key has its own spend limit.
 * Like /v1, nothing here reads or logs a conversation.
 */
import { normalizeEmail, isSessionToken, sendCode, sessionUser, signOut, verifyCode } from "./auth";
import { createCheckout, createPortal, entitlementOf, handlePolarWebhook } from "./billing";
import type { V2Env } from "./env";
import { fail, json } from "./http";
import {
  costUnits,
  estimateUnits,
  type ModelProfile,
  profileOf,
  type Usage,
  UNITS_PER_USD,
} from "./models";
import { nextMidnight, type Reserved, type Snapshot } from "./wallet";

export { Wallet } from "./wallet";

const UPSTREAM = "https://openrouter.ai/api/v1/chat/completions";
const DAY_SECONDS = 86_400;
const INSTALL_TTL = 400 * DAY_SECONDS;
const INSTALLS_PER_IP = 3;
// The per-network cap above stops a farm. The daily total only stops a flood, and
// must be high enough for a release day, when every upgraded app enrols again.
const INSTALLS_PER_DAY = 5000;
const NEW_ACCOUNTS_PER_IP = 5;
const MAX_MESSAGES = 80;
const FREE_MAX_TOKENS = 4096;
const SPEND_SHARDS = 8;
const ALLOWED_FIELDS = new Set([
  "messages",
  "stream",
  "tools",
  "tool_choice",
  "response_format",
  "max_tokens",
  "temperature",
  "reasoning",
]);

const today = () => new Date().toISOString().slice(0, 10);
const secondsUntilMidnight = () =>
  Math.max(1, Math.ceil((Date.parse(nextMidnight(Date.now())) - Date.now()) / 1000));

type Who =
  | { kind: "install"; id: string; wallet: string }
  | { kind: "user"; id: string; email: string; wallet: string; token: string };

export async function handleV2(request: Request, env: V2Env, ctx: ExecutionContext) {
  const path = new URL(request.url).pathname.replace(/^\/v2/, "");
  const route = `${request.method} ${path}`;
  switch (route) {
    case "POST /install":
      return install(request, env, ctx);
    case "GET /balance":
    case "GET /me":
      return withWho(request, env, (who) => balance(env, who));
    case "POST /chat/completions":
      return withWho(request, env, (who) => chat(request, env, ctx, who));
    case "POST /auth/email/send":
      return authSend(request, env);
    case "POST /auth/email/verify":
      return authVerify(request, env);
    case "POST /auth/signout":
      return withWho(request, env, async (who) => {
        if (who.kind !== "user") return fail(401, "login_required");
        await signOut(env, who.token);
        return json({ ok: true });
      });
    case "POST /billing/checkout":
      return withWho(request, env, (who) =>
        who.kind === "user"
          ? createCheckout(env, { id: who.id, email: who.email })
          : fail(401, "login_required"),
      );
    case "POST /billing/portal":
      return withWho(request, env, (who) =>
        who.kind === "user" ? createPortal(env, who) : fail(401, "login_required"),
      );
    case "POST /webhooks/polar":
      return handlePolarWebhook(request, env);
    default:
      return fail(404, "not_found");
  }
}

/* ------------------------------------------------------------------ identity --- */

async function resolveWho(request: Request, env: V2Env): Promise<Who | Response> {
  const header = request.headers.get("Authorization") ?? "";
  const token = header.startsWith("Bearer ") ? header.slice(7).trim() : "";

  if (isSessionToken(token)) {
    const user = await sessionUser(env, token);
    if (!user) return fail(401, "session_expired");
    if (await env.COUNTERS.get(`v2block:user:${user.id}`)) return fail(403, "blocked");
    return { kind: "user", id: user.id, email: user.email, wallet: `user:${user.id}`, token };
  }
  if (!/^[0-9a-f]{32}$/.test(token)) return fail(401, "missing_token");

  if (await env.COUNTERS.get(`v2block:${token}`)) return fail(403, "blocked");
  // Well-formed is not the same as issued by this worker.
  if (!(await env.COUNTERS.get(`v2install:${token}`))) {
    return fail(401, "unknown_install", "this install id was not issued by this server");
  }
  if (env.FREE_REQUIRES_LOGIN === "true") return fail(401, "login_required");
  return { kind: "install", id: token, wallet: `install:${token}` };
}

async function withWho(
  request: Request,
  env: V2Env,
  next: (who: Who) => Response | Promise<Response>,
): Promise<Response> {
  const who = await resolveWho(request, env);
  if (who instanceof Response) return who;

  const { success } = await env.BURST.limit({ key: who.wallet });
  if (!success) return fail(429, "rate_limited", "too many requests", {}, { "Retry-After": "60" });
  return next(who);
}

async function addressBucket(request: Request, env: V2Env, day: string): Promise<string> {
  const address = request.headers.get("CF-Connecting-IP") ?? "local";
  const material = new TextEncoder().encode(`v2:${day}:${env.OPENROUTER_KEY}:${address}`);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", material));
  return Array.from(digest.slice(0, 8), (b) => b.toString(16).padStart(2, "0")).join("");
}

async function install(request: Request, env: V2Env, ctx: ExecutionContext) {
  const day = today();
  const bucket = await addressBucket(request, env, day);
  const { success } = await env.ISSUE.limit({ key: `v2:${bucket}` });
  const retry = { "Retry-After": "60" };
  if (!success)
    return fail(429, "rate_limited", "too many new installs from here just now", {}, retry);

  const [mine, total] = await Promise.all([
    env.COUNTERS.get(`v2issued:${day}:${bucket}`),
    env.COUNTERS.get(`v2issued:${day}:total`),
  ]);
  const midnight = { "Retry-After": String(secondsUntilMidnight()) };
  if (Number(mine ?? 0) >= INSTALLS_PER_IP)
    return fail(
      429,
      "install_limit",
      "this network has set up its installs for today",
      {},
      midnight,
    );
  if (Number(total ?? 0) >= INSTALLS_PER_DAY)
    return fail(429, "install_limit", "all new installs for today are taken", {}, midnight);

  const id = crypto.randomUUID().replace(/-/g, "");
  await env.COUNTERS.put(`v2install:${id}`, day, { expirationTtl: INSTALL_TTL });
  ctx.waitUntil(
    Promise.all([bump(env, `v2issued:${day}:${bucket}`, 1), bump(env, `v2issued:${day}:total`, 1)]),
  );
  return json({ id });
}

/* ------------------------------------------------------------------- account --- */

async function authSend(request: Request, env: V2Env) {
  const body = (await request.json().catch(() => null)) as { email?: unknown } | null;
  const email = normalizeEmail(body?.email);
  if (!email) return fail(400, "bad_email");
  // Per address too, so one machine cannot mail the world.
  const bucket = await addressBucket(request, env, today());
  if (!(await env.ISSUE.limit({ key: `v2mail:${bucket}` })).success) {
    return fail(429, "rate_limited", "too many codes requested", {}, { "Retry-After": "60" });
  }
  const capped = await newAccountCap(env, email, bucket);
  if (capped) return capped;
  const result = await sendCode(env, email);
  if (result === "sent") {
    await countNewAccount(env, email, bucket);
    return json({ ok: true });
  }
  if (result === "rate_limited")
    return fail(
      429,
      "rate_limited",
      "too many codes for this email",
      {},
      { "Retry-After": "3600" },
    );
  if (result === "mail_unavailable") return fail(503, "mail_unavailable");
  return fail(502, "mail_failed");
}

const userExists = async (env: V2Env, email: string) =>
  !!(await env.DB.prepare("SELECT 1 FROM user WHERE email = ?").bind(email).first());

/** Signed-in users get a free grant too, so new accounts have install-like caps. */
async function newAccountCap(env: V2Env, email: string, bucket: string) {
  if (await userExists(env, email)) return null;
  const day = today();
  const [mine, total] = await Promise.all([
    env.COUNTERS.get(`v2newuser:${day}:${bucket}`),
    env.COUNTERS.get(`v2newuser:${day}:total`),
  ]);
  if (Number(mine ?? 0) < NEW_ACCOUNTS_PER_IP && Number(total ?? 0) < INSTALLS_PER_DAY) return null;
  const retry = { "Retry-After": String(secondsUntilMidnight()) };
  return fail(429, "install_limit", "too many new accounts today", {}, retry);
}

async function countNewAccount(env: V2Env, email: string, bucket: string) {
  if (await userExists(env, email)) return;
  const day = today();
  await Promise.all([
    bump(env, `v2newuser:${day}:${bucket}`, 1),
    bump(env, `v2newuser:${day}:total`, 1),
  ]);
}

async function authVerify(request: Request, env: V2Env) {
  const body = (await request.json().catch(() => null)) as {
    email?: unknown;
    code?: unknown;
  } | null;
  const email = normalizeEmail(body?.email);
  const code = typeof body?.code === "string" ? body.code.trim() : "";
  if (!email || !/^\d{6}$/.test(code)) return fail(400, "bad_request");
  const session = await verifyCode(env, email, code);
  if (!session) return fail(401, "invalid_code");
  return json({ token: session.token, email });
}

async function balance(env: V2Env, who: Who) {
  const ent = who.kind === "user" ? await entitlementOf(env, who.id) : null;
  const snap = await callWallet<Snapshot>(env, who.wallet, {
    op: "snapshot",
    premium: !!ent?.premium,
  });
  return json({
    plan: ent?.premium ? "premium" : "free",
    premium: !!ent?.premium,
    email: who.kind === "user" ? who.email : null,
    source: ent?.source ?? null,
    renews_at: ent?.renews_at ?? null,
    cancels_at_period_end: ent?.cancels_at_period_end ?? false,
    ...snap,
  });
}

/* ------------------------------------------------------------------- the wallet --- */

async function callWallet<T>(env: V2Env, name: string, msg: Record<string, unknown>): Promise<T> {
  const stub = env.WALLET.get(env.WALLET.idFromName(name));
  const r = await stub.fetch("https://wallet/", { method: "POST", body: JSON.stringify(msg) });
  return (await r.json()) as T;
}

/* ------------------------------------------------------------------------ chat --- */

/** A sentence saying why the request is refused, or null. */
function problem(input: unknown, p: ModelProfile): string | null {
  if (typeof input !== "object" || input === null) return "body is not an object";
  const { model, messages } = input as Record<string, unknown>;
  if (model !== undefined && model !== p.slug) return "this endpoint serves one model";
  if (!Array.isArray(messages) || messages.length === 0) return "messages is required";
  return messages.length > MAX_MESSAGES ? "too many messages" : null;
}

/** Why a request cannot go upstream, or the body to send. */
function prepare(
  parsed: unknown,
  p: ModelProfile,
  premium: boolean,
): Record<string, unknown> | string {
  const why = problem(parsed, p);
  if (why) return why;
  const input = parsed as Record<string, unknown>;

  const out: Record<string, unknown> = { model: p.slug };
  for (const [k, v] of Object.entries(input)) if (ALLOWED_FIELDS.has(k)) out[k] = v;
  if (p.reasoning && out.reasoning === undefined) out.reasoning = p.reasoning;

  // Clamped, not rejected. Free users get a smaller ceiling so the hold stays
  // small enough to start a request late in the day.
  const cap = premium ? p.maxTokens : Math.min(p.maxTokens, FREE_MAX_TOKENS);
  const asked = Number(out.max_tokens);
  out.max_tokens = Number.isFinite(asked) && asked > 0 ? Math.min(asked, cap) : cap;
  out.usage = { include: true };
  if (out.stream) out.stream_options = { include_usage: true };
  return out;
}

async function readCapped(request: Request, max: number): Promise<string | null> {
  if (Number(request.headers.get("Content-Length") ?? 0) > max) return null;
  const raw = await request.text();
  // The header is a claim. The byte length is the fact.
  return new TextEncoder().encode(raw).length > max ? null : raw;
}

function refusal(r: Extract<Reserved, { ok: false }>): Response {
  const extra = { resets_at: r.snapshot.resets_at };
  const retry = { "Retry-After": String(secondsUntilMidnight()) };
  if (r.reason === "duplicate")
    return fail(409, "duplicate_request", "this request id was already used");
  if (r.reason === "fair_use_ceiling")
    return fail(429, "fair_use_ceiling", "premium fair-use ceiling reached", extra, retry);
  return fail(402, "out_of_credits", "no credits left today", extra);
}

async function chat(request: Request, env: V2Env, ctx: ExecutionContext, who: Who) {
  const p = profileOf(env.MODEL_PROFILE);
  const day = today();

  const budget = Number(env.DAILY_BUDGET_V2_USD);
  if (Number.isFinite(budget) && (await spentToday(env, day)) >= budget) {
    return fail(503, "coach_paused", "the cloud coach is paused");
  }

  const raw = await readCapped(request, p.maxBodyBytes);
  if (raw === null) return fail(413, "context_too_large", "the request is too large");
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return fail(400, "bad_request", "body is not JSON");
  }

  const premium = who.kind === "user" && (await entitlementOf(env, who.id)).premium;
  const forwarded = prepare(parsed, p, premium);
  if (typeof forwarded === "string") return fail(400, "bad_request", forwarded);

  const supplied = request.headers.get("X-Request-Id") ?? "";
  const requestId = /^[\w-]{8,64}$/.test(supplied) ? supplied : crypto.randomUUID();
  const body = JSON.stringify(forwarded);
  const estimate = estimateUnits(
    p,
    new TextEncoder().encode(body).length,
    forwarded.max_tokens as number,
  );

  const held = await callWallet<Reserved>(env, who.wallet, {
    op: "reserve",
    id: requestId,
    units: estimate,
    premium,
  });
  if (!held.ok) return refusal(held);

  const settle = async (units: number) => {
    const r = await callWallet<{ snapshot: Snapshot }>(env, who.wallet, {
      op: "settle",
      id: requestId,
      units,
      premium,
    });
    await bump(
      env,
      `v2spend:${day}:${Math.floor(Math.random() * SPEND_SHARDS)}`,
      units / UNITS_PER_USD,
    );
    return r.snapshot;
  };
  const release = () => callWallet(env, who.wallet, { op: "release", id: requestId, premium });

  const upstream = await callUpstream(env, body);
  if (!upstream.ok || !upstream.body) {
    await release();
    return upstreamRefusal(upstream.status, await upstream.text().catch(() => ""));
  }

  const headers = (snap: Snapshot) => ({
    "Content-Type": upstream.headers.get("Content-Type") ?? "text/event-stream",
    "Cache-Control": "no-store",
    "X-Credits-Remaining": String(snap.credits_remaining),
    "X-Credits-Reset": snap.resets_at,
  });

  if (!(upstream.headers.get("Content-Type") ?? "").includes("event-stream")) {
    const text = await upstream.text();
    const parsed = safeJson(text);
    // OpenRouter answers 200 and puts a provider failure in the body.
    if (parsed?.error && !parsed.choices) {
      await release();
      return upstreamRefusal(502, text);
    }
    const units = costUnits(p, parsed?.usage) ?? estimate;
    return new Response(text, { status: 200, headers: headers(await settle(units)) });
  }

  // The headers go out before the answer, so they show the balance before this
  // request is charged. GET /v2/balance has the exact figure afterwards.
  const stream = meterStream(upstream.body, (usage, failed) => {
    // A provider error before any content costs nothing.
    if (failed) return void ctx.waitUntil(release());
    // No usage block (a cut stream, a client that left): charge the estimate.
    // Refunding there would let a caller abort every request for free.
    ctx.waitUntil(settle(costUnits(p, usage) ?? estimate));
  });
  return new Response(stream, { status: 200, headers: headers(held.snapshot) });
}

async function callUpstream(env: V2Env, body: string): Promise<Response> {
  try {
    return await fetch(UPSTREAM, {
      method: "POST",
      headers: {
        // A separate key keeps a /v2 key limit from stopping /v1 users.
        Authorization: `Bearer ${env.OPENROUTER_KEY_V2 ?? env.OPENROUTER_KEY}`,
        "Content-Type": "application/json",
        "HTTP-Referer": "https://github.com/omznc/garmin-companion",
        "X-Title": "Garmin Companion (hosted)",
      },
      body,
    });
  } catch {
    // Treated like any other upstream failure by the caller.
    return new Response(null, { status: 502 });
  }
}

function upstreamRefusal(status: number, text: string): Response {
  // OpenRouter refuses with 402 when the key limit is reached. It uses 403 for
  // a limit too, but also for content refusals, so 403 needs a matching word.
  if (status === 402 || (status === 403 && /limit|credit|budget/i.test(text))) {
    return fail(503, "coach_paused", "the cloud coach is paused");
  }
  return fail(502, "upstream_error", "the model provider failed", { upstream_status: status });
}

function safeJson(text: string): Record<string, any> | undefined {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

/**
 * Passes the SSE stream through and calls `done` once, with the usage if any.
 * `failed` is true when the provider sent an error and no content or usage.
 */
function meterStream(
  source: ReadableStream<Uint8Array>,
  done: (usage: Usage | undefined, failed: boolean) => void,
): ReadableStream<Uint8Array> {
  const reader = source.getReader();
  const decoder = new TextDecoder();
  let tail = "";
  let usage: Usage | undefined;
  let sawContent = false;
  let sawError = false;
  let finished = false;
  const finish = () => {
    if (!finished) {
      finished = true;
      done(usage, sawError && !sawContent && !usage);
    }
  };
  const scan = (chunk: Uint8Array) => {
    tail += decoder.decode(chunk, { stream: true });
    const events = tail.split("\n\n");
    tail = events.pop() ?? "";
    for (const line of events.flatMap((e) => e.split("\n"))) {
      if (!line.startsWith("data:") || line.includes("[DONE]")) continue;
      try {
        const j = JSON.parse(line.slice(5).trim());
        const u = j?.usage;
        if (u && typeof u.prompt_tokens === "number") usage = u;
        if (j?.error) sawError = true;
        if (j?.choices?.some((c: any) => c?.delta?.content)) sawContent = true;
      } catch {
        // A broken chunk is a missing usage, which charges the estimate.
      }
    }
  };
  return new ReadableStream({
    async pull(controller) {
      try {
        const { done: end, value } = await reader.read();
        if (end) {
          finish();
          controller.close();
          return;
        }
        controller.enqueue(value);
        scan(value);
      } catch (err) {
        finish();
        controller.error(err);
      }
    },
    cancel(reason) {
      finish();
      return reader.cancel(reason);
    },
  });
}

/* ------------------------------------------------------------------ the budget --- */

async function spentToday(env: V2Env, day: string): Promise<number> {
  const shards = await Promise.all(
    Array.from({ length: SPEND_SHARDS }, (_, i) => env.COUNTERS.get(`v2spend:${day}:${i}`)),
  );
  return shards.reduce((n, v) => n + Number(v ?? 0), 0);
}

/** A soft counter in KV. It can lose an increment under load, like /v1's. */
async function bump(env: V2Env, key: string, by: number): Promise<void> {
  try {
    const now = Number((await env.COUNTERS.get(key)) ?? 0);
    await env.COUNTERS.put(key, String(now + by), { expirationTtl: DAY_SECONDS * 2 });
  } catch {
    // Bookkeeping is not worth a failed request.
  }
}
