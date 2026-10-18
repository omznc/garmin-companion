import { describe, expect, it } from "vitest";
import { FREE_GRANT, PREMIUM_MONTH, type Store, WalletCore } from "./wallet";

const NOW = Date.parse("2026-10-19T12:00:00Z");
const HOUR = 3_600_000;

function core() {
  let state: any;
  const store: Store = {
    get: async () => structuredClone(state),
    put: async (s) => void (state = structuredClone(s)),
  };
  return new WalletCore(store);
}

describe("wallet", () => {
  it("holds, settles and refunds the difference", async () => {
    const w = core();
    expect((await w.reserve("r1", 10_000, false, NOW)).ok).toBe(true);
    expect((await w.snapshot(false, NOW)).credits_remaining).toBe(20);
    const s = await w.settle("r1", 4_000, false, NOW);
    expect(s.charged).toBe(4_000);
    expect(s.snapshot.credits_remaining).toBe(26);
  });

  it("charges once when settle runs twice", async () => {
    const w = core();
    await w.reserve("r1", 10_000, false, NOW);
    await w.settle("r1", 4_000, false, NOW);
    const again = await w.settle("r1", 4_000, false, NOW);
    expect(again.charged).toBe(0);
    expect(again.snapshot.credits_remaining).toBe(26);
  });

  it("refuses a repeated request id and holds only once", async () => {
    const w = core();
    await w.reserve("r1", 10_000, false, NOW);
    const second = await w.reserve("r1", 10_000, false, NOW);
    expect(second.ok === false && second.reason).toBe("duplicate");
    expect((await w.snapshot(false, NOW)).credits_remaining).toBe(20);
    // Also after it was charged.
    await w.settle("r1", 1_000, false, NOW);
    expect((await w.reserve("r1", 1_000, false, NOW)).ok).toBe(false);
  });

  it("lets exactly 5 of 20 parallel reservations through", async () => {
    const w = core();
    const results = await Promise.all(
      Array.from({ length: 20 }, (_, i) => w.reserve(`r${i}`, FREE_GRANT / 5, false, NOW)),
    );
    expect(results.filter((r) => r.ok)).toHaveLength(5);
    const refused = results.find((r) => !r.ok);
    expect(refused && !refused.ok && refused.reason).toBe("out_of_credits");
  });

  it("never goes below zero in a random run", async () => {
    const w = core();
    let seed = 7;
    const rnd = () => (seed = (seed * 1103515245 + 12345) % 2 ** 31) / 2 ** 31;
    const open: { id: string; amt: number }[] = [];
    for (let i = 0; i < 300; i++) {
      const roll = rnd();
      if (roll < 0.5) {
        const amt = Math.floor(rnd() * 12_000) + 1;
        if ((await w.reserve(`r${i}`, amt, false, NOW)).ok) open.push({ id: `r${i}`, amt });
      } else if (open.length) {
        const h = open.splice(Math.floor(rnd() * open.length), 1)[0];
        if (roll < 0.8) await w.settle(h.id, Math.floor(rnd() * h.amt * 1.5), false, NOW);
        else await w.release(h.id, false, NOW);
      }
      const left = (await w.snapshot(false, NOW)).credits_remaining;
      expect(left).toBeGreaterThanOrEqual(0);
      expect(left).toBeLessThanOrEqual(30);
    }
  });

  it("clamps a settle above the hold", async () => {
    const w = core();
    await w.reserve("r1", 5_000, false, NOW);
    const s = await w.settle("r1", 9_000, false, NOW);
    expect(s.clamped).toBe(true);
    expect(s.charged).toBe(5_000);
  });

  it("charges an expired hold in full, so a slow client cannot read for free", async () => {
    const w = core();
    await w.reserve("r1", 20_000, false, NOW);
    expect((await w.snapshot(false, NOW + 6 * 60_000)).credits_remaining).toBe(10);
    // The late settle charges nothing more, and the id stays used.
    expect((await w.settle("r1", 5_000, false, NOW + 6 * 60_000)).charged).toBe(0);
    expect((await w.snapshot(false, NOW + 7 * 60_000)).credits_remaining).toBe(10);
    expect((await w.reserve("r1", 1_000, false, NOW + 7 * 60_000)).ok).toBe(false);
  });

  it("release gives everything back and frees the id", async () => {
    const w = core();
    await w.reserve("r1", 20_000, false, NOW);
    await w.release("r1", false, NOW);
    expect((await w.snapshot(false, NOW)).credits_remaining).toBe(30);
    expect((await w.reserve("r1", 20_000, false, NOW)).ok).toBe(true);
  });

  it("grants once per UTC day", async () => {
    const w = core();
    await w.reserve("r1", 30_000, false, NOW);
    await w.settle("r1", 30_000, false, NOW);
    expect((await w.snapshot(false, NOW + HOUR)).credits_remaining).toBe(0);
    const tomorrow = Date.parse("2026-10-20T00:00:01Z");
    expect((await w.snapshot(false, tomorrow)).credits_remaining).toBe(30);
    await w.reserve("r2", 10_000, false, tomorrow);
    expect((await w.snapshot(false, tomorrow + 60_000)).credits_remaining).toBe(20);
  });

  it("premium spends its pool first and falls back to the free grant", async () => {
    const w = core();
    expect((await w.reserve("p1", 100_000, true, NOW)).ok && "prem").toBe("prem");
    await w.settle("p1", 100_000, true, NOW);
    // 400 a day and 30 free: 300 + 30 left.
    expect((await w.snapshot(true, NOW)).credits_remaining).toBe(330);
    // Over the daily premium room: served from the free grant.
    const big = await w.reserve("p2", 350_000, true, NOW);
    expect(big.ok).toBe(false);
    const small = await w.reserve("p3", 300_000, true, NOW);
    expect(small.ok && small.pool).toBe("prem");
  });

  it("falls back to free after the monthly ceiling, then says fair use", async () => {
    const w = core();
    // Use the month up over many days.
    for (let d = 1; d <= 15; d++) {
      const t = Date.parse(`2026-10-${String(d).padStart(2, "0")}T10:00:00Z`);
      await w.reserve(`m${d}`, 400_000, true, t);
      await w.settle(`m${d}`, 400_000, true, t);
    }
    const t = Date.parse("2026-10-16T10:00:00Z");
    const snap = await w.snapshot(true, t);
    expect(snap.month_used).toBe(6000);
    expect(snap.month_used * 1000).toBe(PREMIUM_MONTH);
    const free = await w.reserve("f1", 30_000, true, t);
    expect(free.ok && free.pool).toBe("free");
    await w.settle("f1", 30_000, true, t);
    const none = await w.reserve("f2", 10_000, true, t);
    expect(none.ok === false && none.reason).toBe("fair_use_ceiling");
    // A new month starts the ceiling again.
    const next = await w.reserve("n1", 10_000, true, Date.parse("2026-11-01T01:00:00Z"));
    expect(next.ok && next.pool).toBe("prem");
  });
});
