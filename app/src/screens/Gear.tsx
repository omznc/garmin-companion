import { useQuery } from "@tanstack/react-query";
import { gearList, type GearRow } from "../lib/api";
import { colWidth, Empty, ErrorNote, Loading, PageHeader } from "../components/ui";
import { RefreshButton } from "../components/Refresh";
import { km, num } from "../lib/format";

export function Gear() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["gear"],
    queryFn: gearList,
    retry: false,
    staleTime: 10 * 60_000,
  });

  if (isLoading) return <Loading label="Asking Garmin about your gear" />;
  if (error) {
    return (
      <>
        <Header />
        <ErrorNote error={error} />
      </>
    );
  }

  const rows = data ?? [];

  return (
    <div className="screen">
      <Header space={rows.length ? 44 : 20} />

      {!rows.length && (
        <Empty
          title="No gear registered."
          body={
            <>
              Garmin tracks shoe and bike mileage once you add the item in Garmin Connect and attach
              it to your activities. Nothing is registered on this account yet, so there's no wear
              to report — this screen fills in by itself once there is.
            </>
          }
        />
      )}

      {rows.map((r) => (
        <GearItem key={r.gear.uuid} row={r} />
      ))}

      {rows.length > 0 && (
        <p style={{ fontSize: "var(--fs-base)", color: "var(--faint)", marginTop: 26 }}>
          Distances come from Garmin's own gear totals. The wear bar is against the retirement limit
          you set on each item — items without one show no bar rather than a guessed threshold.
        </p>
      )}
    </div>
  );
}

function GearItem({ row }: { row: GearRow }) {
  const { gear, stats } = row;
  const distance = stats?.totalDistance ?? 0;
  const limit = gear.maximumMeters ?? null;
  const pct = limit ? Math.min((distance / limit) * 100, 100) : null;

  // Only three states, and only when the user gave us a limit to judge against.
  const status =
    pct == null
      ? gear.gearStatusName === "retired"
        ? "Retired"
        : ""
      : pct >= 90
        ? "Retire soon"
        : pct >= 70
          ? "Watch"
          : "Good";
  const accent = pct != null && pct >= 90;

  return (
    <div style={{ padding: "22px 0", borderBottom: "1px solid var(--line2)" }}>
      <div className="cols" style={{ display: "flex", alignItems: "baseline", gap: 16 }}>
        <div className="col-name" style={{ fontSize: "var(--fs-lg)" }}>
          {gear.displayName ?? gear.customMakeModel ?? "Unnamed"}
          <span style={{ color: "var(--faint)", fontSize: "var(--fs-small)", marginLeft: 10 }}>
            {gear.gearTypeName ?? ""}
          </span>
        </div>
        <div className="serif" style={{ fontSize: 24 }}>
          {distance > 0 ? km(distance, 0) : "—"}
        </div>
        <div
          className="col"
          style={{
            ...colWidth(92),
            fontSize: "var(--fs-small)",
            color: accent ? "var(--acc)" : "var(--mut)",
          }}
        >
          {status}
        </div>
      </div>
      {pct != null && (
        <div className="bar" style={{ marginTop: 14 }}>
          <span style={{ width: `${pct}%`, background: accent ? "var(--acc)" : "var(--mut)" }} />
        </div>
      )}
      {stats?.totalActivities != null && (
        <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 10 }}>
          {num(stats.totalActivities)} activities
          {limit ? ` · limit ${km(limit, 0)}` : ""}
        </div>
      )}
    </div>
  );
}

function Header({ space }: { space?: number }) {
  return (
    <PageHeader
      eyebrow="Live from Garmin"
      title="Gear"
      lede="Shoes and bikes registered on your account, with the distance logged against each."
      action={<RefreshButton live />}
      space={space}
    />
  );
}
