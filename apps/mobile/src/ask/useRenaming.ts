import { useState } from "react";
import type { ThreadMeta } from "../data/chatTypes";
import { renameThread } from "./useThreads";

// The chat that the rename dialog edits, and the save. `save` resolves with an error
// sentence, or null when the name was saved.
export function useRenaming() {
  const [renaming, setRenaming] = useState<ThreadMeta | null>(null);
  const save = async (title: string) => {
    if (!renaming) return null;
    try {
      await renameThread(renaming.sessionId, title);
      setRenaming(null);
      return null;
    } catch (error) {
      return (error as Error).message;
    }
  };
  return { renaming, begin: setRenaming, cancel: () => setRenaming(null), save };
}
