import { useEffect, useState } from "react";
import { AppState } from "react-native";
import { greeting } from "./present";

// The greeting follows the clock. A same-day refetch gives equal data, so the
// screen would not render again. The timer and the resume event do it.
export function useGreeting(): string {
  const [text, setText] = useState(() => greeting());
  useEffect(() => {
    const update = () => setText(greeting());
    const timer = setInterval(update, 60_000);
    const sub = AppState.addEventListener("change", (s) => s === "active" && update());
    return () => {
      clearInterval(timer);
      sub.remove();
    };
  }, []);
  return text;
}
