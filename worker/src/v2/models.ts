/**
 * Everything the worker knows about a model, in one place. A swap from Haiku to
 * DeepSeek is a change of `MODEL_PROFILE` in cloudflare.config.ts and nothing else.
 * Prices are USD per million tokens, read from the OpenRouter models API on
 * 2026-10-18.
 */
export interface ModelProfile {
  slug: string;
  input: number;
  output: number;
  cacheRead: number;
  /** Zero when the provider has no separate write price. */
  cacheWrite: number;
  /** The body size limit. It also bounds the prompt, at one token a byte at worst. */
  maxBodyBytes: number;
  maxTokens: number;
  reasoning?: Record<string, unknown>;
  /** A free model to try first. OpenRouter falls back to `slug` when it fails. */
  free?: string;
  /** OpenRouter provider routing. The client cannot set it. */
  provider?: Record<string, unknown>;
}

export const PROFILES: Record<string, ModelProfile> = {
  haiku: {
    slug: "anthropic/claude-haiku-5.5",
    input: 0.1,
    output: 0.5,
    cacheRead: 0.01,
    cacheWrite: 0.125,
    // Anthropic charges five times more for a prompt over 100K tokens.
    maxBodyBytes: 100_000,
    maxTokens: 8192,
    reasoning: { effort: "low" },
  },
  deepseek: {
    // A cap above the cheapest endpoints on 2026-10-20 (DeepInfra 0.078/0.235,
    // Decart 0.09/0.18). The old 0.05/0.1 matched no endpoint, so the fallback
    // had nowhere to go when the free model failed.
    slug: "deepseek/deepseek-v4.1-flash-1017",
    free: "minimax/minimax-m3.1-flash:free",
    input: 0.1,
    output: 0.25,
    cacheRead: 0.01,
    cacheWrite: 0,
    maxBodyBytes: 256_000,
    maxTokens: 8192,
    // Both models reason by default (MiniMax at max, DeepSeek at high). A
    // tool-heavy turn spent the app's whole 3000-token round on reasoning and
    // wrote no answer, or a cut-off one.
    reasoning: { effort: "low" },
    // The cheapest endpoint, never one over the prices above, so the bill stays correct.
    provider: { sort: "price", max_price: { prompt: 0.1, completion: 0.25 } },
  },
};

export function profileOf(name: string | undefined): ModelProfile {
  return PROFILES[name ?? "haiku"] ?? PROFILES.haiku;
}

/** 1 credit is 0.0005 USD. The ledger counts in thousandths of a credit. */
export const UNITS_PER_USD = 2_000_000;

export const usdToUnits = (usd: number): number => Math.ceil(usd * UNITS_PER_USD);

/**
 * Body bytes per prompt token in the hold. English and JSON are about 4 bytes
 * a token and dense numeric JSON is about 3, so 3.5 holds a realistic cost.
 * shortcut: the hold is not a strict bound. A body of rarer text costs more
 * than it holds, and settle clamps the charge to the hold, so a wallet never
 * pays more than it held. The overshoot is at most 100 KB of one-byte tokens,
 * under 0.01 USD, and BURST and the daily budget limit how often it can happen.
 */
export const BYTES_PER_TOKEN = 3.5;

/** The hold: the prompt at `BYTES_PER_TOKEN`, and the answer at max_tokens. */
export function estimateUnits(p: ModelProfile, bodyBytes: number, maxTokens: number): number {
  const promptTokens = Math.ceil(bodyBytes / BYTES_PER_TOKEN);
  return usdToUnits((promptTokens * p.input + maxTokens * p.output) / 1e6);
}

export interface Usage {
  prompt_tokens?: number;
  completion_tokens?: number;
  prompt_tokens_details?: { cached_tokens?: number; cache_write_tokens?: number };
}

/** The response may name the free model without its ":free" suffix. */
const servedFree = (p: ModelProfile, served: string | undefined): boolean =>
  !!p.free && !!served?.startsWith(p.free.split(":")[0]);

/**
 * The cost in units from token counts, or null when the usage block is missing.
 * Not OpenRouter's `usage.cost`: with BYOK that field may not carry the
 * Anthropic price. Thinking tokens are inside completion_tokens.
 */
export function costUnits(
  p: ModelProfile,
  usage: Usage | undefined,
  served?: string,
): number | null {
  if (!usage || typeof usage.prompt_tokens !== "number") return null;
  if (servedFree(p, served)) return 0;
  const cached = usage.prompt_tokens_details?.cached_tokens ?? 0;
  const written = usage.prompt_tokens_details?.cache_write_tokens ?? 0;
  const fresh = Math.max(0, usage.prompt_tokens - cached - written);
  const usd =
    (fresh * p.input +
      cached * p.cacheRead +
      written * (p.cacheWrite || p.input) +
      (usage.completion_tokens ?? 0) * p.output) /
    1e6;
  return usdToUnits(usd);
}
