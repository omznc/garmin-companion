// The one query client of the app. The Ask store reads it outside React.
import { QueryClient } from "@tanstack/react-query";

export const queryClient = new QueryClient();
