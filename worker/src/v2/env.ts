import type { Mailer } from "./auth";

/** The bindings /v2 uses. The `/v1` code uses the first four as well. */
export interface V2Env {
  OPENROUTER_KEY: string;
  COUNTERS: KVNamespace;
  BURST: RateLimit;
  ISSUE: RateLimit;
  DB: D1Database;
  WALLET: DurableObjectNamespace;
  /** `haiku` or `deepseek`, see models.ts. */
  MODEL_PROFILE?: string;
  /** `true` stops free grants for anonymous installs. Off by default. */
  FREE_REQUIRES_LOGIN?: string;
  /** Second guard on all /v2 spend. The main limit is on the OpenRouter key. */
  DAILY_BUDGET_V2_USD?: string;
  POLAR_TOKEN?: string;
  POLAR_WEBHOOK_SECRET?: string;
  /** `https://sandbox-api.polar.sh` for the sandbox. */
  POLAR_API_BASE?: string;
  POLAR_PRODUCT_ID?: string;
  /** Where the browser lands after a paid checkout. */
  CHECKOUT_RETURN_URL?: string;
  RESEND_KEY?: string;
  MAIL_FROM?: string;
  /** Test hook. Replaces the mail sender. */
  MAILER?: Mailer;
}
