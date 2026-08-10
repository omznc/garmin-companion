/**
 * "There's a new version, and it's already downloaded" — said on the phone.
 *
 * The desktop has said this all along, in the sidebar, where an update sits in
 * front of you whatever screen you're on. A phone has no sidebar, so the same
 * fact lived only in Settings: the app fetched a version in the background, and
 * then waited to be gone looking for. The one moment it was worth saying —
 * opening the app — was the moment nothing said it.
 *
 * So this is that sentence, in the card at the bottom of the screen that the
 * sync readout and the provider warning already use. Not an Android
 * notification: the app is open and being looked at, and a notification is for
 * reaching someone who isn't. Tapping it hands the APK to the system installer,
 * which is where the real decision gets made — see `apply` in `lib/updater`.
 *
 * Dismissing it is for this session only. The download doesn't go anywhere, and
 * the next launch is a fresh chance to mention it; a version you said "not now"
 * to in the morning is still worth an offer in the evening. What it won't do is
 * ask twice while you're reading.
 */
import { useState } from "react";
import { createPortal } from "react-dom";
import { apply } from "../lib/updater";
import { IS_MOBILE } from "../lib/platform";
import { CloseIcon, UpdateIcon } from "../lib/icons";
import { useUpdateState } from "./UpdateCheck";
import { useSyncState } from "./SyncBar";
import { useChatHealth } from "./AiBar";

export function UpdateBar() {
  const update = useUpdateState();
  const sync = useSyncState();
  const health = useChatHealth();
  /** The version already offered and waved off, for as long as this run lasts. */
  const [dismissed, setDismissed] = useState<string | null>(null);

  // Desktop says this in the sidebar, which is always on screen and doesn't
  // cover anything.
  if (!IS_MOBILE) return null;
  // The two states where there is nothing left to do but decide: downloaded, or
  // downloaded and waiting on a permission. Everything before that — checking,
  // downloading — is background work nobody asked to watch.
  if (update.at !== "ready" && update.at !== "blocked") return null;
  if (dismissed === update.version) return null;
  // One card, three tenants, and this is the junior one: a sync is running now
  // and a broken provider is breaking something now, where an update has been
  // waiting since launch and can wait a minute longer.
  if (sync.running || health.data?.ok === false) return null;

  const blocked = update.at === "blocked";

  const bar = (
    <div role="status" className="update-bar">
      <UpdateIcon size={14} style={{ flex: "none", color: "var(--acc)" }} aria-hidden />
      <button
        onClick={apply}
        style={{
          fontSize: "var(--fs-small)",
          color: "var(--fg)",
          cursor: "pointer",
          minWidth: 0,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          textAlign: "left",
          flex: 1,
        }}
      >
        {/* Second time round, the sentence changes: the tap already happened,
            it landed on a permission screen, and the way back is the same
            button. Naming the switch rather than the permission, because
            "Install unknown apps" is the string on the screen they were sent
            to — see the same wording in `UpdateCheck`. */}
        {blocked
          ? `Turn on "Install unknown apps", then tap here`
          : `Version ${update.version} is ready to install`}
      </button>
      <button
        className="quiet"
        onClick={() => setDismissed(update.version)}
        aria-label="Dismiss"
        title="Dismiss until the next launch"
        style={{ display: "grid", placeItems: "center", color: "var(--faint)", flex: "none" }}
      >
        <CloseIcon size={12} aria-hidden />
      </button>
    </div>
  );

  // Out of the scroller, for the reason `SyncBar` and `TabBar` are: Android
  // stretches the whole scrolling layer on an overscroll, and anything
  // `position: fixed` inside it gets stretched along with the content.
  return createPortal(bar, document.body);
}
