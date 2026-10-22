import { afterEach, describe, expect, it, vi } from "vitest";
import worker from "../index";
import { entitlementOf } from "./billing";
import { harness, signed, sse, usageChunk } from "./harness";
import { type LimitStore, LimitsCore } from "./limits";
import { costUnits, profileOf, UNITS_PER_USD } from "./models";

afterEach(() => vi.restoreAllMocks());

const DAY = 86_400_000;
const NOW = Date.parse("2026-10-19T12:00:00Z");

type Harness = ReturnType<typeof harness>;

function core() {
  let state: any;
  const store: LimitStore = {
    get: async () => structuredClone(state),
    put: async (s) => void (state = structuredClone(s)),
  };
  return new LimitsCore(store);
}

/** The value of a counter in the Limits object of the harness. */
async function counter(h: Harness, key: string): Promise<number> {
  const env = h.env as any;
  const r = await env.LIMITS.get("daily").fetch("https://object/", {
    method: "POST",
    body: JSON.stringify({ op: "get", key }),
  });
  return ((await r.json()) as { value: number }).value;
}

describe("limits core", () => {
  it("lets exactly the cap through when calls run in parallel", async () => {
    const c = core();
    const out = await Promise.all(Array.from({ length: 20 }, () => c.take([["a", 3]], true, NOW)));
    expect(out.filter((r) => r.ok)).toHaveLength(3);
  });

  it("counts nothing when one of two counters is full", async () => {
    const c = core();
    await c.take([["total", 1]], true, NOW);
    const r = await c.take(
      [
        ["mine", 5],
        ["total", 1],
      ],
      true,
      NOW,
    );
    expect(r).toEqual({ ok: false, key: "total" });
    expect(await c.get("mine", NOW)).toBe(0);
  });

  it("does not count on a check only", async () => {
    const c = core();
    await c.take([["a", 1]], false, NOW);
    expect(await c.get("a", NOW)).toBe(0);
  });

  it("adds without loss and starts again on the next UTC day", async () => {
    const c = core();
    await Promise.all(Array.from({ length: 50 }, () => c.add("spend", 0.5, NOW)));
    expect(await c.get("spend", NOW)).toBe(25);
    expect(await c.get("spend", NOW + DAY)).toBe(0);
  });
});

describe("install cap under load", () => {
  it("lets exactly 10 of 25 parallel installs through for one address", async () => {
    const h = harness();
    const out = await Promise.all(
      Array.from({ length: 25 }, () => h.call("/v2/install", { method: "POST" })),
    );
    const codes = out.map((r) => r.status);
    expect(codes.filter((c) => c === 200)).toHaveLength(10);
    expect(codes.filter((c) => c === 429)).toHaveLength(15);
  });

  it("counts a network once, and another network apart", async () => {
    const h = harness();
    const from = (ip: string) =>
      h.call("/v2/install", { method: "POST", headers: { "CF-Connecting-IP": ip } });
    const out = await Promise.all(
      Array.from({ length: 24 }, (_, i) => from(i % 2 ? "203.0.113.1" : "203.0.113.2")),
    );
    expect(out.filter((r) => r.status === 200)).toHaveLength(20);
  });
});

describe("new account cap", () => {
  it("does not count a code that is sent and never verified", async () => {
    const h = harness();
    for (let i = 0; i < 7; i++) {
      const r = await h.call("/v2/auth/email/send", { json: { email: `u${i}@example.com` } });
      expect(r.status).toBe(200);
    }
    expect(await counter(h, `newuser:total`)).toBe(0);
    expect(h.db.raw.prepare("SELECT COUNT(*) AS n FROM user").get()).toEqual({ n: 0 });
  });

  it("makes exactly 5 accounts from 8 parallel verifies for one address", async () => {
    const h = harness();
    const emails = Array.from({ length: 8 }, (_, i) => `u${i}@example.com`);
    for (const email of emails) await h.call("/v2/auth/email/send", { json: { email } });
    const out = await Promise.all(
      emails.map((email, i) =>
        h.call("/v2/auth/email/verify", { json: { email, code: h.mails[i].code } }),
      ),
    );
    expect(out.filter((r) => r.status === 200)).toHaveLength(5);
    const capped = out.filter((r) => r.status === 429);
    expect(capped).toHaveLength(3);
    expect(((await capped[0].json()) as any).error.type).toBe("install_limit");
    expect(h.db.raw.prepare("SELECT COUNT(*) AS n FROM user").get()).toEqual({ n: 5 });
  });

  it("does not count a login of an account that exists", async () => {
    const h = harness();
    await h.login("a@example.com");
    await h.login("a@example.com");
    expect(await counter(h, "newuser:total")).toBe(1);
  });
});

describe("spend budget under load", () => {
  const cost = costUnits(
    profileOf("haiku"),
    { prompt_tokens: 1000, completion_tokens: 100 },
    undefined,
  ) as number;

  it("loses no spend when 10 requests settle at once", async () => {
    const h = harness();
    const ids = await Promise.all(Array.from({ length: 10 }, () => h.install()));
    h.upstream(() => sse({ choices: [] }, usageChunk(1000, 100)));
    const out = await Promise.all(ids.map((id) => h.ask(id)));
    await Promise.all(out.map((r) => r.text()));
    await h.settled();
    expect(await counter(h, "spend")).toBeCloseTo((10 * cost) / UNITS_PER_USD, 9);
  });

  it("pauses the free coach once the shared budget is spent", async () => {
    const h = harness({ DAILY_BUDGET_V2_USD: String((3 * cost) / UNITS_PER_USD) });
    const ids = await Promise.all(Array.from({ length: 5 }, () => h.install()));
    h.upstream(() => sse({ choices: [] }, usageChunk(1000, 100)));
    for (const id of ids.slice(0, 3)) {
      await (await h.ask(id)).text();
      await h.settled();
    }
    const r = await h.ask(ids[3]);
    expect(r.status).toBe(503);
  });

  it("has no total cap for premium when the budget is unset or empty", async () => {
    for (const vars of [{}, { DAILY_BUDGET_V2_PREMIUM_USD: "" }]) {
      const h = harness({ DAILY_BUDGET_V2_USD: "0.0000001", ...vars });
      const { token, userId } = await h.login();
      await h.db.raw
        .prepare("INSERT INTO entitlement VALUES ('polar','s1',?, 'active', ?, 0, NULL, ?, ?)")
        .run(userId, Date.now() + DAY, Date.now(), Date.now());
      h.upstream(() => sse({ choices: [] }, usageChunk(1000, 100)));
      for (let i = 0; i < 3; i++) {
        const r = await h.ask(token);
        expect(r.status).toBe(200);
        await r.text();
        await h.settled();
      }
    }
  });
});

describe("error wrapper", () => {
  const broken = (make: () => Promise<Response>) => ({
    idFromName: (n: string) => n,
    get: () => ({ fetch: make }),
  });

  it("answers a failed Durable Object call with the JSON error shape", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const h = harness({ LIMITS: broken(async () => new Response("boom", { status: 500 })) });
    const r = await h.call("/v2/install", { method: "POST" });
    expect(r.status).toBe(500);
    expect(r.headers.get("Content-Type")).toBe("application/json");
    expect(((await r.json()) as any).error).toMatchObject({ type: "internal_error", code: 500 });
  });

  it("answers a thrown error with the JSON error shape", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const h = harness({
      WALLET: broken(async () => {
        throw new Error("wallet down");
      }),
    });
    const id = await h.install();
    const r = await h.call("/v2/balance", { token: id });
    expect(r.status).toBe(500);
    expect(((await r.json()) as any).error.type).toBe("internal_error");
  });

  it("does not put the error text in the answer", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const h = harness({
      LIMITS: broken(async () => {
        throw new Error("secret detail");
      }),
    });
    const r = await h.call("/v2/install", { method: "POST" });
    expect(await r.text()).not.toContain("secret detail");
  });
});

describe("cron purge", () => {
  it("deletes expired rows and keeps the rest", async () => {
    const h = harness();
    const now = Date.now();
    const db = h.db.raw;
    db.exec("INSERT INTO user (id, email, created_at) VALUES ('u1', 'a@example.com', 1)");
    const code = db.prepare(
      "INSERT INTO login_code (email, salt, code_hash, created_at, expires_at) VALUES ('a@example.com', 's', 'h', 1, ?)",
    );
    code.run(now - 1000);
    code.run(now + 60_000);
    const session = db.prepare(
      "INSERT INTO session (token_hash, user_id, created_at, expires_at) VALUES (?, 'u1', 1, ?)",
    );
    session.run("old", now - 1000);
    session.run("live", now + DAY);
    const hook = db.prepare(
      "INSERT INTO webhook_event (source, event_id, type, received_at) VALUES ('polar', ?, 't', ?)",
    );
    hook.run("old", now - 31 * DAY);
    hook.run("recent", now - 29 * DAY);

    await worker.scheduled({} as ScheduledController, h.env as any);

    expect(db.prepare("SELECT expires_at > ? AS live FROM login_code").all(now)).toEqual([
      { live: 1 },
    ]);
    expect(db.prepare("SELECT token_hash FROM session").all()).toEqual([{ token_hash: "live" }]);
    expect(db.prepare("SELECT event_id FROM webhook_event").all()).toEqual([
      { event_id: "recent" },
    ]);
  });
});

describe("entitlement edge cases", () => {
  const insert = (h: Harness, status: string, end: number | null, lastEvent: number) =>
    h.db.raw
      .prepare("INSERT INTO entitlement VALUES ('polar', 's1', 'u1', ?, ?, 0, NULL, ?, ?)")
      .run(status, end, lastEvent, lastEvent);

  it("keeps an active row with no period end only for 35 days after its last event", async () => {
    const h = harness();
    insert(h, "active", null, Date.now() - 10 * DAY);
    expect((await entitlementOf(h.env as any, "u1")).premium).toBe(true);
    const g = harness();
    insert(g, "active", null, Date.now() - 40 * DAY);
    expect((await entitlementOf(g.env as any, "u1")).premium).toBe(false);
  });

  it("stores a webhook without a status as unknown, and it is not premium", async () => {
    const h = harness();
    const { userId } = await h.login();
    const event = {
      type: "subscription.updated",
      timestamp: new Date().toISOString(),
      data: { id: "sub_1", customer: { external_id: userId } },
    };
    const s = await signed("polar_whs_test_secret", event);
    const r = await h.call("/v2/webhooks/polar", {
      method: "POST",
      body: s.body,
      headers: s.headers,
    });
    expect(r.status).toBe(200);
    const row = h.db.raw.prepare("SELECT status FROM entitlement").get() as { status: string };
    expect(row.status).toBe("unknown");
    expect((await entitlementOf(h.env as any, userId)).premium).toBe(false);
  });
});
