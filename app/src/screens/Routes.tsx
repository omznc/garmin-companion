/**
 * Routes, grouped from cached GPS traces.
 *
 * The traces are drawn as bare shapes rather than on a map: there is no tile
 * layer in this app and no network call worth making for one, and the shape
 * plus the distance is what identifies a route to the person who ran it.
 *
 * Worth knowing while reading this screen — none of the athlete's runs carry
 * GPS. Every trace here is a ride or a walk, which the screen says outright
 * rather than quietly presenting rides as running routes.
 */
import { useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  routes,
  type Route,
  type RouteOuting,
  type RouteSort,
} from "../lib/api";
import {
  Empty,
  ErrorNote,
  Loading,
  Metric,
  MetricRow,
  PageHeader,
  Rule,
} from "../components/ui";
import { RefreshButton } from "../components/Refresh";
import { DASH, duration, isRun, km, longDate, parseLocal, sportLabel } from "../lib/format";

export function Routes() {
  const [sort, setSort] = useState<RouteSort>("recent");
  const { data, isLoading, error } = useQuery({
    queryKey: ["routes", sort],
    queryFn: () => routes(sort),
    // Changing the order is a re-query, since the traces come with it. Holding
    // the previous list keeps the page from collapsing to a spinner each time.
    placeholderData: keepPreviousData,
  });
  const [shown, setShown] = useState(PAGE);

  function reorder(next: RouteSort) {
    setSort(next);
    // A new order makes the old scroll depth meaningless — showing 60 routes
    // by recency and then switching to longest-first would open on the tail.
    setShown(PAGE);
  }

  if (isLoading) return <Loading />;
  if (error) return <ErrorNote error={error} />;

  const all = data ?? [];

  if (!all.length) {
    return (
      <div>
        <Header />
        <Empty
          title="No GPS traces cached."
          body={
            <>
              Routes are matched from the trace on each activity. Nothing in the
              cache has one yet — run a sync from Settings and any activity
              Garmin recorded outdoors will be fetched.
            </>
          }
          action={
            <Link className="cta" to="/activities">
              Browse activities
            </Link>
          }
        />
      </div>
    );
  }

  const repeated = all.filter((r) => r.times > 1);
  const outings = all.reduce((n, r) => n + r.times, 0);
  const anyRuns = all.some((r) => isRun(r.typeKey));

  return (
    <div className="screen">
      <Header />

      <MetricRow style={{ marginBottom: 10 }}>
        <Metric value={all.length} label="Distinct routes" />
        <Metric value={outings} label="Outings" />
        <Metric value={repeated.length} label="Ridden more than once" />
      </MetricRow>

      {repeated.length === 0 && (
        <p style={{ fontSize: "var(--fs-md)", lineHeight: 1.7, color: "var(--mut)", margin: "0 0 8px", maxWidth: "62ch", textWrap: "pretty" }}>
          Nothing repeats yet — every trace you have starts or finishes
          somewhere different, or covers a different distance. These are one-off
          journeys rather than a route you keep coming back to.
        </p>
      )}

      {!anyRuns && (
        <p style={{ fontSize: "var(--fs-md)", lineHeight: 1.7, color: "var(--mut)", margin: "0 0 8px", maxWidth: "62ch", textWrap: "pretty" }}>
          None of these are runs — every trace here is a ride or a walk. A
          treadmill records no position at all, so those sessions can never
          appear. An outdoor run would show up here, and would start VO2 max
          tracking too.
        </p>
      )}

      <Rule m="46px 0 18px" />
      <SortTabs sort={sort} onChange={reorder} />
      <div>
        {all.slice(0, shown).map((r) => (
          <RouteCard key={r.outings[0].activityId} route={r} />
        ))}
      </div>
      {shown < all.length && (
        <button
          className="underlined"
          style={{ marginTop: 26, fontSize: "var(--fs-small)" }}
          onClick={() => setShown((n) => n + PAGE)}
        >
          Show {Math.min(PAGE, all.length - shown)} more
        </button>
      )}
      {/* Traces are only loaded for the most-repeated routes, so say so rather
          than leave a run of empty thumbnails unexplained. */}
      {all.slice(0, shown).some((r) => r.outings[0].points.length < 2) && (
        <p style={{ fontSize: "var(--fs-small)", color: "var(--faint)", marginTop: 22, maxWidth: "58ch" }}>
          Only the first 40 routes in this order carry a drawn trace — loading
          every one at once is what used to take the window down. Re-sorting
          re-picks which 40 those are.
        </p>
      )}
    </div>
  );
}

const SORTS: { key: RouteSort; label: string }[] = [
  { key: "recent", label: "Latest" },
  { key: "repeats", label: "Most repeated" },
  { key: "distance", label: "Longest" },
];

/**
 * Plain text, no boxes — the same restraint the rest of the app shows. The
 * active order is the only one at full contrast, which is enough to read as
 * selected without a control drawn around it.
 */
function SortTabs({
  sort,
  onChange,
}: {
  sort: RouteSort;
  onChange: (sort: RouteSort) => void;
}) {
  return (
    <div style={{ display: "flex", gap: 18, alignItems: "baseline", marginBottom: 8 }}>
      <span className="eyebrow" style={{ marginRight: 2 }}>
        Sort
      </span>
      {SORTS.map((s) => {
        const on = s.key === sort;
        return (
          <button
            key={s.key}
            aria-pressed={on}
            onClick={() => onChange(s.key)}
            style={{
              fontSize: "var(--fs-small)",
              cursor: "pointer",
              color: on ? "var(--fg)" : "var(--faint)",
              borderBottom: `1px solid ${on ? "var(--acc)" : "transparent"}`,
              paddingBottom: 3,
              transition: "color var(--dur-base)",
            }}
          >
            {s.label}
          </button>
        );
      })}
    </div>
  );
}

function Header() {
  return (
    <PageHeader
      eyebrow="All cached traces"
      title="Routes"
      lede="Every route you have a GPS trace for. Outings that start and finish in the same place and cover a similar distance are folded into one route."
      action={<RefreshButton />}
      space={46}
    />
  );
}

function RouteCard({ route: r }: { route: Route }) {
  const first = r.outings[0];
  return (
    <div style={{ display: "flex", gap: 26, alignItems: "flex-start", padding: "22px 0", borderBottom: "1px solid var(--line2)" }}>
      <Trace points={first.points} />

      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", justifyContent: "space-between", gap: 16, alignItems: "baseline" }}>
          <span style={{ fontSize: "var(--fs-lg)" }}>{first.name ?? "Untitled"}</span>
          <span className="mono" style={{ flex: "none" }}>
            {r.avgDistanceM ? km(r.avgDistanceM) : DASH}
          </span>
        </div>

        <div style={{ fontSize: "var(--fs-small)", color: "var(--faint)", marginTop: 5 }}>
          {sportLabel(r.typeKey)}
          {" · "}
          {r.times === 1 ? "once" : `${r.times} times`}
        </div>

        {/* Only worth listing the individual outings when there's more than
            one — otherwise the card would just repeat itself. */}
        {r.times > 1 && (
          <div style={{ marginTop: 12 }}>
            {r.outings.map((o) => (
              <OutingRow key={o.activityId} outing={o} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function OutingRow({ outing: o }: { outing: RouteOuting }) {
  const d = o.localDate ? parseLocal(o.localDate) : null;
  return (
    <Link
      to="/activities/$activityId"
      params={{ activityId: String(o.activityId) }}
      style={{
        display: "flex",
        justifyContent: "space-between",
        gap: 14,
        fontSize: "var(--fs-small)",
        color: "var(--mut)",
        padding: "4px 0",
      }}
    >
      <span>{d ? longDate(d) : DASH}</span>
      <span className="mono">
        {o.distanceM ? km(o.distanceM) : DASH}
        {o.durationS ? ` · ${duration(o.durationS)}` : ""}
      </span>
    </Link>
  );
}

const BOX = 78;

/** How many routes are rendered before "Show more". Each one is an SVG path. */
const PAGE = 12;

/** Points drawn per thumbnail. Above this the extra detail is sub-pixel. */
const MAX_TRACE_POINTS = 90;

/**
 * The trace as a plain shape. Latitude is flipped because screen y grows
 * downward, and longitude is squeezed by cos(lat) so the drawing keeps the
 * proportions the route actually has on the ground rather than stretching to
 * fill the box.
 */
function Trace({ points: raw }: { points: [number, number][] }) {
  if (raw.length < 2) {
    return <div style={{ width: BOX, height: BOX, flex: "none" }} />;
  }

  // A 78px thumbnail cannot show 400 points, and drawing them anyway is what
  // made a full page of traces expensive. Every nth point, endpoints kept.
  const step = Math.ceil(raw.length / MAX_TRACE_POINTS);
  const points =
    step > 1
      ? [...raw.filter((_, i) => i % step === 0), raw[raw.length - 1]]
      : raw;

  const lats = points.map((p) => p[0]);
  const lons = points.map((p) => p[1]);
  const minLat = Math.min(...lats);
  const maxLat = Math.max(...lats);
  const minLon = Math.min(...lons);
  const maxLon = Math.max(...lons);

  const midLat = (minLat + maxLat) / 2;
  const spanLat = Math.max(maxLat - minLat, 1e-6);
  const spanLon = Math.max((maxLon - minLon) * Math.cos((midLat * Math.PI) / 180), 1e-6);
  // One scale for both axes keeps the aspect ratio honest; the larger span
  // decides it so the whole route fits.
  const scale = (BOX - 8) / Math.max(spanLat, spanLon);

  const d = points
    .map((p, i) => {
      const x = (BOX - spanLon * scale) / 2 + (p[1] - minLon) * Math.cos((midLat * Math.PI) / 180) * scale;
      const y = (BOX - spanLat * scale) / 2 + (maxLat - p[0]) * scale;
      return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");

  return (
    <svg width={BOX} height={BOX} viewBox={`0 0 ${BOX} ${BOX}`} style={{ flex: "none" }} aria-hidden="true">
      <path
        d={d}
        fill="none"
        stroke="var(--acc)"
        strokeWidth={1.2}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}
