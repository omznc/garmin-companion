import { Link, useRouterState } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { cacheSummary, garminProfile, syncNow } from "../lib/api";
import { since } from "../lib/format";
import { useTheme } from "../lib/useTheme";

const NAV = [
  { to: "/", label: "Today" },
  { to: "/activities", label: "Activities" },
  { to: "/health", label: "Health" },
  { to: "/food", label: "Food" },
  { to: "/ask", label: "Ask" },
  { to: "/insights", label: "Insights" },
  { to: "/plan", label: "Plan" },
  { to: "/routes", label: "Routes" },
  { to: "/gear", label: "Gear" },
  { to: "/reports", label: "Reports" },
  { to: "/settings", label: "Settings" },
] as const;

export function Sidebar() {
  const { theme, cycle, label: themeLabel } = useTheme();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const qc = useQueryClient();

  // The recent sync, not the full one — this is the button you press because
  // you just finished a session. A full re-sync stays in Settings, where its
  // cost is spelled out.
  const sync = useMutation({
    mutationFn: () => syncNow(30, false),
    onSuccess: () => qc.invalidateQueries(),
  });

  // Both are cheap and cached; a stale name in the corner is not worth a
  // spinner, so neither blocks render.
  const profile = useQuery({
    queryKey: ["profile"],
    queryFn: garminProfile,
    staleTime: Infinity,
    retry: false,
  });
  const cache = useQuery({
    queryKey: ["cacheSummary"],
    queryFn: cacheSummary,
    refetchInterval: 30_000,
  });

  const who = profile.data?.fullName ?? profile.data?.displayName;
  const synced = (cache.data?.activities ?? 0) > 0;

  return (
    <nav
      style={{
        width: 214,
        flex: "none",
        padding: "38px 26px 40px 34px",
        position: "sticky",
        top: 0,
        height: "100vh",
        display: "flex",
        flexDirection: "column",
        gap: 2,
      }}
    >
      <div className="serif" style={{ fontSize: 19, lineHeight: 1.15, marginBottom: 4 }}>
        Companion
      </div>
      <div
        style={{
          font: "400 10.5px/1.4 'Instrument Sans', sans-serif",
          letterSpacing: "0.1em",
          textTransform: "uppercase",
          color: "var(--faint)",
          marginBottom: 26,
        }}
      >
        Garmin{who ? ` · ${who}` : ""}
      </div>

      {NAV.map((n) => {
        // "/" would otherwise match everything, and /activities/123 should keep
        // the Activities entry lit.
        const active = n.to === "/" ? path === "/" : path.startsWith(n.to);
        return (
          <Link
            key={n.to}
            to={n.to}
            style={{
              fontSize: 13.5,
              lineHeight: 1.2,
              padding: "6.5px 0",
              color: active ? "var(--fg)" : "var(--mut)",
              fontWeight: active ? 500 : 400,
              transition: "color .18s",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.color = "var(--fg)")}
            onMouseLeave={(e) =>
              (e.currentTarget.style.color = active ? "var(--fg)" : "var(--mut)")
            }
          >
            {n.label}
          </Link>
        );
      })}

      <div style={{ flex: 1 }} />

      <button
        className="quiet"
        onClick={() => sync.mutate()}
        disabled={sync.isPending}
        style={{
          fontSize: 11.5,
          padding: "3px 0",
          color: sync.isPending ? "var(--faint)" : "var(--mut)",
          cursor: sync.isPending ? "default" : "pointer",
        }}
        title="Pull the last 30 days from Garmin"
      >
        {sync.isPending ? "Syncing…" : "Sync"}
      </button>
      {/* Under the button rather than in Settings only: the question you have
          when looking at a Sync button is whether you still need to press it. */}
      <div style={{ fontSize: 10.5, color: "var(--faint)", padding: "1px 0 3px", lineHeight: 1.35 }}>
        {sync.isError
          ? "Sync failed. See Settings."
          : cache.data?.lastSync
            ? `Synced ${since(cache.data.lastSync)}`
            : "Never synced"}
      </div>

      <button
        className="quiet"
        onClick={cycle}
        style={{ fontSize: 11.5, padding: "3px 0" }}
        title={`Appearance: ${theme}`}
      >
        {themeLabel}
      </button>

      <div
        style={{
          fontSize: 11.5,
          color: "var(--faint)",
          padding: "3px 0",
          display: "flex",
          alignItems: "center",
          gap: 7,
        }}
      >
        <span
          style={{
            width: 5,
            height: 5,
            borderRadius: "50%",
            background: synced ? "var(--acc)" : "var(--line)",
            display: "inline-block",
          }}
        />
        {synced ? `${cache.data?.activities.toLocaleString()} cached` : "Cache empty"}
      </div>
    </nav>
  );
}
