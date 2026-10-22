// The one query client of the app. The Ask store reads it outside React.
import { QueryClient } from "@tanstack/react-query";
import { shouldRetry } from "./retry";

// A sync and the screens that save data invalidate the keys that they change.
// So a mount or a return to the foreground reads the cache for 60 seconds.
export const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: shouldRetry, staleTime: 60_000 } },
});
