// The one query client of the app. The Ask store reads it outside React.
import { QueryClient } from "@tanstack/react-query";
import { shouldRetry } from "./retry";

export const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: shouldRetry } },
});
