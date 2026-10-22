// The coach settings and the opening questions of the Ask screen.
import { useQuery } from "@tanstack/react-query";
import type { ChatConfig } from "./generated/ChatConfig";
import { call, isSampleData } from "./source";

// Sample mode stores no secret, so the key field is hidden and this line takes its place.
export const SAMPLE_KEY_NOTE =
  "This build shows sample data. The coach key is saved only after you sign in.";

// Questions for an empty thread. None of them names a sport.
export const OPENERS = [
  "How did I sleep this week?",
  "Am I recovered enough for a hard day?",
  "Is my resting heart rate moving?",
  "What does my HRV say about this week?",
  "How active was I compared with last month?",
  "How is my weight trending?",
  "How did my last activity go?",
  "What is worth my attention today?",
];

export function openers(): string[] {
  const day = Math.floor(Date.now() / 86_400_000);
  return [0, 3, 6].map((i) => OPENERS[(day + i) % OPENERS.length] ?? "");
}

// The shape comes from the Rust core (`garmin_coach::ChatConfig`).
export type CoachConfig = ChatConfig;

// Whether the coach can answer: the hosted coach, an OpenRouter key, or a local model.
export function needsSetup(config: CoachConfig | undefined): boolean {
  if (!config) return false;
  if (config.provider === "cloud" || config.provider === "ollama") return false;
  return !(config.provider === "openrouter" && config.hasKey);
}

export async function saveKey(key: string): Promise<void> {
  // The bridge drops the key in sample mode. Saving would still flip the provider.
  if (isSampleData()) return;
  await call("ask_save_config", { provider: "openrouter", key });
}

export const useCoachConfig = () =>
  useQuery({
    queryKey: ["askConfig"],
    queryFn: () => call("ask_config"),
    retry: false,
    // A provider change does not refetch it, so it must read fresh at each mount.
    staleTime: 0,
  });
