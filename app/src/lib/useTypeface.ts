import { useEffect, useState } from "react";
import { applyTypeface, loadTypeface, type Typeface } from "./typeface";

export function useTypeface() {
  const [typeface, setTypeface] = useState<Typeface>(loadTypeface);

  useEffect(() => {
    applyTypeface(typeface);
  }, [typeface]);

  return { typeface, setTypeface };
}
