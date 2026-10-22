// The tiles, extras and charts views of the core, for the date focus.
// The presenters are in `viewsPresent.ts`.
import { useQuery } from "@tanstack/react-query";
import { focusArgs, focusKey, useFocus } from "./focus";
import { call } from "./source";

export * from "./viewsPresent";

export const useTiles = () => {
  const focus = useFocus();
  return useQuery({
    queryKey: ["tiles", focusKey(focus)],
    queryFn: () => call("tiles_view", focusArgs(focus)),
    retry: false,
  });
};
