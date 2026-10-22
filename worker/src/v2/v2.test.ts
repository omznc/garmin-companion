import { afterEach, describe, expect, it, vi } from "vitest";
import { entitlementOf } from "./billing";
import { networkOf } from "./index";
import { harness, signed, sse, usageChunk } from "./harness";
import { costUnits, profileOf } from "./models";

afterEach(() => vi.unstubAllGlobals());

const SECRET = "polar_whs_test_secret";
const HOOK = "/v2/webhooks/polar";
const jsonResponse = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

/** A request body of exactly `bytes` bytes that is a valid chat request. */
function bodyOf(bytes: number): string {
  const head = '{"messages":[{"role":"user","content":"';
  const tail = '"}]}';
  return head + "a".repeat(bytes - head.length - tail.length) + tail;
}

describe("install", () => {
  it("issues an id that works, and refuses one it never issued", async () => {
    const h = harness();
    const id = await h.install();
    expect(id).toMatch(/^[0-9a-f]{32}$/);
    const b = await h.balance(id);
    expect(b).toMatchObject({ plan: "free", credits_remaining: 30, daily_grant: 30, email: null });
    expect(b.resets_at).toMatch(/T00:00:00/);

    const stranger = await h.call("/v2/balance", { token: "0".repeat(32) });
    expect(stranger.status).toBe(401);
    expect(((await stranger.json()) as any).error.type).toBe("unknown_install");
    expect((await h.call("/v2/balance")).status).toBe(401);
  });

  it("allows 10 ids a day for one address, then 429", async () => {
    const h = harness();
    for (let i = 0; i < 10; i++)
      expect((await h.call("/v2/install", { method: "POST" })).status).toBe(200);
    const r = await h.call("/v2/install", { method: "POST" });
    expect(r.status).toBe(429);
    expect(r.headers.get("Retry-After")).toBeTruthy();
  });

  it("answers 429 when the rate limiter says so", async () => {
    const h = harness({ ISSUE: { limit: async () => ({ success: false }) } });
    expect((await h.call("/v2/install", { method: "POST" })).status).toBe(429);
  });

  it("does not take /v1 install ids", async () => {
    const h = harness();
    h.kv.set("install:" + "a".repeat(32), "2026-10-19");
    expect((await h.call("/v2/balance", { token: "a".repeat(32) })).status).toBe(401);
  });
});

describe("chat", () => {
  it("streams, settles from token counts, and sets the credit headers", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => sse({ choices: [{ delta: { content: "hi" } }] }, usageChunk(1000, 200)));

    const r = await h.ask(id);
    expect(r.status).toBe(200);
    expect(r.headers.get("X-Credits-Remaining")).toBe("30");
    expect(r.headers.get("X-Credits-Reset")).toMatch(/^\d{4}-/);
    const text = await r.text();
    expect(text).toContain('"content":"hi"');
    await h.settled();

    // (800 x 0.10 + 200 x 0.50) per million is 0.0002 USD, 0.4 credit.
    expect((await h.balance(id)).credits_remaining).toBe(29);
    const sent = JSON.parse(h.fetchCalls[0].init.body as string);
    expect(sent).toMatchObject({
      model: "anthropic/claude-haiku-5.5",
      max_tokens: 4096,
      reasoning: { effort: "low" },
      usage: { include: true },
      stream_options: { include_usage: true },
    });
  });

  it("prices cached and cache-write tokens", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() =>
      sse({
        choices: [],
        usage: {
          prompt_tokens: 5000,
          completion_tokens: 0,
          prompt_tokens_details: { cached_tokens: 2000, cache_write_tokens: 1000 },
        },
      }),
    );
    await (await h.ask(id)).text();
    await h.settled();
    // 2000 fresh x 0.1 + 2000 read x 0.01 + 1000 write x 0.125 = 345 per million.
    const used = 30 - (await h.balance(id)).credits_remaining;
    expect(used).toBe(1); // 0.000345 USD is 0.69 credit, rounded up by the floor.
  });

  it("settles a non-streamed answer and returns the exact balance", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() =>
      jsonResponse({
        choices: [{ message: { content: "ok" } }],
        usage: { prompt_tokens: 100, completion_tokens: 10 },
      }),
    );
    const r = await h.ask(id, { stream: false });
    expect(r.status).toBe(200);
    expect(r.headers.get("X-Credits-Remaining")).toBe("29");
    expect(((await r.json()) as any).choices[0].message.content).toBe("ok");
  });

  describe("a stream with no data line", () => {
    const enc = new TextEncoder();
    afterEach(() => vi.useRealTimers());

    /** The upstream sends `parts` and stays open. Returns the way to end it. */
    function open(h: ReturnType<typeof harness>, ...parts: string[]) {
      let end = () => {};
      h.upstream(
        () =>
          new Response(
            new ReadableStream({
              start(c) {
                for (const part of parts) c.enqueue(enc.encode(part));
                end = () => c.close();
              },
            }),
            { headers: { "Content-Type": "text/event-stream" } },
          ),
      );
      return () => end();
    }

    async function left(parts: string[], afterMs: number, how: "end" | "cancel") {
      vi.useFakeTimers({ toFake: ["Date"] });
      const h = harness();
      const id = await h.install();
      const end = open(h, ...parts);
      const r = await h.ask(id);
      vi.setSystemTime(Date.now() + afterMs);
      if (how === "end") {
        end();
        await r.text();
      } else {
        await r.body?.cancel();
      }
      await h.settled();
      return (await h.balance(id)).credits_remaining;
    }

    const keepalive = [": OPENROUTER PROCESSING\n\n"];

    it("is free after 60 seconds, ended or cancelled", async () => {
      expect(await left(keepalive, 61_000, "end")).toBe(30);
      expect(await left([], 61_000, "cancel")).toBe(30);
    });

    it("is charged the estimate before 60 seconds", async () => {
      expect(await left(keepalive, 5_000, "end")).toBe(25);
      expect(await left([], 5_000, "cancel")).toBe(25);
    });

    it("still charges when the data hides in an odd format", async () => {
      const chunk = '{"choices":[{"delta":{"content":"a"}}]}';
      const late = 61_000;
      // CRLF separators.
      expect(await left([`data: ${chunk}\r\n\r\n`], late, "end")).toBe(25);
      // The last event has no blank line after it.
      expect(await left([`data: ${chunk}`], late, "end")).toBe(25);
      // The content holds the word [DONE].
      const done = '{"choices":[{"delta":{"content":"[DONE]"}}]}';
      expect(await left([`data: ${done}\n\n`], late, "end")).toBe(25);
      // The client leaves inside the first event.
      expect(await left(['data: {"choices":[{"del'], late, "cancel")).toBe(25);
    });
  });

  it("charges the estimate when the client leaves before the usage chunk", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(
      () =>
        new Response(
          new ReadableStream({
            start(c) {
              c.enqueue(
                new TextEncoder().encode('data: {"choices":[{"delta":{"content":"a"}}]}\n\n'),
              );
              // Never ends, so the usage chunk never comes.
            },
          }),
          { headers: { "Content-Type": "text/event-stream" } },
        ),
    );
    const r = await h.ask(id);
    const reader = r.body!.getReader();
    await reader.read();
    await reader.cancel();
    await h.settled();
    // The hold is about 4.1 credits and all of it is kept.
    expect((await h.balance(id)).credits_remaining).toBe(25);
  });

  it("charges the real cost when the client leaves after the usage chunk", async () => {
    const h = harness();
    const id = await h.install();
    const enc = new TextEncoder();
    h.upstream(
      () =>
        new Response(
          new ReadableStream({
            start(c) {
              c.enqueue(enc.encode('data: {"choices":[{"delta":{"content":"a"}}]}\n\n'));
              c.enqueue(enc.encode(`data: ${JSON.stringify(usageChunk(1000, 200))}\n\n`));
              // Never ends: the client leaves before the [DONE] line.
            },
          }),
          { headers: { "Content-Type": "text/event-stream" } },
        ),
    );
    const r = await h.ask(id);
    const reader = r.body!.getReader();
    await reader.read();
    await reader.read();
    await reader.cancel();
    await h.settled();
    // 0.4 credit, not the hold of about 4.1 credits.
    expect((await h.balance(id)).credits_remaining).toBe(29);
  });

  it("holds less for a smaller max_tokens", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => sse({ choices: [{ delta: { content: "x" } }] }));
    await (await h.ask(id, { max_tokens: 1500 })).text();
    await h.settled();
    // No usage block, so the whole hold is charged: 1500 tokens, about 1.5 credits.
    expect((await h.balance(id)).credits_remaining).toBe(28);
    const sent = JSON.parse(h.fetchCalls[0].init.body as string);
    expect(sent.max_tokens).toBe(1500);
  });

  it("holds a prompt at about 3.5 bytes a token, not one token a byte", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => sse({ choices: [{ delta: { content: "x" } }] }));
    const big = { messages: [{ role: "user", content: "a".repeat(90_000) }], max_tokens: 100 };
    await (await h.ask(id, big)).text();
    await h.settled();
    // 90,000 bytes: 25,700 tokens x 0.1 + 100 x 0.5 per million is 0.0026 USD, 5.2 credits.
    // At one token a byte it would be 18 credits.
    const used = 30 - (await h.balance(id)).credits_remaining;
    expect(used).toBeGreaterThanOrEqual(5);
    expect(used).toBeLessThanOrEqual(6);
  });

  it("releases the hold and answers 503 when the OpenRouter key limit is reached", async () => {
    const h = harness();
    const id = await h.install();
    for (const status of [402, 403]) {
      h.upstream(() => jsonResponse({ error: { message: "Key limit exceeded" } }, status));
      const r = await h.ask(id);
      expect(r.status).toBe(503);
      expect(((await r.json()) as any).error.type).toBe("coach_paused");
    }
    expect((await h.balance(id)).credits_remaining).toBe(30);
  });

  it("does not call a 403 refusal a pause", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => jsonResponse({ error: { message: "content policy violation" } }, 403));
    const r = await h.ask(id);
    expect(r.status).toBe(502);
    expect((await h.balance(id)).credits_remaining).toBe(30);
  });

  it("releases the hold when the provider fails inside a 200 stream", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => sse({ error: { message: "provider down", code: 502 } }));
    await (await h.ask(id)).text();
    await h.settled();
    expect((await h.balance(id)).credits_remaining).toBe(30);
    expect((await h.call("/v2/balance", { token: id })).status).toBe(200);
  });

  it("answers 502 and releases the hold on a 200 JSON error body", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => jsonResponse({ error: { message: "provider down" } }));
    const r = await h.ask(id, { stream: false });
    expect(r.status).toBe(502);
    expect(((await r.json()) as any).error.type).toBe("upstream_error");
    expect((await h.balance(id)).credits_remaining).toBe(30);
  });

  it("releases the hold on another upstream error", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => jsonResponse({ error: "boom" }, 500));
    const r = await h.ask(id);
    expect(r.status).toBe(502);
    expect(((await r.json()) as any).error.type).toBe("upstream_error");
    expect((await h.balance(id)).credits_remaining).toBe(30);
    h.upstream(() => {
      throw new Error("network");
    });
    expect((await h.ask(id)).status).toBe(502);
    expect((await h.balance(id)).credits_remaining).toBe(30);
  });

  it("answers 402 out_of_credits with the reset time once the day is spent", async () => {
    const h = harness();
    const id = await h.install();
    // No usage block: each request costs its whole hold, about 4 credits.
    h.upstream(() => sse({ choices: [{ delta: { content: "x" } }] }));
    let status = 200;
    let tries = 0;
    while (status === 200 && tries < 12) {
      const r = await h.ask(id);
      status = r.status;
      if (status === 200) await r.text();
      await h.settled();
      tries++;
    }
    expect(status).toBe(402);
    expect(tries).toBeLessThanOrEqual(9);
    const r = await h.ask(id);
    const err = ((await r.json()) as any).error;
    expect(err).toMatchObject({ type: "out_of_credits" });
    expect(err.resets_at).toMatch(/T00:00:00/);
  });

  it("does not let parallel requests spend the same credits", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => sse({ choices: [{ delta: { content: "x" } }] }));
    const all = await Promise.all(Array.from({ length: 20 }, () => h.ask(id)));
    const ok = all.filter((r) => r.status === 200).length;
    expect(ok).toBeGreaterThan(0);
    expect(ok).toBeLessThanOrEqual(7); // 30 credits over a hold of about 4.1
    expect(all.filter((r) => r.status === 402)).toHaveLength(20 - ok);
  });

  it("refuses a repeated X-Request-Id and charges once", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => sse({ choices: [] }, usageChunk(1000, 200)));
    const headers = { "X-Request-Id": "req-12345678" };
    await (await h.ask(id, {}, headers)).text();
    await h.settled();
    const again = await h.ask(id, {}, headers);
    expect(again.status).toBe(409);
    expect(((await again.json()) as any).error.type).toBe("duplicate_request");
    expect(h.fetchCalls).toHaveLength(1);
  });

  it("accepts a body of exactly 100,000 bytes and refuses 100,001", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => sse({ choices: [] }, usageChunk(1000, 10)));
    const ok = await h.call("/v2/chat/completions", { token: id, body: bodyOf(100_000) });
    // The hold for a body this size is about 28 credits, which fits in 30.
    expect(ok.status).toBe(200);
    await ok.text();
    const big = await h.call("/v2/chat/completions", { token: id, body: bodyOf(100_001) });
    expect(big.status).toBe(413);
    expect(((await big.json()) as any).error.type).toBe("context_too_large");
    // Multi-byte text counts in bytes, not characters.
    const wide = JSON.stringify({ messages: [{ role: "user", content: "é".repeat(50_001) }] });
    expect((await h.call("/v2/chat/completions", { token: id, body: wide })).status).toBe(413);
  });

  it("refuses a declared length over the cap without reading the body", async () => {
    const h = harness();
    const id = await h.install();
    const r = await h.call("/v2/chat/completions", {
      token: id,
      headers: { "Content-Length": "100001" },
      body: "{}",
    });
    expect(r.status).toBe(413);
  });

  it("uses the DeepSeek cap and prices when the profile is swapped", async () => {
    const h = harness({ MODEL_PROFILE: "deepseek" });
    const id = await h.install();
    h.upstream(() => sse({ choices: [] }, usageChunk(1000, 10)));
    const r = await h.call("/v2/chat/completions", { token: id, body: bodyOf(150_000) });
    expect(r.status).toBe(200);
    await r.text();
    const sent = JSON.parse(h.fetchCalls[0].init.body as string);
    expect(sent.model).toBe("deepseek/deepseek-v4.1-flash-1017");
    expect(sent.models).toBeUndefined();
    expect(sent.provider).toMatchObject({ sort: "price" });
    expect(sent.reasoning).toEqual({ effort: "low" });
    // The bill uses the profile prices, so the cap must be the same prices.
    const p = profileOf("deepseek");
    expect(sent.provider.max_price).toEqual({ prompt: p.input, completion: p.output });
  });

  it("charges nothing when the free model answered", () => {
    const p = { ...profileOf("deepseek"), free: "minimax/minimax-m3.1-flash:free" };
    const usage = { prompt_tokens: 100_000, completion_tokens: 8000 };
    expect(costUnits(p, usage, "minimax/minimax-m3.1-flash:free")).toBe(0);
    expect(costUnits(p, usage, "minimax/minimax-m3.1-flash")).toBe(0);
    expect(costUnits(p, usage, "deepseek/deepseek-v4.1-flash-1017")).toBeGreaterThan(0);
  });

  it("rejects bad bodies and another model", async () => {
    const h = harness();
    const id = await h.install();
    expect((await h.call("/v2/chat/completions", { token: id, body: "nope" })).status).toBe(400);
    expect((await h.ask(id, { messages: [] })).status).toBe(400);
    expect((await h.ask(id, { model: "openai/gpt-5" })).status).toBe(400);
    expect(h.fetchCalls).toHaveLength(0);
  });

  it("refuses server tools, image parts and extra reasoning fields", async () => {
    const h = harness();
    const id = await h.install();
    const bad: Record<string, unknown>[] = [
      { tools: [{ type: "openrouter:web_search" }] },
      { tools: [{ type: "function", function: { name: "a" } }, { type: "openrouter:advisor" }] },
      { tools: "web_search" },
      { tool_choice: { type: "openrouter:web_search" } },
      { reasoning: { effort: "high", max_tokens: 50_000 } },
      { reasoning: "high" },
      {
        messages: [
          {
            role: "user",
            content: [{ type: "image_url", image_url: { url: "https://example.com/x.png" } }],
          },
        ],
      },
      { messages: [{ role: "user", content: { type: "text" } }] },
      { messages: ["hi"] },
    ];
    for (const extra of bad) {
      const r = await h.ask(id, extra);
      expect(r.status, JSON.stringify(extra)).toBe(400);
      expect(((await r.json()) as any).error.type).toBe("bad_request");
    }
    expect(h.fetchCalls).toHaveLength(0);
    expect((await h.balance(id)).credits_remaining).toBe(30);
  });

  it("accepts function tools, text parts and an effort", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => sse({ choices: [] }, usageChunk(10, 10)));
    const r = await h.ask(id, {
      tools: [{ type: "function", function: { name: "recovery", parameters: {} } }],
      tool_choice: "none",
      reasoning: { effort: "low" },
      messages: [
        {
          role: "system",
          content: [{ type: "text", text: "s", cache_control: { type: "ephemeral" } }],
        },
        { role: "user", content: "hi" },
        { role: "assistant", content: null, tool_calls: [] },
        { role: "tool", tool_call_id: "c1", content: "{}" },
      ],
    });
    expect(r.status).toBe(200);
    await r.text();
  });

  it("drops fields that are not on the allowlist", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(() => sse({ choices: [] }, usageChunk(10, 10)));
    await (
      await h.ask(id, { provider: { order: ["x"] }, models: ["y"], max_tokens: 99_999 })
    ).text();
    const sent = JSON.parse(h.fetchCalls[0].init.body as string);
    expect(sent.provider).toBeUndefined();
    expect(sent.models).toBeUndefined();
    expect(sent.max_tokens).toBe(4096);
  });

  it("stops at the v2 daily budget", async () => {
    const h = harness({ DAILY_BUDGET_V2_USD: "0.0001" });
    const id = await h.install();
    h.upstream(() => sse({ choices: [] }, usageChunk(10_000, 1000)));
    await (await h.ask(id)).text();
    await h.settled();
    const r = await h.ask(id);
    expect(r.status).toBe(503);
    expect(((await r.json()) as any).error.type).toBe("coach_paused");
  });

  it("does not let a spent free budget pause a premium user", async () => {
    const h = harness({ DAILY_BUDGET_V2_USD: "0.0001" });
    const id = await h.install();
    h.upstream(() => sse({ choices: [] }, usageChunk(10_000, 1000)));
    await (await h.ask(id)).text();
    await h.settled();
    expect((await h.ask(id)).status).toBe(503);

    const { token, userId } = await h.login();
    await post(h, sub(userId));
    h.upstream(() => sse({ choices: [] }, usageChunk(10, 10)));
    const r = await h.ask(token);
    expect(r.status).toBe(200);
    await r.text();
  });

  it("has an optional premium budget of its own", async () => {
    const h = harness({ DAILY_BUDGET_V2_PREMIUM_USD: "0.0001" });
    const { token, userId } = await h.login();
    await post(h, sub(userId));
    h.upstream(() => sse({ choices: [] }, usageChunk(10_000, 1000)));
    await (await h.ask(token)).text();
    await h.settled();
    expect((await h.ask(token)).status).toBe(503);
    // The free pool is not affected.
    h.upstream(() => sse({ choices: [] }, usageChunk(10, 10)));
    expect((await h.ask(await h.install())).status).toBe(200);
  });

  it("answers 502 and releases the hold when a JSON body breaks off", async () => {
    const h = harness();
    const id = await h.install();
    h.upstream(
      () =>
        new Response(
          new ReadableStream({
            start(c) {
              c.error(new Error("cut"));
            },
          }),
          { headers: { "Content-Type": "application/json" } },
        ),
    );
    const r = await h.ask(id, { stream: false });
    expect(r.status).toBe(502);
    expect((await h.balance(id)).credits_remaining).toBe(30);
  });

  it("refuses a blocked install and, with the switch on, anonymous installs", async () => {
    const h = harness();
    const id = await h.install();
    h.kv.set(`v2block:${id}`, "1");
    expect((await h.ask(id)).status).toBe(403);

    const g = harness({ FREE_REQUIRES_LOGIN: "true" });
    const other = await g.install();
    const r = await g.ask(other);
    expect(r.status).toBe(401);
    expect(((await r.json()) as any).error.type).toBe("login_required");
    // A signed-in user still gets the free grant.
    const { token } = await g.login();
    expect((await g.balance(token)).credits_remaining).toBe(30);
  });

  it("answers 429 when the burst limit is hit", async () => {
    const h = harness({ BURST: { limit: async () => ({ success: false }) } });
    const id = await h.install();
    expect((await h.ask(id)).status).toBe(429);
  });
});

describe("login", () => {
  it("sends a code, verifies it and returns a session that works", async () => {
    const h = harness();
    expect(
      (await h.call("/v2/auth/email/send", { json: { email: " A@Example.com " } })).status,
    ).toBe(200);
    expect(h.mails).toHaveLength(1);
    expect(h.mails[0].to).toBe("a@example.com");
    expect(h.mails[0].code).toMatch(/^\d{6}$/);

    const r = await h.call("/v2/auth/email/verify", {
      json: { email: "a@example.com", code: h.mails[0].code },
    });
    expect(r.status).toBe(200);
    const { token, email } = (await r.json()) as any;
    expect(token).toMatch(/^s_[0-9a-f]{64}$/);
    expect(email).toBe("a@example.com");

    const b = await h.balance(token);
    expect(b).toMatchObject({ email: "a@example.com", plan: "free", premium: false });
    // The stored value is a hash, never the token.
    const rows = h.db.raw.prepare("SELECT token_hash FROM session").all() as any[];
    expect(rows[0].token_hash).not.toBe(token);

    // A used code does not work twice.
    const again = await h.call("/v2/auth/email/verify", {
      json: { email: "a@example.com", code: h.mails[0].code },
    });
    expect(again.status).toBe(401);
  });

  it("rejects a bad email and a wrong code", async () => {
    const h = harness();
    expect((await h.call("/v2/auth/email/send", { json: { email: "nope" } })).status).toBe(400);
    await h.call("/v2/auth/email/send", { json: { email: "a@example.com" } });
    const wrong = String((Number(h.mails[0].code) + 1) % 1_000_000).padStart(6, "0");
    const r = await h.call("/v2/auth/email/verify", {
      json: { email: "a@example.com", code: wrong },
    });
    expect(r.status).toBe(401);
    expect(((await r.json()) as any).error.type).toBe("invalid_code");
    expect(
      (await h.call("/v2/auth/email/verify", { json: { email: "a@example.com", code: "12" } }))
        .status,
    ).toBe(400);
  });

  it("locks a code after 5 wrong tries, even for the right code", async () => {
    const h = harness();
    await h.call("/v2/auth/email/send", { json: { email: "a@example.com" } });
    const right = h.mails[0].code;
    const wrong = String((Number(right) + 1) % 1_000_000).padStart(6, "0");
    for (let i = 0; i < 5; i++) {
      await h.call("/v2/auth/email/verify", { json: { email: "a@example.com", code: wrong } });
    }
    const r = await h.call("/v2/auth/email/verify", {
      json: { email: "a@example.com", code: right },
    });
    expect(r.status).toBe(401);
  });

  it("lets any live code sign in, so a flood of sends cannot lock the owner out", async () => {
    const h = harness();
    for (let i = 0; i < 3; i++) {
      await h.call("/v2/auth/email/send", { json: { email: "a@example.com" } });
    }
    // The owner uses the first mail, not the newest.
    const r = await h.call("/v2/auth/email/verify", {
      json: { email: "a@example.com", code: h.mails[0].code },
    });
    expect(r.status).toBe(200);
    // A used code does not work twice, and no other code works after the sign-in.
    const again = await h.call("/v2/auth/email/verify", {
      json: { email: "a@example.com", code: h.mails[2].code },
    });
    expect(again.status).toBe(401);
  });

  it("counts wrong tries against each code, so guessing stays capped", async () => {
    const h = harness();
    for (let i = 0; i < 3; i++) {
      await h.call("/v2/auth/email/send", { json: { email: "a@example.com" } });
    }
    const right = h.mails[1].code;
    const wrong = [right, h.mails[0].code, h.mails[2].code].every((c) => c !== "000000")
      ? "000000"
      : "000001";
    for (let i = 0; i < 5; i++) {
      await h.call("/v2/auth/email/verify", { json: { email: "a@example.com", code: wrong } });
    }
    const attempts = h.db.raw.prepare("SELECT attempts FROM login_code").all() as any[];
    expect(attempts.every((a) => a.attempts === 5)).toBe(true);
    const r = await h.call("/v2/auth/email/verify", {
      json: { email: "a@example.com", code: right },
    });
    expect(r.status).toBe(401);
  });

  it("limits codes per email and per address", async () => {
    const h = harness();
    for (let i = 0; i < 5; i++) {
      expect(
        (await h.call("/v2/auth/email/send", { json: { email: "a@example.com" } })).status,
      ).toBe(200);
    }
    expect((await h.call("/v2/auth/email/send", { json: { email: "a@example.com" } })).status).toBe(
      429,
    );
    const g = harness({ ISSUE: { limit: async () => ({ success: false }) } });
    expect((await g.call("/v2/auth/email/send", { json: { email: "b@example.com" } })).status).toBe(
      429,
    );
  });

  it("caps new accounts per address, but not logins of existing ones", async () => {
    const h = harness();
    for (let i = 0; i < 5; i++) await h.login(`user${i}@example.com`);
    const r = await h.call("/v2/auth/email/send", { json: { email: "user5@example.com" } });
    expect(r.status).toBe(429);
    expect(((await r.json()) as any).error.type).toBe("install_limit");
    expect(
      (await h.call("/v2/auth/email/send", { json: { email: "user0@example.com" } })).status,
    ).toBe(200);
  });

  it("limits verify tries per address", async () => {
    const h = harness({ ISSUE: { limit: async () => ({ success: false }) } });
    const r = await h.call("/v2/auth/email/verify", {
      json: { email: "a@example.com", code: "123456" },
    });
    expect(r.status).toBe(429);
  });

  it("says so when no mail sender is set up", async () => {
    const h = harness({ MAILER: undefined });
    const r = await h.call("/v2/auth/email/send", { json: { email: "a@example.com" } });
    expect(r.status).toBe(503);
    expect(((await r.json()) as any).error.type).toBe("mail_unavailable");
  });

  it("sends through Resend when the key and sender are set", async () => {
    const h = harness({
      MAILER: undefined,
      RESEND_KEY: "re_test",
      MAIL_FROM: "Coach <login@example.com>",
    });
    h.upstream(() => jsonResponse({ id: "1" }));
    expect((await h.call("/v2/auth/email/send", { json: { email: "a@example.com" } })).status).toBe(
      200,
    );
    const call = h.fetchCalls[0];
    expect(call.url).toBe("https://api.resend.com/emails");
    expect(JSON.parse(call.init.body as string)).toMatchObject({
      to: "a@example.com",
      from: "Coach <login@example.com>",
    });
    h.upstream(() => jsonResponse({}, 422));
    expect((await h.call("/v2/auth/email/send", { json: { email: "c@example.com" } })).status).toBe(
      502,
    );
  });

  it("signs out, expires sessions and keeps only 3 sessions", async () => {
    const h = harness();
    const first = await h.login();
    const second = await h.login();
    await h.login();
    await h.login();
    // The oldest session is gone. Three remain.
    expect((await h.call("/v2/balance", { token: first.token })).status).toBe(401);
    expect((await h.call("/v2/balance", { token: second.token })).status).toBe(200);

    const out = await h.call("/v2/auth/signout", { method: "POST", token: second.token });
    expect(out.status).toBe(200);
    const after = await h.call("/v2/balance", { token: second.token });
    expect(after.status).toBe(401);
    expect(((await after.json()) as any).error.type).toBe("session_expired");

    const third = await h.login("z@example.com");
    h.db.raw.prepare("UPDATE session SET expires_at = 1 WHERE user_id = ?").run(third.userId);
    expect((await h.call("/v2/balance", { token: third.token })).status).toBe(401);
  });

  it("needs a login for signout and billing", async () => {
    const h = harness();
    const id = await h.install();
    for (const path of ["/v2/auth/signout", "/v2/billing/checkout", "/v2/billing/portal"]) {
      const r = await h.call(path, { method: "POST", token: id });
      expect(r.status).toBe(401);
      expect(((await r.json()) as any).error.type).toBe("login_required");
    }
  });
});

describe("billing", () => {
  it("creates a Polar checkout for the signed-in user", async () => {
    const h = harness({ CHECKOUT_RETURN_URL: "https://example.com/thanks" });
    const { token, userId } = await h.login();
    h.upstream(() => jsonResponse({ url: "https://sandbox.polar.sh/checkout/abc" }, 201));
    const r = await h.call("/v2/billing/checkout", { method: "POST", token });
    expect(r.status).toBe(200);
    expect(await r.json()).toEqual({ url: "https://sandbox.polar.sh/checkout/abc" });
    const call = h.fetchCalls[0];
    expect(call.url).toBe("https://sandbox-api.polar.sh/v1/checkouts/");
    expect(new Headers(call.init.headers).get("Authorization")).toBe("Bearer polar-test-token");
    expect(JSON.parse(call.init.body as string)).toMatchObject({
      products: ["prod_test"],
      external_customer_id: userId,
      customer_email: "a@example.com",
      success_url: "https://example.com/thanks",
    });
  });

  it("maps Polar failures and missing setup", async () => {
    const h = harness();
    const { token } = await h.login();
    h.upstream(() => jsonResponse({ detail: "no" }, 500));
    expect((await h.call("/v2/billing/checkout", { method: "POST", token })).status).toBe(502);
    const g = harness({ POLAR_TOKEN: undefined });
    const s = await g.login();
    expect((await g.call("/v2/billing/checkout", { method: "POST", token: s.token })).status).toBe(
      503,
    );
  });

  it("gives the customer portal link, and 404 for someone who never bought", async () => {
    const h = harness();
    const { token, userId } = await h.login();
    h.upstream(() =>
      jsonResponse({ customer_portal_url: "https://sandbox.polar.sh/portal/x" }, 201),
    );
    const r = await h.call("/v2/billing/portal", { method: "POST", token });
    expect(await r.json()).toEqual({ url: "https://sandbox.polar.sh/portal/x" });
    expect(JSON.parse(h.fetchCalls[0].init.body as string)).toEqual({
      external_customer_id: userId,
    });
    h.upstream(() => jsonResponse({ detail: "none" }, 404));
    expect((await h.call("/v2/billing/portal", { method: "POST", token })).status).toBe(404);
  });
});

/* ---------------------------------------------------------------- webhooks --- */

const sub = (
  userId: string,
  over: Record<string, unknown> = {},
  type = "subscription.created",
  at = new Date(),
) => ({
  type,
  timestamp: at.toISOString(),
  data: {
    id: "sub_1",
    status: "active",
    current_period_end: new Date(Date.now() + 30 * 86_400_000).toISOString(),
    cancel_at_period_end: false,
    customer: { external_id: userId },
    ...over,
  },
});

async function post(
  h: ReturnType<typeof harness>,
  event: unknown,
  opts: Parameters<typeof signed>[2] = {},
) {
  const s = await signed(SECRET, event, opts);
  return h.call(HOOK, { method: "POST", body: s.body, headers: s.headers });
}

const rows = (h: ReturnType<typeof harness>) =>
  h.db.raw.prepare("SELECT * FROM entitlement").all() as any[];

describe("polar webhook", () => {
  it("rejects a bad signature, a wrong secret, a stale timestamp and missing headers, with no write", async () => {
    const h = harness();
    const { userId } = await h.login();
    const ev = sub(userId);
    expect((await post(h, ev, { badSig: true })).status).toBe(400);
    const wrongKey = await signed("another-secret", ev);
    expect(
      (await h.call(HOOK, { method: "POST", body: wrongKey.body, headers: wrongKey.headers }))
        .status,
    ).toBe(400);
    expect((await post(h, ev, { ts: Math.floor(Date.now() / 1000) - 3600 })).status).toBe(400);
    expect((await h.call(HOOK, { method: "POST", body: JSON.stringify(ev) })).status).toBe(400);
    // A body changed after signing.
    const s = await signed(SECRET, ev);
    expect(
      (await h.call(HOOK, { method: "POST", body: s.body + " ", headers: s.headers })).status,
    ).toBe(400);
    expect(rows(h)).toHaveLength(0);
    expect(h.db.raw.prepare("SELECT * FROM webhook_event").all()).toHaveLength(0);
  });

  it("accepts an older Polar secret, the UTF-8 bytes of the whole string", async () => {
    const secret = "whsec_oldstyle-secret";
    const h = harness({ POLAR_WEBHOOK_SECRET: secret });
    const { userId } = await h.login();
    const s = await signed(secret, sub(userId));
    expect((await h.call(HOOK, { method: "POST", body: s.body, headers: s.headers })).status).toBe(
      200,
    );
  });

  it("accepts a whsec_ base64 secret", async () => {
    const raw = "super-secret-bytes";
    const h = harness({ POLAR_WEBHOOK_SECRET: `whsec_${btoa(raw)}` });
    const { userId } = await h.login();
    const s = await signed(raw, sub(userId));
    expect((await h.call(HOOK, { method: "POST", body: s.body, headers: s.headers })).status).toBe(
      200,
    );
  });

  it("refuses everything when no secret is set", async () => {
    const h = harness({ POLAR_WEBHOOK_SECRET: undefined });
    const s = await signed(SECRET, {});
    expect((await h.call(HOOK, { method: "POST", body: s.body, headers: s.headers })).status).toBe(
      503,
    );
  });

  it("makes the user premium on a created event", async () => {
    const h = harness();
    const { token, userId } = await h.login();
    expect((await post(h, sub(userId))).status).toBe(200);
    const b = await h.balance(token);
    expect(b).toMatchObject({ plan: "premium", premium: true, source: "polar" });
    expect(b.credits_remaining).toBe(430); // 400 premium a day and 30 free
    expect(b.renews_at).toBeTruthy();
  });

  it("spends from the premium pool when premium", async () => {
    const h = harness();
    const { token, userId } = await h.login();
    await post(h, sub(userId));
    h.upstream(() => sse({ choices: [] }, usageChunk(3000, 500)));
    await (await h.ask(token)).text();
    await h.settled();
    const b = await h.balance(token);
    // 3000 x 0.1 + 500 x 0.5 = 0.00055 USD, 1.1 credits, from the premium pool.
    expect(b.credits_remaining).toBe(428);
    expect(b.month_used).toBe(1);
  });

  it("applies a replayed event once and answers 200 both times", async () => {
    const h = harness();
    const { userId } = await h.login();
    const ev = sub(userId);
    expect((await post(h, ev, { id: "evt_same" })).status).toBe(200);
    const again = await post(h, ev, { id: "evt_same" });
    expect(again.status).toBe(200);
    expect(((await again.json()) as any).duplicate).toBe(true);
    expect(rows(h)).toHaveLength(1);
    expect(h.db.raw.prepare("SELECT * FROM webhook_event").all()).toHaveLength(1);
  });

  it("ignores an older event that arrives late", async () => {
    const h = harness();
    const { userId } = await h.login();
    const t = Date.now();
    await post(
      h,
      sub(
        userId,
        { status: "canceled", current_period_end: new Date(t - 1000).toISOString() },
        "subscription.updated",
        new Date(t),
      ),
    );
    // "created" is older than "updated" but arrives after it.
    await post(h, sub(userId, { status: "active" }, "subscription.created", new Date(t - 60_000)));
    expect(rows(h)[0].status).toBe("canceled");
    expect((await entitlementOf(h.env as any, userId)).premium).toBe(false);
  });

  it("keeps premium after a cancel at period end, until the revoke", async () => {
    const h = harness();
    const { token, userId } = await h.login();
    const t = Date.now();
    // What Polar sends: still active, flagged to end at the period end.
    await post(
      h,
      sub(userId, { cancel_at_period_end: true }, "subscription.canceled", new Date(t - 2000)),
    );
    const b = await h.balance(token);
    expect(b).toMatchObject({ premium: true, cancels_at_period_end: true });
    await post(h, sub(userId, { status: "canceled" }, "subscription.revoked", new Date(t)));
    expect((await h.balance(token)).premium).toBe(false);
  });

  it("ends premium on an immediate cancel even if the period end is ahead", async () => {
    const h = harness();
    const { userId } = await h.login();
    const t = Date.now();
    const at = new Date(t);
    await post(h, sub(userId, { status: "canceled" }, "subscription.revoked", at));
    // Same timestamp, other order: still not premium.
    await post(h, sub(userId, { status: "canceled" }, "subscription.canceled", at));
    expect((await entitlementOf(h.env as any, userId)).premium).toBe(false);
  });

  it("gives 7 days of grace when past due, and no more", async () => {
    const h = harness();
    const { userId } = await h.login();
    const t = Date.now();
    await post(h, sub(userId, { status: "past_due" }, "subscription.updated", new Date(t)));
    // A second past_due event must not restart the grace.
    await post(
      h,
      sub(userId, { status: "past_due" }, "subscription.updated", new Date(t + 86_400_000)),
    );
    const env = h.env as any;
    expect((await entitlementOf(env, userId, t + 6 * 86_400_000)).premium).toBe(true);
    expect((await entitlementOf(env, userId, t + 7 * 86_400_000 + 1000)).premium).toBe(false);
  });

  it("ends premium at once on a revoke and on a full refund, not on a partial one", async () => {
    const h = harness();
    const { token, userId } = await h.login();
    await post(h, sub(userId, {}, "subscription.created", new Date(Date.now() - 5000)));
    await post(h, {
      type: "order.refunded",
      timestamp: new Date(Date.now() - 4000).toISOString(),
      data: { status: "partially_refunded", subscription_id: "sub_1" },
    });
    expect((await h.balance(token)).premium).toBe(true);
    await post(h, {
      type: "order.refunded",
      timestamp: new Date().toISOString(),
      data: { status: "refunded", subscription_id: "sub_1" },
    });
    expect((await h.balance(token)).premium).toBe(false);

    const g = harness();
    const u = await g.login();
    await post(g, sub(u.userId, {}, "subscription.created", new Date(Date.now() - 5000)));
    await post(g, sub(u.userId, { status: "canceled" }, "subscription.revoked"));
    expect((await g.balance(u.token)).premium).toBe(false);
  });

  it("answers 200 and makes nobody premium for an unknown customer", async () => {
    const h = harness();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const { token } = await h.login();
    const r = await post(h, sub("not-a-user"));
    expect(r.status).toBe(200);
    expect(((await r.json()) as any).result).toBe("unknown_customer");
    expect(rows(h)).toHaveLength(0);
    expect((await h.balance(token)).premium).toBe(false);
    warn.mockRestore();
  });

  it("answers 500 and forgets the event id when applying fails, so the retry works", async () => {
    const h = harness();
    const { userId } = await h.login();
    const err = vi.spyOn(console, "error").mockImplementation(() => {});
    h.db.raw.exec("ALTER TABLE entitlement RENAME TO entitlement_off");
    const ev = sub(userId);
    expect((await post(h, ev, { id: "evt_retry" })).status).toBe(500);
    h.db.raw.exec("ALTER TABLE entitlement_off RENAME TO entitlement");
    expect((await post(h, ev, { id: "evt_retry" })).status).toBe(200);
    expect(rows(h)).toHaveLength(1);
    err.mockRestore();
  });

  it("records the event id only after the event is applied", async () => {
    const h = harness();
    const { userId } = await h.login();
    const err = vi.spyOn(console, "error").mockImplementation(() => {});
    h.db.raw.exec("ALTER TABLE entitlement RENAME TO entitlement_off");
    expect((await post(h, sub(userId), { id: "evt_x" })).status).toBe(500);
    expect(h.db.raw.prepare("SELECT * FROM webhook_event").all()).toHaveLength(0);
    h.db.raw.exec("ALTER TABLE entitlement_off RENAME TO entitlement");
    expect((await post(h, sub(userId), { id: "evt_x" })).status).toBe(200);
    expect(h.db.raw.prepare("SELECT * FROM webhook_event").all()).toHaveLength(1);
    err.mockRestore();
  });

  it("ends premium after the period end plus 2 days when no revoke arrives", async () => {
    const h = harness();
    const { userId } = await h.login();
    const end = Date.now() + 86_400_000;
    await post(h, sub(userId, { current_period_end: new Date(end).toISOString() }));
    const env = h.env as any;
    expect((await entitlementOf(env, userId, end + 86_400_000)).premium).toBe(true);
    expect((await entitlementOf(env, userId, end + 2 * 86_400_000 + 1000)).premium).toBe(false);
  });

  it("ignores event types it does not use", async () => {
    const h = harness();
    const r = await post(h, {
      type: "checkout.created",
      timestamp: new Date().toISOString(),
      data: {},
    });
    expect(r.status).toBe(200);
    expect(rows(h)).toHaveLength(0);
  });

  it("refuses checkout when already premium", async () => {
    const h = harness();
    const { token, userId } = await h.login();
    await post(h, sub(userId));
    const r = await h.call("/v2/billing/checkout", { method: "POST", token });
    expect(r.status).toBe(409);
  });
});

describe("network", () => {
  it("names an IPv6 address by its /64 and leaves IPv4 alone", () => {
    expect(networkOf("203.0.113.9")).toBe("203.0.113.9");
    expect(networkOf("2001:db8:1:2:aaaa:bbbb:cccc:dddd")).toBe("2001:0db8:0001:0002");
    expect(networkOf("2001:db8:1:2::5")).toBe("2001:0db8:0001:0002");
    expect(networkOf("2001:DB8::1")).toBe("2001:0db8:0000:0000");
    expect(networkOf("::1")).toBe("0000:0000:0000:0000");
  });

  it("counts two IPv6 hosts of one /64 as one network", async () => {
    const h = harness();
    const from = (ip: string) =>
      h.call("/v2/install", { method: "POST", headers: { "CF-Connecting-IP": ip } });
    for (let i = 0; i < 10; i++) {
      expect((await from(`2001:db8:1:2::${i + 1}`)).status).toBe(200);
    }
    expect((await from("2001:db8:1:2:ffff::9")).status).toBe(429);
  });
});

describe("checkout return page", () => {
  it("is public, static and locked down", async () => {
    const h = harness();
    const r = await h.call("/v2/checkout/done");
    expect(r.status).toBe(200);
    expect(r.headers.get("Content-Type")).toBe("text/html; charset=utf-8");
    expect(r.headers.get("Content-Security-Policy")).toBe(
      "default-src 'none'; style-src 'unsafe-inline'",
    );
    expect(r.headers.get("X-Content-Type-Options")).toBe("nosniff");
    expect(r.headers.get("Cache-Control")).toBe("public, max-age=3600");
    const html = await r.text();
    expect(html).toContain("<title>Payment done</title>");
    expect(html).not.toContain("<script");
    expect((await h.call("/v2/checkout/done", { method: "POST" })).status).toBe(404);
  });
});
