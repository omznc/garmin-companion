// Keep the same rule as `garmin_api::session::ticket_in`.
// The sign-in ticket works once, so the app stops the navigation that has it.
// The host must be garmin.com or end with ".garmin.com". "notgarmin.com" does not match.
const LINK = /^[a-z][a-z0-9+.-]*:\/\/(?:[^/?#@]*@)?([^/?#:]+)(?::\d*)?(?:[/?#]|$)/i;

export function ticketIn(link: string): string | null {
  const host = LINK.exec(link)?.[1]?.toLowerCase();
  if (!host || (host !== "garmin.com" && !host.endsWith(".garmin.com"))) return null;
  const query = link.split("#")[0]?.split("?")[1] ?? "";
  for (const pair of query.split("&")) {
    const [key, value = ""] = pair.split("=");
    if (key !== "ticket") continue;
    try {
      const text = decodeURIComponent(value.replace(/\+/g, " "));
      if (text) return text;
    } catch {
      if (value) return value;
    }
  }
  return null;
}
