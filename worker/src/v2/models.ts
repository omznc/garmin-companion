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
  /** A token is at least one byte, so this also caps the prompt in tokens. */
  maxBodyBytes: number;
  maxTokens: number;
  reasoning?: Record<string, unknown>;
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
    slug: "deepseek/deepseek-v4.1-flash",
    input: 0.06,
    output: 0.12,
    cacheRead: 0.0016,
    cacheWrite: 0,
    maxBodyBytes: 256_000,
    maxTokens: 8192,
  },
};

export function profileOf(name: string | undefined): ModelProfile {
  return PROFILES[name ?? "haiku"] ?? PROFILES.haiku;
}

/** 1 credit is 0.0005 USD. The ledger counts in thousandths of a credit. */
export const UNITS_PER_USD = 2_000_000;

export const usdToUnits = (usd: number): number => Math.ceil(usd * UNITS_PER_USD);

/** An upper bound: every body byte is a token, and the answer uses max_tokens. */
export function estimateUnits(p: ModelProfile, bodyBytes: number, maxTokens: number): number {
  return usdToUnits((bodyBytes * p.input + maxTokens * p.output) / 1e6);
}

export interface Usage {
  prompt_tokens?: number;
  completion_tokens?: number;
  prompt_tokens_details?: { cached_tokens?: number; cache_write_tokens?: number };
}

/**
 * The cost in units from token counts, or null when the usage block is missing.
 * Not OpenRouter's `usage.cost`: with BYOK that field may not carry the
 * Anthropic price. Thinking tokens are inside completion_tokens.
 */
export function costUnits(p: ModelProfile, usage: Usage | undefined): number | null {
  if (!usage || typeof usage.prompt_tokens !== "number") return null;
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
