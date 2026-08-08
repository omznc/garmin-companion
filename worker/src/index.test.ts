/**
 * The checks between an anonymous request and a funded API key.
 *
 * Everything else in the worker is a counter or a pipe. These are the parts
 * where a bug is silent and expensive — a forwarded field that lets a caller
 * pick a different model, a token shape that lets one through unchecked, or an
 * enrolment endpoint that hands out ids faster than it means to — so they are
 * the parts worth pinning down.
 */
import { describe, expect, it } from "vitest";
import worker, { costOf, deviceId, MAX_TOKENS, sanitize } from "./index";

const MODEL = "inclusionai/ling-3.0-flash";
const ID = "0123456789abcdef0123456789abcdef";

const bearer = (token: string) =>
  new Request("https://example.com/v1/chat/completions", {
    method: "POST",
    headers: { Authorization: `Bearer ${token}` },
  });

/* ---------------------------------------------------- a worker to talk to --- */

type Env = Parameters<typeof worker.fetch>[1];
type Ctx = Parameters<typeof worker.fetch>[2];

const TODAY = new Date().toISOString().slice(0, 10);

/**
 * Enough of the bindings to run the parts that never reach OpenRouter.
 *
 * KV is a Map, which is a better model of it than it sounds: the real thing is
 * eventually consistent, and every counter here is already written as though a
 * read might be stale. What this deliberately keeps is the TTL each key was
 * written with, because the install record's lifetime is the difference between
 * a quiet winter and being locked out after one.
 */
function harness({ allowIssue = true }: { allowIssue?: boolean } = {}) {
  const kv = new Map<string, string>();
  const ttl = new Map<string, number | undefined>();
  let pending: Promise<unknown>[] = [];

  const env = {
    OPENROUTER_KEY: "not-a-real-key",
    MODEL,
    DAILY_BUDGET_USD: "5",
    COUNTERS: {
      get: async (key: string) => kv.get(key) ?? null,
      put: async (key: string, value: string, opts?: { expirationTtl?: number }) => {
        kv.set(key, value);
        ttl.set(key, opts?.expirationTtl);
      },
    },
    BURST: { limit: async () => ({ success: true }) },
    ISSUE: { limit: async () => ({ success: allowIssue }) },
  } as unknown as Env;

  const ctx = {
    waitUntil: (p: Promise<unknown>) => void pending.push(p),
    passThroughOnException: () => {},
  } as unknown as Ctx;

  return {
    kv,
    ttl,
    /** A call, with whatever it deferred settled before the next one starts. */
    async call(request: Request) {
      const response = await worker.fetch(request, env, ctx);
      await Promise.all(pending);
      pending = [];
      return response;
    },
    enroll() {
      return new Request("https://example.com/v1/install", { method: "POST" });
    },
    ask(token: string, body: unknown = { messages: [{ role: "user", content: "hi" }] }) {
      return new Request("https://example.com/v1/chat/completions", {
        method: "POST",
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    },
  };
}

describe("enrolment", () => {
  it("issues an id, and takes it back afterwards", async () => {
    const h = harness();

    const issued = await h.call(h.enroll());
    expect(issued.status).toBe(200);
    const { id } = (await issued.json()) as { id: string };

    // Whatever it hands out has to survive its own front door.
    expect(deviceId(bearer(id))).toBe(id);

    // And is then known. The body is deliberately junk, so this stops at
    // sanitize rather than at OpenRouter — a 400 about messages means every
    // check before it was passed, which is the thing being asserted.
    const used = await h.call(h.ask(id, {}));
    expect(used.status).toBe(400);
  });

  it("refuses a well-formed id it never issued", async () => {
    const h = harness();

    // The old failure in one line: this is a perfectly valid-looking token, and
    // before ids were issued here it was as good as any other.
    const response = await h.call(h.ask(ID));
    expect(response.status).toBe(401);

    const { error } = (await response.json()) as { error: { type?: string } };
    // Machine-readable, because the app re-enrols on exactly this and on
    // nothing else — a 403 block must not read as an invitation to try again.
    expect(error.type).toBe("unknown_install");
  });

  it("gives a fresh id a lifetime long enough to survive a quiet winter", async () => {
    const h = harness();
    const { id } = (await (await h.call(h.enroll())).json()) as { id: string };

    expect(h.ttl.get(`install:${id}`)).toBeGreaterThan(180 * 86_400);
  });

  it("stops one address collecting ids", async () => {
    const h = harness();

    // Three is the cap. A household setting up a few machines is fine; the
    // fourth in a day is a script, and it can wait for midnight.
    for (let i = 0; i < 3; i++) {
      expect((await h.call(h.enroll())).status).toBe(200);
    }

    const refused = await h.call(h.enroll());
    expect(refused.status).toBe(429);
    expect(refused.headers.get("Retry-After")).toBeTruthy();
  });

  it("stops everyone collecting ids", async () => {
    const h = harness();
    // A day that has already handed out its allowance.
    h.kv.set(`issued:${TODAY}:total`, "200");

    expect((await h.call(h.enroll())).status).toBe(429);
  });

  it("refuses a burst before it reaches the counters", async () => {
    const h = harness({ allowIssue: false });

    const refused = await h.call(h.enroll());
    expect(refused.status).toBe(429);
    // Nothing written: a burst is turned away before it can spend the day's
    // allowance, or a script would exhaust the global cap in one second.
    expect([...h.kv.keys()]).toHaveLength(0);
  });

  it("is POST only", async () => {
    const h = harness();
    const response = await h.call(new Request("https://example.com/v1/install"));
    expect(response.status).toBe(405);
  });
});

describe("deviceId", () => {
  it("takes a well-formed id and nothing else", () => {
    expect(deviceId(bearer(ID))).toBe(ID);

    // Anything that isn't 32 lowercase hex characters. That is what /v1/install
    // hands out; everything here is someone else's idea of a token.
    expect(deviceId(bearer(""))).toBeNull();
    expect(deviceId(bearer("short"))).toBeNull();
    expect(deviceId(bearer(ID.toUpperCase()))).toBeNull();
    expect(deviceId(bearer(`${ID}0`))).toBeNull();
    expect(deviceId(bearer("sk-or-v1-someones-actual-openrouter-key"))).toBeNull();
  });

  it("requires the scheme", () => {
    const req = new Request("https://example.com/", { headers: { Authorization: ID } });
    expect(deviceId(req)).toBeNull();
    expect(deviceId(new Request("https://example.com/"))).toBeNull();
  });
});

describe("costOf", () => {
  it("reads the cost off a non-streamed completion", () => {
    // The shape that went unmetered until a live request showed it: the app's
    // follow-up and weight-summary calls set stream:false, so their cost comes
    // back on one JSON object rather than in a final SSE chunk. Missing it is a
    // leak the budget ceiling never sees.
    const body = JSON.stringify({
      choices: [{ message: { content: "hi" } }],
      usage: { prompt_tokens: 26, completion_tokens: 20, cost: 0.00000178794 },
    });
    expect(costOf(body)).toBeCloseTo(0.00000178794, 12);
  });

  it("is zero rather than NaN when there is nothing to read", () => {
    // Anything unparseable is an unmetered request, which is a rounding error.
    // NaN would poison the shard it was added to and take the ceiling with it.
    for (const body of ["", "not json", "{}", '{"usage":null}', '{"usage":{"cost":"free"}}']) {
      expect(costOf(body)).toBe(0);
    }
  });
});

describe("sanitize", () => {
  const ok = (body: unknown) => {
    const out = sanitize(body, MODEL);
    if (typeof out === "string") throw new Error(`rejected: ${out}`);
    return out;
  };

  it("pins the model no matter what arrived", () => {
    // Absent is fine — it gets set.
    expect(ok({ messages: [{ role: "user", content: "hi" }] }).model).toBe(MODEL);
    // Naming the one model is fine.
    expect(ok({ model: MODEL, messages: [{ role: "user", content: "hi" }] }).model).toBe(MODEL);
    // Naming a different one is the whole attack, so it is refused outright
    // rather than quietly rewritten — a caller asking for Opus should be told
    // no, not billed for something else and left wondering.
    expect(
      sanitize({ model: "anthropic/claude-opus-4", messages: [{ role: "user", content: "hi" }] }, MODEL),
    ).toContain(MODEL);
  });

  it("drops the fields that would let a caller spend more", () => {
    const out = ok({
      messages: [{ role: "user", content: "hi" }],
      // Each of these changes what a request costs or where it runs.
      provider: { order: ["expensive-provider"] },
      models: ["anthropic/claude-opus-4"],
      route: "fallback",
      transforms: [],
      n: 50,
      user: "someone",
    });
    for (const gone of ["provider", "models", "route", "transforms", "n", "user"]) {
      expect(out).not.toHaveProperty(gone);
    }
    // And the things the app genuinely sends survive.
    expect(out).toHaveProperty("messages");
  });

  it("keeps what a real turn needs", () => {
    const out = ok({
      messages: [{ role: "user", content: "how was my week?" }],
      stream: true,
      stream_options: { include_usage: true },
      tools: [{ type: "function", function: { name: "recovery" } }],
      tool_choice: "auto",
      response_format: { type: "json_schema" },
      temperature: 0.7,
      reasoning: { enabled: false },
    });
    expect(out.stream).toBe(true);
    expect(out.tools).toHaveLength(1);
    expect(out.stream_options).toEqual({ include_usage: true });
    // Stripping this is why the app's small calls came back empty: reasoning is
    // drawn from max_tokens, so "don't think" has to survive the proxy or a
    // 200-token ceiling gets eaten before a single character is written.
    expect(out.reasoning).toEqual({ enabled: false });
  });

  it("clamps max_tokens rather than refusing it", () => {
    // A ceiling the caller didn't know about should shorten the answer, not
    // lose it.
    const msgs = [{ role: "user", content: "hi" }];
    expect(ok({ messages: msgs, max_tokens: 1_000_000 }).max_tokens).toBe(MAX_TOKENS);
    expect(ok({ messages: msgs, max_tokens: 200 }).max_tokens).toBe(200);
    // Absent or junk gets the ceiling, never Infinity and never NaN.
    expect(ok({ messages: msgs }).max_tokens).toBe(MAX_TOKENS);
    expect(ok({ messages: msgs, max_tokens: "lots" }).max_tokens).toBe(MAX_TOKENS);
    // Generous on purpose: reasoning tokens come out of this budget, and the
    // app's main turn sends no ceiling of its own for the proxy to respect.
    expect(MAX_TOKENS).toBeGreaterThanOrEqual(8192);
  });

  it("always asks upstream for the usage block", () => {
    // Without this the stream carries no cost and the daily ceiling is
    // unenforceable — so it is set here rather than trusted from the request.
    expect(ok({ messages: [{ role: "user", content: "hi" }], usage: false }).usage).toEqual({
      include: true,
    });
  });

  it("refuses a body that isn't a conversation", () => {
    expect(sanitize(null, MODEL)).toBeTypeOf("string");
    expect(sanitize("hello", MODEL)).toBeTypeOf("string");
    expect(sanitize({}, MODEL)).toBeTypeOf("string");
    expect(sanitize({ messages: [] }, MODEL)).toBeTypeOf("string");
    expect(sanitize({ messages: "hi" }, MODEL)).toBeTypeOf("string");
    // A conversation longer than any turn this app makes.
    const many = Array.from({ length: 500 }, () => ({ role: "user", content: "hi" }));
    expect(sanitize({ messages: many }, MODEL)).toBeTypeOf("string");
  });
});
