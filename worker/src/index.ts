/**
 * The hosted coach: an OpenAI-compatible proxy in front of one OpenRouter key.
 *
 * The desktop app can talk to OpenRouter with the athlete's own key, or to an
 * Ollama on their machine. Both ask something of them before the first question
 * gets answered. This is the third option and the default one — it asks nothing,
 * and the bill lands here instead.
 *
 * Which is the whole design problem. An unauthenticated OpenAI-compatible
 * endpoint with a funded key behind it is a free API to anyone who finds it, and
 * they will. So every request has to get past, in order: an install id this
 * worker issued, a burst limit, a per-install daily count, and a global daily
 * spend ceiling.
 *
 * The first of those used to be weaker than it looked. The id was minted on the
 * athlete's machine and this worker accepted any 32 hex characters, so the
 * per-install cap bound nobody — a caller who wanted more sent a different
 * random string, and the only thing actually holding was the budget. Ids are
 * issued here now, from `/v1/install`, rate-limited by address and capped per
 * day. That doesn't make an id expensive, but it makes it cost something, which
 * is the difference between a counter and a decoration. The budget ceiling is
 * still the guarantee; the rest is what keeps ordinary use from reaching it.
 *
 * What this deliberately does not do is look at the conversation. The body is
 * health data — resting heart rates, sleep, weight — and it is checked for shape
 * and size and then forwarded. Nothing here logs a message, and there is no
 * observability block in `wrangler.jsonc` that would do it from underneath.
 */

interface Env {
  /** `wrangler secret put OPENROUTER_KEY` */
  OPENROUTER_KEY: string;
  MODEL: string;
  DAILY_BUDGET_USD: string;
  COUNTERS: KVNamespace;
  BURST: RateLimit;
  /** The burst limit on handing out new ids, keyed by address rather than id. */
  ISSUE: RateLimit;
}

const UPSTREAM = "https://openrouter.ai/api/v1/chat/completions";

/**
 * Caps on one request. The app is the only intended caller and stays well under
 * all of these — history is trimmed to 20k characters, a turn is at most seven
 * rounds, and the one-shot calls ask for a few hundred tokens. They exist for
 * the caller that isn't the app.
 */
const MAX_BODY_BYTES = 256 * 1024;
const MAX_MESSAGES = 80;

/**
 * The output ceiling, and it has to be generous for a reason that isn't obvious.
 *
 * The model this serves reasons before it answers, and those reasoning tokens
 * come out of `max_tokens` — OpenRouter documents effort levels as a percentage
 * of exactly this budget. So a ceiling that looks comfortable for the prose can
 * be entirely consumed by the thinking, and what comes back is `content: null`
 * with `finish_reason: "length"`. An empty answer, not a short one.
 *
 * The app's main turn deliberately sends no ceiling at all. Imposing a tight
 * one here would truncate answers that were fine before the proxy existed, so
 * this is set well above any coaching answer and left to the daily budget to
 * bound in aggregate.
 */
export const MAX_TOKENS = 8192;

/** Requests per install per UTC day. A heavy day of real use is far under. */
const DAILY_PER_DEVICE = 400;

const DAY_SECONDS = 86_400;

/**
 * How long an install is remembered without being used.
 *
 * Long, because the failure it causes is bad out of proportion to what it
 * saves: an athlete who opens the app after a quiet winter and finds the coach
 * has forgotten them. The record is one small KV value, and it is re-stamped on
 * the first request of each day, so anything in regular use never approaches
 * this.
 */
const INSTALL_TTL = 400 * DAY_SECONDS;

/**
 * New ids per address per UTC day, and across everyone.
 *
 * These are the numbers that decide what an id costs. A household reinstalling
 * on three machines in one day is fine; a script wanting a thousand ids needs a
 * thousand addresses and several days, by which point it is cheaper for them to
 * pay OpenRouter than to bother.
 *
 * The global cap is the one that can hurt an innocent party — a genuinely
 * popular day would lock out new installs until midnight. It is set well above
 * any real day for a project this size, and it fails with a sentence that says
 * to come back tomorrow or use their own key, which is at least honest.
 */
const INSTALLS_PER_IP = 3;
const INSTALLS_PER_DAY = 200;

/**
 * The request fields that get forwarded. An allowlist rather than a blocklist:
 * OpenRouter has parameters that select providers, raise limits and change what
 * a request costs, and a proxy that passes through whatever it is handed has
 * given its callers control of its own bill.
 */
const ALLOWED_FIELDS = new Set([
  "messages",
  "stream",
  "stream_options",
  "tools",
  "tool_choice",
  "response_format",
  "max_tokens",
  "temperature",
  // How hard the model thinks before answering. Forwarded because the app's
  // small mechanical calls — three follow-up questions, three sentences of
  // weight prose — turn it off, and without this that instruction would be
  // stripped here and the thinking would eat their whole token budget.
  "reasoning",
]);

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === "/health") return new Response("ok");

    if (url.pathname === "/v1/install") {
      if (request.method !== "POST") return fail(405, "POST only");
      return issue(request, env, ctx);
    }

    if (url.pathname !== "/v1/chat/completions") return fail(404, "not found");
    if (request.method !== "POST") return fail(405, "POST only");

    const device = deviceId(request);
    if (!device) return fail(401, "missing or malformed install id");

    // The manual override: `wrangler kv key put --binding COUNTERS block:<id> 1`
    // for the one that turns up in the counters spending everyone else's day.
    if (await env.COUNTERS.get(`block:${device}`)) {
      return fail(403, "this install has been blocked");
    }

    // Well-formed is not the same as ours. Without this the id is just a shape,
    // and every limit counted against it is one a caller can step out of by
    // picking a different 32 characters.
    //
    // `unknown_install` is in the body because the app acts on it: it enrolls
    // again and retries, which is what makes a forgotten or hand-deleted id
    // heal instead of stranding someone. A block is a 403 and is deliberately
    // not that — re-enrolling past one costs an issue slot and is visible.
    const seen = await env.COUNTERS.get(`install:${device}`);
    if (!seen) {
      return fail(401, "this install id was not issued by this server", {}, "unknown_install");
    }

    const { success } = await env.BURST.limit({ key: device });
    if (!success) return fail(429, "too many requests", { "Retry-After": "60" });

    const day = new Date().toISOString().slice(0, 10);

    // Checked before the daily count, because it is the ceiling that matters:
    // a second install starts the count again and does nothing at all to this.
    // Getting that second install costs something now, which it didn't before,
    // but this is still the number that holds however many there are.
    const budget = Number(env.DAILY_BUDGET_USD);
    if (Number.isFinite(budget) && (await spentToday(env, day)) >= budget) {
      return fail(402, "the shared budget for today is spent");
    }

    const used = Number((await env.COUNTERS.get(`count:${day}:${device}`)) ?? 0);
    if (used >= DAILY_PER_DEVICE) {
      return fail(429, "this install is over its requests for today", {
        "Retry-After": String(secondsUntilUtcMidnight()),
      });
    }

    const raw = await readCapped(request);
    if (raw === null) return fail(413, "body too large");

    let body: unknown;
    try {
      body = JSON.parse(raw);
    } catch {
      return fail(400, "body is not JSON");
    }

    const forwarded = sanitize(body, env.MODEL);
    if (typeof forwarded === "string") return fail(400, forwarded);

    const upstream = await fetch(UPSTREAM, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${env.OPENROUTER_KEY}`,
        "Content-Type": "application/json",
        "HTTP-Referer": "https://github.com/omznc/garmin-companion",
        "X-Title": "Garmin Companion (hosted)",
      },
      body: JSON.stringify(forwarded),
    });

    // Counted on attempt rather than on success: a request that fails upstream
    // still cost a round trip, and counting only the good ones makes a client
    // stuck in a retry loop free.
    ctx.waitUntil(bump(env, `count:${day}:${device}`, 1, DAY_SECONDS * 2));

    // The install record holds the day it was last used, so this is at most one
    // write per install per day — enough to keep anything in regular use from
    // ever ageing out, without a write on every question.
    if (seen !== day) {
      ctx.waitUntil(env.COUNTERS.put(`install:${device}`, day, { expirationTtl: INSTALL_TTL }));
    }

    if (!upstream.ok || !upstream.body) {
      // Upstream's own status, so a 402 from OpenRouter still reads as "out of
      // credit" at the other end rather than as an unexplained 500.
      return new Response(await upstream.text(), {
        status: upstream.status,
        headers: { "Content-Type": "application/json" },
      });
    }

    const contentType = upstream.headers.get("Content-Type") ?? "text/event-stream";

    // Not every request through here is a conversation. The app also makes
    // small non-streaming calls — the follow-up suggestions after an answer, the
    // Weight screen's summary — and those come back as one JSON object with the
    // usage on it rather than as an SSE stream. Reading only the stream shape
    // would leave every one of them unmetered, which is a slow leak in exactly
    // the direction that matters: spend the ceiling never sees.
    if (!contentType.includes("event-stream")) {
      const text = await upstream.text();
      ctx.waitUntil(recordCost(env, day, costOf(text)));
      return new Response(text, {
        status: upstream.status,
        headers: { "Content-Type": contentType, "Cache-Control": "no-store" },
      });
    }

    // The cost lands in the last chunk of the stream, which means it arrives
    // after the answer has already gone to the athlete. So the spend is trailed
    // rather than pre-authorised: today's ceiling is enforced with yesterday's
    // arithmetic plus whatever has landed since. Being one in-flight turn over
    // budget is the accepted slack.
    return new Response(upstream.body.pipeThrough(meter(env, ctx, day)), {
      status: upstream.status,
      headers: { "Content-Type": contentType, "Cache-Control": "no-store" },
    });
  },
} satisfies ExportedHandler<Env>;

/* ------------------------------------------------------------------ auth --- */

/**
 * The bearer token, which is the app's per-install id and nothing else — no
 * account, no name, handed out by `/v1/install` and stored in the athlete's
 * keyring. It identifies an install so one can be counted and one can be
 * blocked; it identifies nobody.
 *
 * This only checks the shape. Whether it is an id this worker actually issued
 * is a KV lookup, and it happens on every request — the shape check is here to
 * keep a malformed token from becoming a KV key.
 */
export function deviceId(request: Request): string | null {
  const header = request.headers.get("Authorization") ?? "";
  const token = header.startsWith("Bearer ") ? header.slice(7).trim() : "";
  return /^[0-9a-f]{32}$/.test(token) ? token : null;
}

/* ------------------------------------------------------------- enrolment --- */

/**
 * Hand out an id for a new install.
 *
 * The whole value of this endpoint is that it is stingy, because an id is what
 * every other limit here is counted against. The address is the only thing
 * available to be stingy about — there is no account, and asking for one would
 * cost more privacy than the abuse is worth.
 *
 * Note what this does *not* do: prove the caller is the app. It can't. A binary
 * anyone can download cannot hold a secret from the person running it, and
 * anything shipped in it to sign a request with is a secret they already have.
 * So this doesn't pretend — it makes ids finite rather than unforgeable, and
 * leaves the daily budget to be the thing that can't be argued with.
 */
async function issue(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
  // Set by Cloudflare on the way in and overwritten if the caller sends their
  // own, so this is one of the few things in a request worth trusting. Absent
  // only under `wrangler dev`, where everything shares one bucket.
  const address = request.headers.get("CF-Connecting-IP") ?? "local";
  const day = new Date().toISOString().slice(0, 10);
  const bucket = await addressBucket(address, day, env);

  const { success } = await env.ISSUE.limit({ key: bucket });
  if (!success) {
    return fail(429, "too many new installs from here just now", { "Retry-After": "60" });
  }

  const [mine, total] = await Promise.all([
    env.COUNTERS.get(`issued:${day}:${bucket}`),
    env.COUNTERS.get(`issued:${day}:total`),
  ]);

  if (Number(mine ?? 0) >= INSTALLS_PER_IP) {
    return fail(429, "this network has set up its installs for today", {
      "Retry-After": String(secondsUntilUtcMidnight()),
    });
  }
  if (Number(total ?? 0) >= INSTALLS_PER_DAY) {
    return fail(429, "the coach has taken on all the new installs it can today — tomorrow, or use your own OpenRouter key", {
      "Retry-After": String(secondsUntilUtcMidnight()),
    });
  }

  // Awaited rather than trailed: the app's next act is to use this, and an id
  // the worker hasn't finished writing down is one it would answer 401 to.
  const id = crypto.randomUUID().replace(/-/g, "");
  await env.COUNTERS.put(`install:${id}`, day, { expirationTtl: INSTALL_TTL });

  ctx.waitUntil(
    Promise.all([
      bump(env, `issued:${day}:${bucket}`, 1, DAY_SECONDS * 2),
      bump(env, `issued:${day}:total`, 1, DAY_SECONDS * 2),
    ]),
  );

  return new Response(JSON.stringify({ id }), {
    headers: { "Content-Type": "application/json", "Cache-Control": "no-store" },
  });
}

/**
 * A per-address counter key that isn't the address.
 *
 * Rate limiting by address means keeping something per address, and the plain
 * thing would put every visitor's IP in a KV namespace that outlives their
 * visit — for a service whose stated position is that it keeps nothing. This
 * keeps a truncated hash instead, salted with the day so it rotates at midnight
 * and with a secret so the small IPv4 space can't simply be enumerated against
 * it. The API key is that secret because it is one this worker already holds;
 * it is used here as salt and nothing else.
 */
async function addressBucket(address: string, day: string, env: Env): Promise<string> {
  const material = new TextEncoder().encode(`${day}:${env.OPENROUTER_KEY}:${address}`);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", material));
  return Array.from(digest.slice(0, 8), (b) => b.toString(16).padStart(2, "0")).join("");
}

/* ---------------------------------------------------------------- request --- */

/** The body, or null if it is bigger than anything the app would send. */
async function readCapped(request: Request): Promise<string | null> {
  const declared = Number(request.headers.get("Content-Length") ?? 0);
  if (declared > MAX_BODY_BYTES) return null;

  const raw = await request.text();
  // The header is a claim; the length is the fact.
  return new TextEncoder().encode(raw).length > MAX_BODY_BYTES ? null : raw;
}

/**
 * What actually goes upstream, or a sentence saying why nothing will.
 *
 * The model is set here rather than read from the request. One model is what
 * this serves, and a proxy that forwards whichever id it is handed has let its
 * callers pick how much a request costs — which for the expensive models on
 * OpenRouter is three orders of magnitude of difference.
 */
export function sanitize(body: unknown, model: string): Record<string, unknown> | string {
  if (typeof body !== "object" || body === null) return "body is not an object";
  const input = body as Record<string, unknown>;

  if (input.model !== undefined && input.model !== model) {
    return `this endpoint only serves ${model}`;
  }

  const messages = input.messages;
  if (!Array.isArray(messages) || messages.length === 0) {
    return "messages is required";
  }
  if (messages.length > MAX_MESSAGES) return "too many messages";

  const out: Record<string, unknown> = { model };
  for (const [k, v] of Object.entries(input)) {
    if (ALLOWED_FIELDS.has(k)) out[k] = v;
  }

  // Clamped rather than rejected: a ceiling the caller didn't know about should
  // shorten their answer, not lose it.
  const asked = Number(out.max_tokens);
  out.max_tokens = Number.isFinite(asked) ? Math.min(asked, MAX_TOKENS) : MAX_TOKENS;

  // Not forwarded from the request — the app asks for it, but it is what makes
  // the metering below work at all, so it is set here regardless.
  out.usage = { include: true };
  return out;
}

/* ---------------------------------------------------------------- metering --- */

/**
 * How many keys the day's spend is spread across.
 *
 * KV allows one write per second *per key*, and every request in the fleet
 * would otherwise be incrementing the same `spend:<day>`. Past one request a
 * second the writes start losing to each other, and the counter that is
 * supposed to be the hard ceiling is the one drifting furthest below the truth
 * — exactly backwards. Sharding trades one extra read per request for eight
 * times the write headroom.
 *
 * Reading means summing all of them, which is why this is 8 and not 100.
 */
const SPEND_SHARDS = 8;

/** Today's spend so far, across every shard. */
async function spentToday(env: Env, day: string): Promise<number> {
  const shards = await Promise.all(
    Array.from({ length: SPEND_SHARDS }, (_, i) => env.COUNTERS.get(`spend:${day}:${i}`)),
  );
  return shards.reduce((total, v) => total + Number(v ?? 0), 0);
}

/**
 * Passes the stream through untouched while reading the cost out of it.
 *
 * OpenRouter puts a usage block in a chunk of its own at the end of an SSE
 * stream, carrying what the request actually cost in dollars. Watching for it
 * here is what makes the daily ceiling a spend ceiling rather than a request
 * count that hopes to approximate one.
 *
 * Every failure in here is swallowed. A malformed chunk means an unmetered
 * request, which is a small amount of money; throwing would mean a broken
 * answer, which is the thing the athlete actually notices.
 */
function meter(env: Env, ctx: ExecutionContext, day: string): TransformStream<Uint8Array, Uint8Array> {
  const decoder = new TextDecoder();
  let tail = "";
  let cost = 0;

  return new TransformStream({
    transform(chunk, controller) {
      controller.enqueue(chunk);
      try {
        tail += decoder.decode(chunk, { stream: true });
        // Whole SSE events only; a half-arrived one waits for the rest.
        const events = tail.split("\n\n");
        tail = events.pop() ?? "";
        for (const event of events) {
          for (const line of event.split("\n")) {
            if (!line.startsWith("data:")) continue;
            const data = line.slice(5).trim();
            if (!data || data === "[DONE]") continue;
            const usage = JSON.parse(data)?.usage;
            if (typeof usage?.cost === "number") cost = usage.cost;
          }
        }
      } catch {
        // An unparseable chunk is an unmetered request, not a failed one.
      }
    },
    flush() {
      ctx.waitUntil(recordCost(env, day, cost));
    },
  });
}

/** What one JSON (non-streamed) completion cost, or 0 if it doesn't say. */
export function costOf(body: string): number {
  try {
    const cost = JSON.parse(body)?.usage?.cost;
    return typeof cost === "number" && cost > 0 ? cost : 0;
  } catch {
    return 0;
  }
}

/**
 * Add one request's cost to the day's total.
 *
 * Which shard is arbitrary — they are summed on the way back in. Spread rather
 * than chosen so concurrent requests land on different keys, which is the whole
 * reason there is more than one.
 */
async function recordCost(env: Env, day: string, cost: number): Promise<void> {
  if (!(cost > 0)) return;
  const shard = Math.floor(Math.random() * SPEND_SHARDS);
  await bump(env, `spend:${day}:${shard}`, cost, DAY_SECONDS * 2);
}

/**
 * Add to a counter.
 *
 * Read-modify-write on eventually-consistent storage, so two requests landing
 * together can lose one of the increments — and KV additionally caps writes at
 * one per second per key, which is what `SPEND_SHARDS` exists to spread. That
 * is the right trade here: the alternative is a Durable Object per counter, and
 * what this protects is a budget with a soft edge rather than a balance that
 * has to reconcile. It still undercounts under load, which is why
 * `DAILY_BUDGET_USD` should be a number you can afford to overshoot rather than
 * one you cannot.
 */
async function bump(env: Env, key: string, by: number, ttl: number): Promise<void> {
  try {
    const now = Number((await env.COUNTERS.get(key)) ?? 0);
    await env.COUNTERS.put(key, String(now + by), { expirationTtl: ttl });
  } catch {
    // Bookkeeping is not worth a failed request.
  }
}

function secondsUntilUtcMidnight(): number {
  const now = new Date();
  const midnight = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() + 1);
  return Math.ceil((midnight - now.getTime()) / 1000);
}

/**
 * OpenAI's error shape, so a client that parses the body finds what it expects.
 *
 * `type` is for the one refusal the app does something about rather than shows.
 * Matching on a sentence would mean the wording could never be improved without
 * breaking a released build, so the machine-readable half is separate from the
 * half people read.
 */
function fail(
  status: number,
  message: string,
  headers: Record<string, string> = {},
  type?: string,
): Response {
  return new Response(JSON.stringify({ error: { message, code: status, ...(type && { type }) } }), {
    status,
    headers: { "Content-Type": "application/json", ...headers },
  });
}
