/**
 * The daily limits of /v2: the install cap, the new-account cap and the spend
 * budget. One Durable Object holds all of them, so a check and an increment are
 * one atomic step. A KV counter cannot do this: parallel requests all read the
 * same value before any of them writes.
 *
 * The logic is a plain class over a tiny storage interface, like WalletCore, so
 * Vitest can run it without a Workers runtime. The counters reset at UTC midnight.
 *
 * shortcut: one object serves every request, so it handles one request at a time.
 * Each call is a few microseconds. Shard by key prefix if the load ever needs it.
 */

interface State {
  day: string;
  /** Counts, and USD spend. The key names the meaning. */
  n: Record<string, number>;
}

export interface LimitStore {
  get(): Promise<State | undefined>;
  put(state: State): Promise<void>;
}

/** A counter and the highest value it may reach in one day. */
export type Cap = [key: string, cap: number];

export type Taken = { ok: true } | { ok: false; key: string };

const dayOf = (now: number) => new Date(now).toISOString().slice(0, 10);

export class LimitsCore {
  private chain: Promise<unknown> = Promise.resolve();

  constructor(private store: LimitStore) {}

  /** One caller at a time. The state is read, changed and written without a gap. */
  private locked<T>(fn: (n: Record<string, number>) => T, now: number): Promise<T> {
    const run = async () => {
      const day = dayOf(now);
      let s = await this.store.get();
      if (!s || s.day !== day) s = { day, n: {} };
      const out = fn(s.n);
      await this.store.put(s);
      return out;
    };
    const next = this.chain.then(run, run);
    this.chain = next.catch(() => {});
    return next;
  }

  /**
   * Add 1 to every counter, only if all of them are under their cap. With
   * `commit` false, only check. The first counter at its cap is named.
   */
  take(caps: Cap[], commit = true, now = Date.now()): Promise<Taken> {
    return this.locked((n): Taken => {
      const full = caps.find(([key, cap]) => (n[key] ?? 0) >= cap);
      if (full) return { ok: false, key: full[0] };
      if (commit) for (const [key] of caps) n[key] = (n[key] ?? 0) + 1;
      return { ok: true };
    }, now);
  }

  /** Add `by` to a counter and return the new value. */
  add(key: string, by: number, now = Date.now()): Promise<number> {
    return this.locked((n) => (n[key] = (n[key] ?? 0) + by), now);
  }

  get(key: string, now = Date.now()): Promise<number> {
    return this.locked((n) => n[key] ?? 0, now);
  }
}

/* ------------------------------------------------------- Durable Object --- */

type Message =
  | { op: "take"; caps: Cap[]; commit?: boolean }
  | { op: "add"; key: string; by: number }
  | { op: "get"; key: string };

/** A plain class, like Wallet. The calls come in as JSON over `fetch`. */
export class Limits {
  private core: LimitsCore;

  constructor(state: { storage: DurableObjectStorage }) {
    this.core = new LimitsCore({
      get: () => state.storage.get<State>("s"),
      put: (s) => state.storage.put("s", s),
    });
  }

  async fetch(request: Request): Promise<Response> {
    const m = (await request.json()) as Message;
    switch (m.op) {
      case "take":
        return Response.json(await this.core.take(m.caps, m.commit !== false));
      case "add":
        return Response.json({ total: await this.core.add(m.key, m.by) });
      case "get":
        return Response.json({ value: await this.core.get(m.key) });
      default:
        return new Response("bad op", { status: 400 });
    }
  }
}
