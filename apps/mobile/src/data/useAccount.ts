import { useQuery } from "@tanstack/react-query";
import { getSettings, getStatus } from "./account";

export const useStatus = () => useQuery({ queryKey: ["status"], queryFn: getStatus, retry: false });
export const useSettings = () =>
  useQuery({ queryKey: ["settings"], queryFn: getSettings, retry: false });
