import { describe, expect, it } from "vitest";
import worker from "./index";

type Env = Parameters<typeof worker.fetch>[1];
type Ctx = Parameters<typeof worker.fetch>[2];

const call = (path: string, method = "GET") =>
  worker.fetch(new Request(`https://example.com${path}`, { method }), {} as Env, {} as Ctx);

describe("removed /v1 routes", () => {
  it.each(["/v1/install", "/v1/chat/completions"])("answers 410 on %s", async (path) => {
    const r = await call(path, "POST");
    expect(r.status).toBe(410);
    expect(r.headers.get("Content-Type")).toBe("application/json");
    expect(((await r.json()) as any).error.message).toContain("Update the app");
  });

  it("still answers /health", async () => {
    expect(await (await call("/health")).text()).toBe("ok");
  });

  it("answers 404 on an unknown path", async () => {
    expect((await call("/nope")).status).toBe(404);
  });
});
