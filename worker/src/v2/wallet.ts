/**
 * The credit wallet. One Durable Object per identity: `install:<id>` for a free
 * anonymous install, `user:<id>` for a signed-in user.
 *
 * The logic is a plain class over a tiny storage interface so Vitest can run it
 * without a Workers runtime. All amounts are integer units (a thousandth of a
 * credit, see models.ts). The balance cannot go below zero because every request
 * first holds an upper-bound estimate, and settling never charges more than the
 * hold.
 *
 * shortcut: expired holds are released lazily on the next call instead of by a
 * Durable Object alarm. The result is the same, and an idle wallet costs nothing.
 */

export const FREE_GRANT = 30_000; // 30 credits a day
export const PREMIUM_DAY = 400_000;
export const PREMIUM_MONTH = 6_000_000;
const HOLD_MS = 5 * 60_000;
const DONE_KEEP = 200;

type Pool = "free" | "prem";

interface State {
  free: { day: string; used: number };
  prem: { day: string; dayUsed: number; month: string; monthUsed: number };
  holds: Record<string, { amt: number; pool: Pool; exp: number }>;
  /** Request ids that were charged, oldest first. Blocks a second charge. */
  done: string[];
}

export interface Store {
  get(): Promise<State | undefined>;
  put(state: State): Promise<void>;
}

export interface Snapshot {
  credits_remaining: number;
  daily_grant: number;
  resets_at: string;
  month_used: number;
  month_ceiling: number;
}

export type Reserved =
  | { ok: true; pool: Pool; snapshot: Snapshot }
  | { ok: false; reason: "out_of_credits" | "fair_use_ceiling" | "duplicate"; snapshot: Snapshot };

const dayOf = (now: number) => new Date(now).toISOString().slice(0, 10);

export function nextMidnight(now: number): string {
  const d = new Date(now);
  return new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate() + 1)).toISOString();
}

const blank = (now: number): State => ({
  free: { day: dayOf(now), used: 0 },
  prem: { day: dayOf(now), dayUsed: 0, month: dayOf(now).slice(0, 7), monthUsed: 0 },
  holds: {},
  done: [],
});

export class WalletCore {
  private chain: Promise<unknown> = Promise.resolve();

  constructor(private store: Store) {}

  /** One caller at a time, so two requests cannot both spend the same credits. */
  private locked<T>(fn: (s: State) => T, now: number): Promise<T> {
    const run = async () => {
      const s = (await this.store.get()) ?? blank(now);
      const day = dayOf(now);
      if (s.free.day !== day) s.free = { day, used: 0 };
      if (s.prem.day !== day) {
        s.prem.day = day;
        s.prem.dayUsed = 0;
      }
      if (s.prem.month !== day.slice(0, 7)) {
        s.prem.month = day.slice(0, 7);
        s.prem.monthUsed = 0;
      }
      for (const [id, h] of Object.entries(s.holds)) if (h.exp <= now) delete s.holds[id];
      const out = fn(s);
      await this.store.put(s);
      return out;
    };
    const next = this.chain.then(run, run);
    this.chain = next.catch(() => {});
    return next;
  }

  private held(s: State, pool: Pool): number {
    return Object.values(s.holds).reduce((n, h) => n + (h.pool === pool ? h.amt : 0), 0);
  }

  private room(s: State, pool: Pool): number {
    const held = this.held(s, pool);
    if (pool === "free") return FREE_GRANT - s.free.used - held;
    return Math.min(PREMIUM_DAY - s.prem.dayUsed, PREMIUM_MONTH - s.prem.monthUsed) - held;
  }

  private snap(s: State, premium: boolean, now: number): Snapshot {
    const units =
      Math.max(0, this.room(s, "free")) + (premium ? Math.max(0, this.room(s, "prem")) : 0);
    return {
      credits_remaining: Math.floor(units / 1000),
      daily_grant: FREE_GRANT / 1000,
      resets_at: nextMidnight(now),
      month_used: Math.floor(s.prem.monthUsed / 1000),
      month_ceiling: PREMIUM_MONTH / 1000,
    };
  }

  snapshot(premium: boolean, now = Date.now()): Promise<Snapshot> {
    return this.locked((s) => this.snap(s, premium, now), now);
  }

  /** Hold `units` for request `id`. A repeated id is refused, never charged twice. */
  reserve(id: string, units: number, premium: boolean, now = Date.now()): Promise<Reserved> {
    return this.locked((s): Reserved => {
      const snapshot = this.snap(s, premium, now);
      if (s.holds[id] || s.done.includes(id)) return { ok: false, reason: "duplicate", snapshot };
      let pool: Pool | null = null;
      if (premium && this.room(s, "prem") >= units) pool = "prem";
      else if (this.room(s, "free") >= units) pool = "free";
      if (!pool) {
        return { ok: false, reason: premium ? "fair_use_ceiling" : "out_of_credits", snapshot };
      }
      s.holds[id] = { amt: units, pool, exp: now + HOLD_MS };
      return { ok: true, pool, snapshot };
    }, now);
  }

  /**
   * Charge `actual` units against the hold. More than the hold is clamped to the
   * hold (it should not happen, the hold is an upper bound). A second settle of
   * the same id, or one after the hold expired, charges nothing.
   */
  settle(id: string, actual: number, premium: boolean, now = Date.now()) {
    return this.locked((s) => {
      const h = s.holds[id];
      let charged = 0;
      let clamped = false;
      if (h) {
        charged = Math.min(Math.max(0, Math.round(actual)), h.amt);
        clamped = actual > h.amt;
        if (clamped) console.warn("wallet: settle above hold, clamped");
        if (h.pool === "free") s.free.used += charged;
        else {
          s.prem.dayUsed += charged;
          s.prem.monthUsed += charged;
        }
        delete s.holds[id];
        s.done.push(id);
        if (s.done.length > DONE_KEEP) s.done.shift();
      }
      return { charged, clamped, snapshot: this.snap(s, premium, now) };
    }, now);
  }

  /** Give the hold back, no charge. The id can be used again. */
  release(id: string, premium: boolean, now = Date.now()) {
    return this.locked((s) => {
      delete s.holds[id];
      return this.snap(s, premium, now);
    }, now);
  }
}

/* ------------------------------------------------------- Durable Object --- */

/**
 * A plain class, not `extends DurableObject`, so this file loads in Node tests.
 * The calls come in as JSON over `fetch` from `callWallet`.
 */
export class Wallet {
  private core: WalletCore;

  constructor(private state: { storage: DurableObjectStorage }) {
    this.core = new WalletCore({
      get: () => state.storage.get<State>("s"),
      put: (s) => state.storage.put("s", s),
    });
  }

  async fetch(request: Request): Promise<Response> {
    const m = (await request.json()) as {
      op: string;
      id: string;
      units: number;
      premium: boolean;
    };
    switch (m.op) {
      case "snapshot":
        return Response.json(await this.core.snapshot(m.premium));
      case "reserve":
        return Response.json(await this.core.reserve(m.id, m.units, m.premium));
      case "settle":
        return Response.json(await this.core.settle(m.id, m.units, m.premium));
      case "release":
        return Response.json(await this.core.release(m.id, m.premium));
      case "delete":
        await this.state.storage.deleteAll();
        return Response.json({ ok: true });
      default:
        return new Response("bad op", { status: 400 });
    }
  }
}
