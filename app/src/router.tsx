/**
 * Code-based routes. There is no server and no file-system routing here — a
 * desktop app has one window, so the tree is small enough to read in one go.
 *
 * Hash history is deliberate: the webview loads from a file URL in a packaged
 * build, where path-based history has no origin to push against.
 */
import { useEffect, useLayoutEffect, useRef } from "react";
import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  useRouterState,
} from "@tanstack/react-router";
import { defaultRoute } from "./lib/nav";
import { scroller } from "./lib/scroller";
import { IS_MOBILE } from "./lib/platform";
import { Sidebar, SIDEBAR_W } from "./components/Sidebar";
import { TabBar } from "./components/TabBar";
import { ScrollFade } from "./components/ScrollFade";
import { PullRefresh } from "./components/PullRefresh";
import { Today } from "./screens/Today";
import { Activities } from "./screens/Activities";
import { ActivityDetail } from "./screens/ActivityDetail";
import { Health } from "./screens/Health";
import { Sleep } from "./screens/Sleep";
import { Ask } from "./screens/Ask";
import { Insights } from "./screens/Insights";
import { Gear } from "./screens/Gear";
import { Reports } from "./screens/Reports";
import { Settings } from "./screens/Settings";
import { Food } from "./screens/Food";
import { Weight } from "./screens/Weight";
import { Plan } from "./screens/Plan";
import { Routes } from "./screens/Routes";
import { Strength } from "./screens/Strength";
import { Fitness } from "./screens/Fitness";

/**
 * On an ultrawide the sidebar would otherwise sit pinned to the far left with
 * the reading column stranded somewhere off-centre, leaving the two ends of the
 * app a monitor apart. Capping the pair and centring them together keeps the
 * nav next to what it navigates, however wide the window gets.
 */
const SHELL_MAX = 1240;

function Shell() {
  return IS_MOBILE ? <MobileShell /> : <DesktopShell />;
}

function DesktopShell() {
  const shell = useRef<HTMLDivElement>(null);

  return (
    <div
      ref={shell}
      style={{
        minHeight: "100vh",
        background: "var(--bg)",
        color: "var(--fg)",
        display: "flex",
        justifyContent: "center",
      }}
    >
      {/* Fixed, so it's positioned against the window rather than this box —
          hence the centring being repeated as a calc. It covers the reading
          column only: the nav beside it is sticky and never moves. */}
      <ScrollFade
        left={`calc(max(0px, (100vw - ${SHELL_MAX}px) / 2) + ${SIDEBAR_W}px)`}
        track={shell}
      />
      <div style={{ display: "flex", width: "100%", maxWidth: SHELL_MAX }}>
        <Sidebar />
        <main style={{ flex: 1, minWidth: 0, padding: "0 56px 160px" }}>
          {/* Left-aligned, not centred: centring made the gap to the nav grow
              with the window, so the two drifted apart on a wide screen while
              sitting a fixed 56px apart on a narrow one. */}
          <div style={{ maxWidth: 720, paddingTop: 78 }}>
            <Page />
          </div>
        </main>
      </div>
    </div>
  );
}

/**
 * One column, and the nav along the bottom.
 *
 * `ScrollFade` renders here too. It used not to, on the reasoning that the fade
 * exists to soften content passing under the window's top strip and a phone has
 * no strip — but Android is edge-to-edge, so the status bar draws *over* the
 * webview rather than above it, which is the same fact the top padding below is
 * built out of. Without the fade, a heading scrolling up doesn't end: it carries
 * on behind the clock and the battery until it runs out of screen.
 *
 * Only the top one, though. At the bottom the tab bar is opaque, full-width and
 * carries a hairline of its own, so the page already ends against a surface
 * rather than at a cut — `ScrollFade` draws nothing there.
 *
 * The padding is in CSS rather than inline like the desktop shell's, because it
 * has to compose with `env(safe-area-inset-*)` — a notch, a punch-hole, the
 * gesture bar — and those are only readable from a stylesheet. The fade's own
 * edge is placed there for the same reason.
 */
function MobileShell() {
  const shell = useRef<HTMLDivElement>(null);

  return (
    <div className="shell-mobile" ref={shell}>
      {/* Full width: there is no nav beside the column to clear. */}
      <ScrollFade left="0" track={shell} />
      {/* Here rather than per screen: it is a property of the shell, and which
          screens answer it is decided by whether they publish a refresh — see
          `lib/refreshable`. */}
      <PullRefresh />
      <main>
        <Page />
      </main>
      <TabBar />
    </div>
  );
}

/**
 * The screen, plus the fact that it's a new one.
 *
 * Keyed on the path so the entrance keyframe restarts on every navigation —
 * a CSS animation only runs when the node is new, and React would otherwise
 * reuse this div across the swap and play nothing. The key is the path alone,
 * so a search-param change (a range picker, a filter) leaves the node in place
 * and doesn't re-announce a screen you're already reading.
 *
 * Scroll goes with it, in a layout effect so the reset lands in the same paint
 * as the new screen. Deep in a long activity list and opening one of them,
 * you'd otherwise arrive at a detail page already scrolled past its title —
 * and now that the column fades in, land there mid-fade as well.
 */
function Page() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });

  useLayoutEffect(() => {
    scroller().scrollTop = 0;
  }, [pathname]);

  return (
    <div key={pathname} className="page">
      <Outlet />
    </div>
  );
}

const rootRoute = createRootRoute({ component: Shell });

/**
 * The window opens on `#/`, which belongs to no screen: the first one is
 * whichever the user has put at the top of the nav, and that's a preference
 * rather than a route. Today has its own path like everything else, so that
 * preference can name it without `/` meaning two things at once.
 *
 * Rewritten on the history rather than through a route redirect so the very
 * first render is already the right screen — and rewritten, not pushed, so
 * Back from the opening screen leaves the app instead of landing here again.
 */
const history = createHashHistory();
if (history.location.pathname === "/") history.replace(defaultRoute());

/** The same hop for anything that reaches `/` later in the session. */
function DefaultScreen() {
  useEffect(() => {
    history.replace(defaultRoute());
  }, []);
  return null;
}

// Written out one by one rather than through a helper: TanStack derives the
// typed path union from these literals, and a helper taking `path: string`
// erases it, which costs every `<Link to>` in the app its type checking.
const routeTree = rootRoute.addChildren([
  createRoute({ getParentRoute: () => rootRoute, path: "/", component: DefaultScreen }),
  createRoute({ getParentRoute: () => rootRoute, path: "/today", component: Today }),
  createRoute({ getParentRoute: () => rootRoute, path: "/activities", component: Activities }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/activities/$activityId",
    component: ActivityDetail,
  }),
  createRoute({ getParentRoute: () => rootRoute, path: "/health", component: Health }),
  createRoute({ getParentRoute: () => rootRoute, path: "/sleep", component: Sleep }),
  createRoute({ getParentRoute: () => rootRoute, path: "/food", component: Food }),
  createRoute({ getParentRoute: () => rootRoute, path: "/weight", component: Weight }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/ask",
    component: Ask,
    // `?q=` seeds the composer, so a screen can hand a question over with the
    // subject already filled in. Validated rather than trusted: search params
    // survive a reload and are the one part of the URL a person can type into.
    validateSearch: (search: Record<string, unknown>) => ({
      q: typeof search.q === "string" ? search.q.slice(0, 400) : undefined,
    }),
  }),
  createRoute({ getParentRoute: () => rootRoute, path: "/insights", component: Insights }),
  createRoute({ getParentRoute: () => rootRoute, path: "/strength", component: Strength }),
  createRoute({ getParentRoute: () => rootRoute, path: "/fitness", component: Fitness }),
  createRoute({ getParentRoute: () => rootRoute, path: "/plan", component: Plan }),
  createRoute({ getParentRoute: () => rootRoute, path: "/routes", component: Routes }),
  createRoute({ getParentRoute: () => rootRoute, path: "/gear", component: Gear }),
  createRoute({ getParentRoute: () => rootRoute, path: "/reports", component: Reports }),
  createRoute({ getParentRoute: () => rootRoute, path: "/settings", component: Settings }),
]);

export const router = createRouter({
  routeTree,
  history,
  defaultPreload: false,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
