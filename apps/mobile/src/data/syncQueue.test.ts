import assert from "node:assert/strict";
import { test } from "node:test";
import { createSyncQueue, type SyncArgs } from "./syncQueue.ts";

// A fake source: each run waits until the test finishes it.
function fake() {
  const log: string[] = [];
  const open: { args: SyncArgs; finish: (fail?: boolean) => void }[] = [];
  const busy: boolean[] = [];
  let running = 0;
  const run = (args: SyncArgs) =>
    new Promise<string>((resolve, reject) => {
      running += 1;
      assert.equal(running, 1, "two runs overlap");
      log.push(`start ${JSON.stringify(args)}`);
      open.push({
        args,
        finish: (fail) => {
          running -= 1;
          log.push(`end ${JSON.stringify(args)}`);
          if (fail) reject(new Error("boom"));
          else resolve(JSON.stringify(args));
        },
      });
    });
  const sync = createSyncQueue(run, (b) => busy.push(b));
  return { log, open, busy, sync };
}

const tick = () => new Promise((r) => setImmediate(r));

test("a call with the same args joins the run", async () => {
  const f = fake();
  const a = f.sync({ days: 14 });
  const b = f.sync({ days: 14 });
  assert.equal(a, b);
  await tick();
  f.open[0]?.finish();
  await a;
  assert.equal(f.open.length, 1);
});

test("a full import waits for a quick sync and runs with its own args", async () => {
  const f = fake();
  const quick = f.sync({});
  const full = f.sync({ full: true, days: 365 });
  await tick();
  assert.equal(f.open.length, 1);
  f.open[0]?.finish();
  assert.equal(await quick, "{}");
  await tick();
  assert.equal(f.open.length, 2);
  assert.deepEqual(f.open[1]?.args, { full: true, days: 365 });
  f.open[1]?.finish();
  assert.equal(await full, '{"full":true,"days":365}');
});

test("a failed run does not block the next run", async () => {
  const f = fake();
  const first = f.sync({});
  const second = f.sync({ days: 14 });
  await tick();
  f.open[0]?.finish(true);
  await assert.rejects(first, /boom/);
  await tick();
  f.open[1]?.finish();
  assert.equal(await second, '{"days":14}');
});

test("busy is true until the queue is empty", async () => {
  const f = fake();
  const first = f.sync({});
  const second = f.sync({ days: 14 });
  await tick();
  f.open[0]?.finish();
  await first;
  await tick();
  assert.deepEqual(f.busy, [true, true]);
  f.open[1]?.finish();
  await second;
  await tick();
  assert.deepEqual(f.busy, [true, true, false]);
});
