import { useQuery } from "@tanstack/react-query";
import { focusKey, useFocus } from "./focus";
import { fetchToday } from "./source";

export const useToday = () => {
  const focus = useFocus();
  return useQuery({
    queryKey: ["today", focusKey(focus)],
    queryFn: () => fetchToday(focus),
    retry: false,
  });
};
