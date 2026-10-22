/**
 * Test doubles for /v2: KV as a Map, D1 on node:sqlite with the real migration,
 * the Wallet Durable Object on a Map, and a scripted `fetch` for OpenRouter,
 * Polar and the mail API.
 */
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { vi } from "vitest";
import worker from "../index";
import type { Mailer } from "./auth";
import { Limits } from "./limits";
import { Wallet } from "./wallet";

type Env = Parameters<typeof worker.fetch>[1];

class Stmt {
  constructor(
    private db: DatabaseSync,
    private sql: string,
    private params: unknown[] = [],
  ) {}
  bind(...p: unknown[]) {
    return new Stmt(this.db, this.sql, p);
  }
  async run() {
    const r = this.db.prepare(this.sql).run(...(this.params as any[]));
    return { success: true, meta: { changes: Number(r.changes) } };
  }
  async first() {
    return this.db.prepare(this.sql).get(...(this.params as any[])) ?? null;
  }
  async all() {
    return { results: this.db.prepare(this.sql).all(...(this.params as any[])) };
  }
}

function d1() {
  const db = new DatabaseSync(":memory:");
  db.exec(
    readFileSync(
      fileURLToPath(new URL("../../migrations/0001_init.sql", import.meta.url).href),
      "utf8",
    ),
  );
  return {
    raw: db,
    prepare: (sql: string) => new Stmt(db, sql),
    batch: async (list: Stmt[]) => {
      const out = [];
      for (const s of list) out.push(await s.run());
      return out;
    },
  };
}

function objectNamespace(make: (state: { storage: DurableObjectStorage }) => Wallet | Limits) {
  const wallets = new Map<string, Wallet | Limits>();
  const get = (name: string) => {
    let w = wallets.get(name);
    if (!w) {
      const mem = new Map<string, unknown>();
      w = make({
        storage: {
          get: async (k: string) => mem.get(k),
          put: async (k: string, v: unknown) => void mem.set(k, structuredClone(v)),
          deleteAll: async () => mem.clear(),
        } as unknown as DurableObjectStorage,
      });
      wallets.set(name, w);
    }
    return w;
  };
  return {
    idFromName: (n: string) => n,
    get: (name: string) => ({
      fetch: (url: string, init: RequestInit) => get(name).fetch(new Request(url, init)),
    }),
  };
}

export const sse = (...events: unknown[]) =>
  new Response(
    new ReadableStream({
      start(c) {
        const enc = new TextEncoder();
        for (const e of events) c.enqueue(enc.encode(`data: ${JSON.stringify(e)}\n\n`));
        c.enqueue(enc.encode("data: [DONE]\n\n"));
        c.close();
      },
    }),
    { headers: { "Content-Type": "text/event-stream" } },
  );

export const usageChunk = (prompt: number, completion: number) => ({
  choices: [],
  usage: { prompt_tokens: prompt, completion_tokens: completion },
});

export function harness(vars: Record<string, unknown> = {}) {
  const kv = new Map<string, string>();
  const db = d1();
  const mails: { to: string; code: string }[] = [];
  const mailer: Mailer = {
    send: async (to, code) => {
      mails.push({ to, code });
      return true;
    },
  };
  const fetchCalls: { url: string; init: RequestInit }[] = [];
  let script: (url: string, init: RequestInit) => Response | Promise<Response> = () =>
    new Response("{}", { status: 500 });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init: RequestInit) => {
      fetchCalls.push({ url, init });
      return script(url, init);
    }),
  );

  const env = {
    OPENROUTER_KEY: "not-a-real-key",
    MODEL: "unused-by-v2",
    DAILY_BUDGET_USD: "5",
    COUNTERS: {
      get: async (k: string) => kv.get(k) ?? null,
      put: async (k: string, v: string) => void kv.set(k, v),
    },
    BURST: { limit: async () => ({ success: true }) },
    ISSUE: { limit: async () => ({ success: true }) },
    DB: db,
    WALLET: objectNamespace((st) => new Wallet(st)),
    LIMITS: objectNamespace((st) => new Limits(st)),
    MAILER: mailer,
    MODEL_PROFILE: "haiku",
    POLAR_TOKEN: "polar-test-token",
    POLAR_WEBHOOK_SECRET: "polar_whs_test_secret",
    POLAR_PRODUCT_ID: "prod_test",
    POLAR_API_BASE: "https://sandbox-api.polar.sh",
    ...vars,
  } as unknown as Env;

  let pending: Promise<unknown>[] = [];
  const ctx = {
    waitUntil: (p: Promise<unknown>) => void pending.push(p),
    passThroughOnException: () => {},
  } as unknown as Parameters<typeof worker.fetch>[2];

  const h = {
    kv,
    db,
    env,
    mails,
    fetchCalls,
    upstream: (fn: typeof script) => void (script = fn),
    /** A call, with whatever it deferred settled before it returns. */
    async call(path: string, init: RequestInit & { token?: string; json?: unknown } = {}) {
      const headers = new Headers(init.headers);
      if (init.token) headers.set("Authorization", `Bearer ${init.token}`);
      let body = init.body;
      if (init.json !== undefined) body = JSON.stringify(init.json);
      const res = await worker.fetch(
        new Request(`https://example.com${path}`, {
          method: init.method ?? (body ? "POST" : "GET"),
          headers,
          body,
        }),
        env,
        ctx,
      );
      await Promise.all(pending);
      pending = [];
      return res;
    },
    async settled() {
      await Promise.all(pending);
      pending = [];
    },
    async install() {
      const r = await h.call("/v2/install", { method: "POST" });
      return ((await r.json()) as { id: string }).id;
    },
    async balance(token: string) {
      return (await (await h.call("/v2/balance", { token })).json()) as Record<string, any>;
    },
    /** Sign in and return the session token and user id. */
    async login(email = "a@example.com") {
      await h.call("/v2/auth/email/send", { json: { email } });
      const code = mails[mails.length - 1].code;
      const r = await h.call("/v2/auth/email/verify", { json: { email, code } });
      const { token } = (await r.json()) as { token: string };
      const row = db.raw.prepare("SELECT id FROM user WHERE email = ?").get(email) as {
        id: string;
      };
      return { token, userId: row.id };
    },
    ask(token: string, extra: Record<string, unknown> = {}, headers: Record<string, string> = {}) {
      return h.call("/v2/chat/completions", {
        token,
        headers,
        json: { messages: [{ role: "user", content: "hi" }], stream: true, ...extra },
      });
    },
  };
  return h;
}

/** A signed Polar webhook, Standard Webhooks style. */
export async function signed(
  secret: string,
  event: unknown,
  opts: { id?: string; ts?: number; badSig?: boolean } = {},
) {
  const body = JSON.stringify(event);
  const id = opts.id ?? `evt_${crypto.randomUUID()}`;
  const ts = String(opts.ts ?? Math.floor(Date.now() / 1000));
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = new Uint8Array(
    await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(`${id}.${ts}.${body}`)),
  );
  let sig = btoa(String.fromCharCode(...mac));
  if (opts.badSig) sig = btoa("x".repeat(32));
  return {
    body,
    headers: { "webhook-id": id, "webhook-timestamp": ts, "webhook-signature": `v1,${sig}` },
  };
}
