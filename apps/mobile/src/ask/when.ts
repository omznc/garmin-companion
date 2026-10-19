const DAY_MS = 24 * 60 * 60_000;

// "14:05" today, "Yesterday", or "3 Oct", in the words of the phone.
export function whenOf(iso: string, now: number): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "";
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  if (at.getTime() >= today.getTime()) {
    return at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }
  if (at.getTime() >= today.getTime() - DAY_MS) return "Yesterday";
  return at.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}
