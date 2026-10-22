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

export const useExtras = () => {
  const focus = useFocus();
  return useQuery({
    queryKey: ["extras", focusKey(focus)],
    queryFn: () => call("extras_view", focusArgs(focus)),
    retry: false,
  });
};

export const useCharts = () => {
  const focus = useFocus();
  return useQuery({
    queryKey: ["charts", focusKey(focus)],
    queryFn: () => call("charts_view", focusArgs(focus)),
    retry: false,
  });
};
