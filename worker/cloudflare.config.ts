import { bindings, defineConfig, exports } from "cf/config";

// No `observability` block on purpose. Workers Logs would persist request
// metadata for a service whose requests carry someone's resting heart rate.
export default defineConfig({
  worker: {
    name: "coach",
    compatibilityDate: "2025-06-01",
    entrypoint: "src/index.ts",
    // The balance of each install or user. SQLite storage.
    exports: {
      Wallet: exports.durableObject({ storage: "sqlite" }),
    },
    env: {
      // --- /v1: frozen for v0.9.1 users. Do not change. ---
      MODEL: bindings.text("inclusionai/ling-3.0-flash"),
      // USD across all /v1 users per UTC day.
      DAILY_BUDGET_USD: bindings.text("5"),
      // Per-day request counts and the running spend. Eventually consistent,
      // which is fine: the budget is a ceiling with a soft edge, not a ledger.
      COUNTERS: bindings.kv({
        id: "b34d6d482b4e43e9afa0864c9faac10e",
        dev: { remote: true },
      }),
      // Burst limits. Counted per Cloudflare location, so they are looser than
      // they read. The spend limits are what bound the damage.
      BURST: bindings.rateLimit({ namespace: "1001", simple: { limit: 30, period: 60 } }),
      ISSUE: bindings.rateLimit({ namespace: "1002", simple: { limit: 3, period: 60 } }),

      // --- /v2: the paid coach. See V2.md. ---
      // `haiku` or `deepseek`, from src/v2/models.ts. The swap is this one line.
      MODEL_PROFILE: bindings.text("haiku"),
      // "true" stops free grants for anonymous installs. Keep it off until abuse shows.
      FREE_REQUIRES_LOGIN: bindings.text("false"),
      // A second guard on /v2 spend. The main limit is on the OpenRouter key.
      DAILY_BUDGET_V2_USD: bindings.text("5"),
      // Production. The sandbox is https://sandbox-api.polar.sh, product
      // 5bad678a-4124-4c12-901c-09e79ba19818.
      POLAR_API_BASE: bindings.text("https://api.polar.sh"),
      POLAR_PRODUCT_ID: bindings.text("0f637cf7-4de6-4956-b90b-23b7e9e3b42a"),
      CHECKOUT_RETURN_URL: bindings.text("https://github.com/omznc/garmin-companion"),
      // Needs a domain with Resend set up. Until then login mail answers 503.
      MAIL_FROM: bindings.text(""),
      // Users, sessions, login codes, entitlements and webhook ids.
      // Schema in migrations/. Apply with `cf d1 migrations apply <id>`.
      DB: bindings.d1({ name: "garmin-coach", id: "9791e324-7a61-49a2-a513-e2d63d778bd8" }),
      WALLET: bindings.durableObject({ worker: "coach", exportName: "Wallet" }),
      // Secrets, not in this file. Set them with `cf workers secrets update`:
      // OPENROUTER_KEY, POLAR_TOKEN, POLAR_WEBHOOK_SECRET, RESEND_KEY.
    },
  },
});
