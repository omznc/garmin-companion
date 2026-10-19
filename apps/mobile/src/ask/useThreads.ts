// The Ask threads for the screens: the list, one thread with the live turn over it,
// and the actions. The core keeps the threads, so nothing here saves a turn.
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import type { Thread, ThreadMeta, Turn } from "../data/chatTypes";
import { queryClient } from "../data/queryClient";
import { callJson } from "../data/source";
import { dropLive, hasLive, startTurn, threadKey, threadsKey, useLive, useRunningId } from "./runs";
import { historyFor, overlay } from "./turns";

const LIST_LIMIT = 200;
const POLL_MS = 1000;

export const useThreadList = () =>
  useQuery({
    queryKey: threadsKey,
    queryFn: async () =>
      (await callJson<{ threads: ThreadMeta[] }>("chat_list", { limit: LIST_LIMIT })).threads,
  });

export function useThread(sessionId: string | null) {
  const live = useLive(sessionId);
  const query = useQuery({
    queryKey: threadKey(sessionId ?? ""),
    enabled: sessionId !== null,
    queryFn: () => callJson<Thread | null>("chat_get", { sessionId }),
    // A turn that runs with no live copy here (the app reloaded) is read again and again.
    refetchInterval: (q) => (q.state.data?.running && !hasLive(sessionId ?? "") ? POLL_MS : false),
  });
  const turns: Turn[] = overlay(query.data?.turns ?? [], live);
  return {
    title: query.data?.title ?? null,
    turns,
    loading: sessionId !== null && query.isPending && !live,
    // The thread was read and is not there: deleted.
    gone: sessionId !== null && query.data === null && !live,
    error: query.error as Error | null,
    refetch: query.refetch,
  };
}

// True when the message went to the core. On false the composer takes the text back.
export async function sendMessage(sessionId: string, text: string): Promise<boolean> {
  try {
    // Read the saved thread now: the cache can be a turn behind.
    const stored = (await callJson<Thread | null>("chat_get", { sessionId }))?.turns ?? [];
    return await startTurn(sessionId, stored.length, text, historyFor(stored, text));
  } catch {
    return false;
  }
}

export async function renameThread(sessionId: string, title: string) {
  await callJson("chat_rename", { sessionId, title });
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: threadsKey }),
    queryClient.invalidateQueries({ queryKey: threadKey(sessionId) }),
  ]);
}

export async function deleteThread(sessionId: string) {
  await callJson("chat_delete", { sessionId });
  dropLive(sessionId);
  // removeQueries does not tell the screens. The null tells them the thread is gone.
  queryClient.setQueryData(threadKey(sessionId), null);
  // A late "done" of a deleted running turn is dropped, so read the balance here.
  void queryClient.invalidateQueries({ queryKey: ["cloudAccount"] });
  await queryClient.invalidateQueries({ queryKey: threadsKey });
}

// The list rows for the history screen. The live store knows the running thread before
// the list does, so it wins while a turn runs.
export function useThreadRows() {
  const query = useThreadList();
  const runningId = useRunningId();
  const [refreshing, setRefreshing] = useState(false);
  const rows = (query.data ?? []).map((thread) => ({
    ...thread,
    running: runningId === null ? thread.running : thread.sessionId === runningId,
  }));
  return {
    rows,
    loading: query.isPending,
    error: query.error as Error | null,
    refetch: query.refetch,
    refreshing,
    onRefresh: () => {
      setRefreshing(true);
      void query.refetch().finally(() => setRefreshing(false));
    },
  };
}
