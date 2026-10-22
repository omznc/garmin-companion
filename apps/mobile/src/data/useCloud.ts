// The hosted coach account: the balance, the plan and the email sign-in.
// The core builds the sentences and keeps the tokens. The screens only draw.
// Purchase is not here: the stores come later, so the app has no checkout call.
import { useQuery } from "@tanstack/react-query";
import type { AccountView } from "./generated/AccountView";
import { call } from "./source";

// The account shape comes from the Rust core (`garmin_coach::AccountView`).
export type CloudAccount = AccountView;

export type Provider = "cloud" | "openrouter" | "ollama";

export const PREMIUM_LATER =
  "Premium is not on sale in the app yet. If you bought it on the desktop app, sign in here with the same email.";

// A bridge error reads "internal: <sentence>". The person needs the sentence.
export function sentence(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  return text.replace(/^(internal|cloud): /, "");
}

async function plain<T>(run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (error) {
    throw new Error(sentence(error));
  }
}

// Keep the last balance on screen when the network is gone. Only the coach page
// passes `enrol`: it may ask the server for an install id. The Ask tab only
// looks, so an app that is only open does not use up the daily install cap.
// The result is null before the first enrol.
export const useCloudAccount = (enabled: boolean, enrol = false) =>
  useQuery({
    queryKey: ["cloudAccount", enrol],
    queryFn: () => plain(() => call("cloud_account", { enrol })),
    enabled,
    retry: false,
    staleTime: 30_000,
  });

export const sendCode = (email: string) => plain(() => call("cloud_send_code", { email }));

export const CODE_HINT = "Check the code. It has 6 digits.";

export async function verifyCode(email: string, code: string): Promise<{ email: string }> {
  if (!/^\d{6}$/.test(code)) throw new Error(CODE_HINT);
  return plain(() => call("cloud_verify", { email, code }));
}

export const signOutCloud = () => plain(() => call("cloud_sign_out"));

export const chooseProvider = (provider: Exclude<Provider, "ollama">) =>
  plain(() => call("ask_save_config", { provider }));
