// The hosted coach account: the balance, the plan and the email sign-in.
// The core builds the sentences and keeps the tokens. The screens only draw.
// Purchase is not here: the stores come later, so the app has no checkout call.
import { useQuery } from "@tanstack/react-query";
import { callJson } from "./source";

export type CloudAccount = {
  premium: boolean;
  email: string | null;
  creditsRemaining: number;
  /** The small line near the composer. */
  creditsLine: string;
  planLine: string;
  /** No credits left: show the upgrade card. */
  out: boolean;
  /** Under a fifth of the daily grant: show a quiet line only. */
  low: boolean;
};

export type Provider = "cloud" | "openrouter" | "ollama";

export const PREMIUM_LATER =
  "Premium is not on sale in the app yet. If you bought it on the desktop app, sign in here with the same email.";

// A bridge error reads "internal: <sentence>". The person needs the sentence.
export function sentence(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  return text.replace(/^(internal|cloud): /, "");
}

async function plain<T>(cmd: string, args?: unknown): Promise<T> {
  try {
    return await callJson<T>(cmd, args);
  } catch (error) {
    throw new Error(sentence(error));
  }
}

// Keep the last balance on screen when the network is gone.
export const useCloudAccount = (enabled: boolean) =>
  useQuery({
    queryKey: ["cloudAccount"],
    queryFn: () => plain<CloudAccount>("cloud_account"),
    enabled,
    retry: false,
    staleTime: 30_000,
  });

export const sendCode = (email: string) => plain("cloud_send_code", { email });

export const verifyCode = (email: string, code: string) =>
  plain<{ email: string }>("cloud_verify", { email, code });

export const signOutCloud = () => plain("cloud_sign_out");

export const chooseProvider = (provider: Exclude<Provider, "ollama">) =>
  plain("ask_save_config", { provider });
