/**
 * The sidebar's entries, and the order the user has put them in.
 *
 * Order is a preference rather than a constant because the top entry is also
 * the default screen — the one the app opens on. That makes "set as default"
 * and "move to the top" the same gesture, which is why there's no separate
 * stored default to keep in sync with the list.
 */
import {
  NavActivities,
  NavAsk,
  NavFood,
  NavGear,
  NavHealth,
  NavInsights,
  NavPlan,
  NavReports,
  NavRoutes,
  NavSettings,
  NavToday,
  NavWeight,
} from "./icons";

/**
 * The shipped order, which is also the fallback for anyone who has never
 * touched it. `as const` matters: TanStack derives the typed path union from
 * these literals, so `to` stays checked against the route tree.
 */
export const NAV = [
  { to: "/today", label: "Today", icon: NavToday },
  { to: "/activities", label: "Activities", icon: NavActivities },
  { to: "/health", label: "Health", icon: NavHealth },
  { to: "/food", label: "Food", icon: NavFood },
  { to: "/weight", label: "Weight", icon: NavWeight },
  { to: "/ask", label: "Ask", icon: NavAsk },
  { to: "/insights", label: "Insights", icon: NavInsights },
  { to: "/plan", label: "Plan", icon: NavPlan },
  { to: "/routes", label: "Routes", icon: NavRoutes },
  { to: "/gear", label: "Gear", icon: NavGear },
  { to: "/reports", label: "Reports", icon: NavReports },
  { to: "/settings", label: "Settings", icon: NavSettings },
] as const;

export type NavEntry = (typeof NAV)[number];
export type NavPath = NavEntry["to"];

const KEY = "garmin-companion:nav-order";

function entry(to: string): NavEntry | undefined {
  return NAV.find((n) => n.to === to);
}

/**
 * The stored order, reconciled against the entries this build actually has.
 *
 * Reconciling rather than trusting the stored list is what makes the
 * preference survive a release: a screen that has since been removed drops
 * out, and a screen that didn't exist when the order was saved is appended.
 * Appended, specifically — a new screen arriving at the top would silently
 * take over as the default.
 */
export function loadNavOrder(): NavEntry[] {
  let stored: unknown = null;
  try {
    stored = JSON.parse(localStorage.getItem(KEY) ?? "null");
  } catch {
    // A corrupt value is the same as no value; the shipped order is right.
  }

  const order: NavEntry[] = [];
  if (Array.isArray(stored)) {
    for (const to of stored) {
      const found = typeof to === "string" ? entry(to) : undefined;
      // Duplicates would render the same link twice and make the list one
      // entry short somewhere else.
      if (found && !order.includes(found)) order.push(found);
    }
  }
  for (const n of NAV) if (!order.includes(n)) order.push(n);
  return order;
}

export function saveNavOrder(order: readonly NavEntry[]): void {
  localStorage.setItem(KEY, JSON.stringify(order.map((n) => n.to)));
}

/** The screen the app opens on: whatever sits at the top of the nav. */
export function defaultRoute(): NavPath {
  return loadNavOrder()[0].to;
}

/** A copy of `list` with the item at `from` lifted out and dropped at `to`. */
export function move<T>(list: readonly T[], from: number, to: number): T[] {
  const next = list.slice();
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}
