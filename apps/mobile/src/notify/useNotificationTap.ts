// A tap on a notification opens its screen. The last response also covers a cold start from a tap.
import * as Notifications from "expo-notifications";
import { useRouter } from "expo-router";
import { useEffect } from "react";
import { safeHref } from "./plan";

export function useNotificationTap() {
  const router = useRouter();
  const response = Notifications.useLastNotificationResponse();
  useEffect(() => {
    if (response?.actionIdentifier !== Notifications.DEFAULT_ACTION_IDENTIFIER) return;
    const href = safeHref(response.notification.request.content.data?.href);
    if (href) router.push(href as never, { withAnchor: true });
    // Without this, a remount or a cold start would open the same screen again.
    Notifications.clearLastNotificationResponse();
  }, [response, router]);
}
