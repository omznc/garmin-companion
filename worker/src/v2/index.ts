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
import { checkoutDone } from "./checkout";
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
import type { Cap, Taken } from "./limits";
import { nextMidnight, type Reserved, type Snapshot } from "./wallet";

export { Limits } from "./limits";
export { Wallet } from "./wallet";

const UPSTREAM = "https://openrouter.ai/api/v1/chat/completions";
const DAY_SECONDS = 86_400;
const INSTALL_TTL = 400 * DAY_SECONDS;
const INSTALLS_PER_IP = 10;
// The per-network cap above stops a farm. The daily total only stops a flood, and
// must be high enough for a release day, when every upgraded app enrols again.
const INSTALLS_PER_DAY = 5000;
const NEW_ACCOUNTS_PER_IP = 5;
// Each new account gets a free grant and costs one mail, so it keeps a low cap.
const NEW_ACCOUNTS_PER_DAY = 200;
const MAX_MESSAGES = 80;
const FREE_MAX_TOKENS = 4096;
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

/** A thrown error becomes the JSON error shape. The log line has no request data. */
export async function handleV2(request: Request, env: V2Env, ctx: ExecutionContext) {
  try {
    return await route(request, env, ctx);
  } catch (err) {
    console.error("v2 failed", err instanceof Error ? err.message : "unknown");
    return fail(500, "internal_error", "the server failed");
  }
}

async function route(request: Request, env: V2Env, ctx: ExecutionContext) {
  const path = new URL(request.url).pathname.replace(/^\/v2/, "");
  switch (`${request.method} ${path}`) {
    case "POST /install":
      return install(request, env);
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
    case "GET /checkout/done":
      return checkoutDone();
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

/** An IPv6 network is a /64: one home gets a whole /64, so the first 4 groups name it. */
export function networkOf(address: string): string {
  if (!address.includes(":")) return address;
  const [head = "", tail = ""] = address.split("::");
  const first = head ? head.split(":") : [];
  const last = tail ? tail.split(":") : [];
  const gap = address.includes("::") ? Math.max(0, 8 - first.length - last.length) : 0;
  const groups = [...first, ...Array<string>(gap).fill("0"), ...last];
  return groups
    .slice(0, 4)
    .map((g) => g.toLowerCase().padStart(4, "0"))
    .join(":");
}

async function addressBucket(request: Request, env: V2Env, day: string): Promise<string> {
  const address = networkOf(request.headers.get("CF-Connecting-IP") ?? "local");
  const material = new TextEncoder().encode(`v2:${day}:${env.OPENROUTER_KEY}:${address}`);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", material));
  return Array.from(digest.slice(0, 8), (b) => b.toString(16).padStart(2, "0")).join("");
}

async function install(request: Request, env: V2Env) {
  const day = today();
  const bucket = await addressBucket(request, env, day);
  const { success } = await env.ISSUE.limit({ key: `v2:${bucket}` });
  const retry = { "Retry-After": "60" };
  if (!success)
    return fail(429, "rate_limited", "too many new installs from here just now", {}, retry);

  // The check and the count are one atomic step in the Limits object.
  const taken = await callLimits<Taken>(env, {
    op: "take",
    caps: [
      [`issued:${bucket}`, INSTALLS_PER_IP],
      ["issued:total", INSTALLS_PER_DAY],
    ] satisfies Cap[],
  });
  if (!taken.ok) {
    const midnight = { "Retry-After": String(secondsUntilMidnight()) };
    const message =
      taken.key === "issued:total"
        ? "all new installs for today are taken"
        : "this network has set up its installs for today";
    return fail(429, "install_limit", message, {}, midnight);
  }

  const id = crypto.randomUUID().replace(/-/g, "");
  await env.COUNTERS.put(`v2install:${id}`, day, { expirationTtl: INSTALL_TTL });
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
  // Only a look here, to save a mail. The count happens when the account is made.
  if (!(await userExists(env, email)) && !(await newAccountTaken(env, bucket, false)).ok) {
    return newAccountCapped();
  }
  const result = await sendCode(env, email);
  if (result === "sent") return json({ ok: true });
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

/**
 * Signed-in users get a free grant too, so new accounts have install-like caps.
 * With `commit` true the account is counted, in the same step as the check.
 */
const newAccountTaken = (env: V2Env, bucket: string, commit: boolean) =>
  callLimits<Taken>(env, {
    op: "take",
    commit,
    caps: [
      [`newuser:${bucket}`, NEW_ACCOUNTS_PER_IP],
      ["newuser:total", NEW_ACCOUNTS_PER_DAY],
    ] satisfies Cap[],
  });

const newAccountCapped = () =>
  fail(
    429,
    "install_limit",
    "too many new accounts today",
    {},
    { "Retry-After": String(secondsUntilMidnight()) },
  );

async function authVerify(request: Request, env: V2Env) {
  const body = (await request.json().catch(() => null)) as {
    email?: unknown;
    code?: unknown;
  } | null;
  const email = normalizeEmail(body?.email);
  const code = typeof body?.code === "string" ? body.code.trim() : "";
  if (!email || !/^\d{6}$/.test(code)) return fail(400, "bad_request");
  // A wrong try counts against every live code, so a network that guesses must slow down.
  const bucket = await addressBucket(request, env, today());
  if (!(await env.ISSUE.limit({ key: `v2verify:${bucket}` })).success) {
    return fail(429, "rate_limited", "too many tries", {}, { "Retry-After": "60" });
  }
  const session = await verifyCode(
    env,
    email,
    code,
    Date.now(),
    async () => (await newAccountTaken(env, bucket, true)).ok,
  );
  if (session === "capped") return newAccountCapped();
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

/** A Durable Object call. A non-2xx answer is an error, not data. */
async function callObject<T>(
  ns: DurableObjectNamespace,
  name: string,
  msg: Record<string, unknown>,
): Promise<T> {
  const stub = ns.get(ns.idFromName(name));
  const r = await stub.fetch("https://object/", { method: "POST", body: JSON.stringify(msg) });
  if (!r.ok) throw new Error(`durable object answered ${r.status}`);
  return (await r.json()) as T;
}

const callWallet = <T>(env: V2Env, name: string, msg: Record<string, unknown>) =>
  callObject<T>(env.WALLET, name, msg);

/** One object for all the daily limits. See limits.ts. */
const callLimits = <T>(env: V2Env, msg: Record<string, unknown>) =>
  callObject<T>(env.LIMITS, "daily", msg);

/* ------------------------------------------------------------------------ chat --- */

const isObject = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

/**
 * Only text reaches the model, and only function tools. An OpenRouter server
 * tool (web search, an advisor model) or an image part costs money that the
 * wallet does not count, and it breaks the bytes-per-token hold.
 */
function content(message: unknown): string | null {
  if (!isObject(message)) return "a message is not an object";
  const c = message.content;
  if (c === undefined || c === null || typeof c === "string") return null;
  if (!Array.isArray(c)) return "message content must be text";
  const text = (part: unknown) => isObject(part) && part.type === "text";
  return c.every(text) ? null : "only text content parts are accepted";
}

function toolsProblem(input: Record<string, unknown>): string | null {
  const { tools, tool_choice: choice, reasoning } = input;
  if (tools !== undefined) {
    const fn = (t: unknown) => isObject(t) && t.type === "function";
    if (!Array.isArray(tools) || !tools.every(fn)) return "only function tools are accepted";
  }
  if (choice !== undefined && typeof choice !== "string") {
    if (!isObject(choice) || choice.type !== "function") return "tool_choice must name a function";
  }
  if (reasoning !== undefined) {
    const keys = isObject(reasoning) ? Object.keys(reasoning) : ["?"];
    if (keys.some((k) => k !== "effort")) return "reasoning accepts only effort";
  }
  return null;
}

/** A sentence saying why the request is refused, or null. */
function problem(input: unknown, p: ModelProfile): string | null {
  if (!isObject(input)) return "body is not an object";
  const { model, messages } = input;
  if (model !== undefined && model !== p.slug) return "this endpoint serves one model";
  if (!Array.isArray(messages) || messages.length === 0) return "messages is required";
  if (messages.length > MAX_MESSAGES) return "too many messages";
  for (const m of messages) {
    const why = content(m);
    if (why) return why;
  }
  return toolsProblem(input);
}

/** The model fields of the body. ALLOWED_FIELDS keeps the client from setting them. */
const routeOf = (p: ModelProfile): Record<string, unknown> => ({
  ...(p.free ? { models: [p.free, p.slug] } : { model: p.slug }),
  ...(p.provider && { provider: p.provider }),
});

/** Why a request cannot go upstream, or the body to send. */
function prepare(
  parsed: unknown,
  p: ModelProfile,
  premium: boolean,
): Record<string, unknown> | string {
  const why = problem(parsed, p);
  if (why) return why;
  const input = parsed as Record<string, unknown>;

  const out: Record<string, unknown> = { ...routeOf(p) };
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

  // Premium has its own budget: free installs must not pause a paying user.
  const premium = who.kind === "user" && (await entitlementOf(env, who.id)).premium;
  if (await overBudget(env, premium)) {
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
    // The charge is final here. A failed count must not fail the answer.
    await callLimits(env, { op: "add", key: spendKey(premium), by: units / UNITS_PER_USD }).catch(
      (err) => console.error("v2 spend count failed", err instanceof Error ? err.message : ""),
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
    // A body that breaks off is a failed call: nothing was read, so nothing is charged.
    const text = await upstream.text().catch(() => null);
    if (text === null) {
      await release();
      return upstreamRefusal(502, "");
    }
    const parsed = safeJson(text);
    // OpenRouter answers 200 and puts a provider failure in the body.
    if (parsed?.error && !parsed.choices) {
      await release();
      return upstreamRefusal(502, text);
    }
    const units = costUnits(p, parsed?.usage, parsed?.model) ?? estimate;
    return new Response(text, { status: 200, headers: headers(await settle(units)) });
  }

  // The headers go out before the answer, so they show the balance before this
  // request is charged. GET /v2/balance has the exact figure afterwards.
  const stream = meterStream(upstream.body, (usage, failed, served) => {
    // Free: a provider error before any content, or a stream that sent no
    // data line for NO_DATA_REFUND_MS (a stall). See meterStream.
    if (failed) return void ctx.waitUntil(release());
    // Anything else with no usage block (a cut stream, a client that left,
    // an abort before 60 s): charge the estimate. Refunding there would let a
    // caller abort every request for free.
    ctx.waitUntil(settle(costUnits(p, usage, served) ?? estimate));
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

/** A stream with no data line for this long is a stall, and it is not charged. */
const NO_DATA_REFUND_MS = 60_000;
/** A data line is any `data:` line that is not exactly `[DONE]`. */
const DATA_LINE = /^data:(?!\s*\[DONE\]\s*$)/;

/**
 * Passes the SSE stream through and calls `done` once, with the usage if any.
 * `failed` is true when the hold is released and nothing is charged: the
 * provider sent an error and no content or usage, or the stream sent no data
 * line at all and NO_DATA_REFUND_MS passed since it started. A shorter stream
 * with no data line is not free, or a caller could abort for free.
 */
function meterStream(
  source: ReadableStream<Uint8Array>,
  done: (usage: Usage | undefined, failed: boolean, served: string | undefined) => void,
): ReadableStream<Uint8Array> {
  const reader = source.getReader();
  const decoder = new TextDecoder();
  let tail = "";
  let usage: Usage | undefined;
  let served: string | undefined;
  let sawContent = false;
  let sawData = false;
  let sawError = false;
  let finished = false;
  const started = Date.now();
  // The start of the line that has no newline yet. 32 characters decide it.
  let open = "";
  // Look at the raw text, before any split, so no format hides a data line.
  const sniff = (text: string) => {
    const lines = (open + text).split("\n");
    open = lines.pop()?.slice(0, 32) ?? "";
    if (!sawData) sawData = lines.some((l) => DATA_LINE.test(l));
  };
  const finish = () => {
    if (finished) return;
    finished = true;
    // A cut line counts as data. A cut "[DONE]" does too: this errs to a charge.
    if (!sawData && open.startsWith("data:")) sawData = true;
    const stalled = !sawData && Date.now() - started >= NO_DATA_REFUND_MS;
    done(usage, stalled || (sawError && !sawContent && !usage), served);
  };
  const scan = (chunk: Uint8Array) => {
    const text = decoder.decode(chunk, { stream: true });
    sniff(text);
    tail += text;
    const events = tail.split("\n\n");
    tail = events.pop() ?? "";
    for (const line of events.flatMap((e) => e.split("\n"))) {
      if (!DATA_LINE.test(line)) continue;
      try {
        const j = JSON.parse(line.slice(5).trim());
        const u = j?.usage;
        if (u && typeof u.prompt_tokens === "number") usage = u;
        if (typeof j?.model === "string") served = j.model;
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

/** Free and premium spend are counted apart, so one cannot pause the other. */
const spendKey = (premium: boolean) => (premium ? "pspend" : "spend");

/**
 * The daily total, in USD, that stops the coach. An unset or empty value means
 * no total cap: only the wallet limits and the OpenRouter key limit apply. That
 * is the case for premium, so a paid user is never paused by a budget.
 */
function budgetOf(env: V2Env, premium: boolean): number | null {
  const raw = premium ? env.DAILY_BUDGET_V2_PREMIUM_USD : env.DAILY_BUDGET_V2_USD;
  const budget = raw === undefined || raw.trim() === "" ? Number.NaN : Number(raw);
  return Number.isFinite(budget) ? budget : null;
}

async function overBudget(env: V2Env, premium: boolean): Promise<boolean> {
  const budget = budgetOf(env, premium);
  if (budget === null) return false;
  const { value } = await callLimits<{ value: number }>(env, { op: "get", key: spendKey(premium) });
  return value >= budget;
}
