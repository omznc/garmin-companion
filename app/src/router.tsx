/**
 * Code-based routes. There is no server and no file-system routing here — a
 * desktop app has one window, so the tree is small enough to read in one go.
 *
 * Hash history is deliberate: the webview loads from a file URL in a packaged
 * build, where path-based history has no origin to push against.
 */
import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
} from "@tanstack/react-router";
import { Sidebar } from "./components/Sidebar";
import { Today } from "./screens/Today";
import { Activities } from "./screens/Activities";
import { ActivityDetail } from "./screens/ActivityDetail";
import { Health } from "./screens/Health";
import { Ask } from "./screens/Ask";
import { Insights } from "./screens/Insights";
import { Gear } from "./screens/Gear";
import { Reports } from "./screens/Reports";
import { Settings } from "./screens/Settings";
import { Food } from "./screens/Food";
import { Plan } from "./screens/Plan";
import { Routes } from "./screens/Routes";

/**
 * On an ultrawide the sidebar would otherwise sit pinned to the far left with
 * the reading column stranded somewhere off-centre, leaving the two ends of the
 * app a monitor apart. Capping the pair and centring them together keeps the
 * nav next to what it navigates, however wide the window gets.
 */
const SHELL_MAX = 1240;

function Shell() {
  return (
    <div
      style={{
        minHeight: "100vh",
        background: "var(--bg)",
        color: "var(--fg)",
        display: "flex",
        justifyContent: "center",
      }}
    >
      <div style={{ display: "flex", width: "100%", maxWidth: SHELL_MAX }}>
        <Sidebar />
        <main style={{ flex: 1, minWidth: 0, padding: "0 56px 160px" }}>
          <div style={{ maxWidth: 720, margin: "0 auto", paddingTop: 78 }}>
            <Outlet />
          </div>
        </main>
      </div>
    </div>
  );
}

const rootRoute = createRootRoute({ component: Shell });

// Written out one by one rather than through a helper: TanStack derives the
// typed path union from these literals, and a helper taking `path: string`
// erases it, which costs every `<Link to>` in the app its type checking.
const routeTree = rootRoute.addChildren([
  createRoute({ getParentRoute: () => rootRoute, path: "/", component: Today }),
  createRoute({ getParentRoute: () => rootRoute, path: "/activities", component: Activities }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/activities/$activityId",
    component: ActivityDetail,
  }),
  createRoute({ getParentRoute: () => rootRoute, path: "/health", component: Health }),
  createRoute({ getParentRoute: () => rootRoute, path: "/food", component: Food }),
  createRoute({ getParentRoute: () => rootRoute, path: "/ask", component: Ask }),
  createRoute({ getParentRoute: () => rootRoute, path: "/insights", component: Insights }),
  createRoute({ getParentRoute: () => rootRoute, path: "/plan", component: Plan }),
  createRoute({ getParentRoute: () => rootRoute, path: "/routes", component: Routes }),
  createRoute({ getParentRoute: () => rootRoute, path: "/gear", component: Gear }),
  createRoute({ getParentRoute: () => rootRoute, path: "/reports", component: Reports }),
  createRoute({ getParentRoute: () => rootRoute, path: "/settings", component: Settings }),
]);

export const router = createRouter({
  routeTree,
  history: createHashHistory(),
  defaultPreload: false,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
