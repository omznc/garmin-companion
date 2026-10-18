import { useQuery } from "@tanstack/react-query";
import { fetchToday } from "./source";

export const useToday = () => useQuery({ queryKey: ["today"], queryFn: fetchToday, retry: false });
