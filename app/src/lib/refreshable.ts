/**
 * Whether the screen you're on can be refreshed, and how.
 *
 * A phone expects to pull the top of a page down to reload it, and that gesture
 * has no button to hang off — it happens on the shell, which knows nothing about
 * which screen is mounted or what refreshing it would mean. This is the one
 * sentence between them: `RefreshButton` puts its own action here while it is on
 * screen, and `PullRefresh` asks for it when a finger arrives at the top.
 *
 * Deriving "refreshable" from the button rather than from a list of routes is
 * deliberate — the two can't drift apart, and a screen that grows a refresh
 * button gets the gesture without anyone remembering to add it. Screens with no
 * button (Ask, Settings, Setup) have nothing to sync and the pull is inert.
 *
 * A module variable rather than context because there is exactly one at a time,
 * the gesture reads it imperatively from an event handler, and nothing renders
 * differently for it — a provider would re-render the tree to store a value no
 * component reads.
 */

type Refresher = () => Promise<unknown>;

let current: Refresher | null = null;

/**
 * Offer this screen's refresh to the pull gesture, until the component that
 * owns it goes away.
 *
 * The teardown only clears what it registered. Navigating between two screens
 * that both have a button unmounts the old one and mounts the new one in the
 * same commit, and a cleanup that cleared unconditionally could run second and
 * take the incoming screen's action down with it.
 */
export function registerRefresh(run: Refresher): () => void {
  current = run;
  return () => {
    if (current === run) current = null;
  };
}

export function canRefresh(): boolean {
  return current !== null;
}

/** Run it, or resolve immediately if this screen has nothing to offer. The
 *  refreshers handle their own failures — see `RefreshButton`, which reports
 *  one in its own label — so this never rejects. */
export function refreshNow(): Promise<unknown> {
  return current ? current() : Promise.resolve();
}
