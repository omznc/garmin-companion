// Links from Health to Weight, Food and Strength. Strength shows only for a gym profile.
import { Host } from "@expo/ui";
import { Column, ListItem, Text } from "@expo/ui/jetpack-compose";
import { clickable, fillMaxWidth } from "@expo/ui/jetpack-compose/modifiers";
import { useRouter } from "expo-router";
import { useStrengthShown } from "../data/body";
import { HOST_SEED, useMd } from "../theme/md";

export function BodyLinks() {
  const router = useRouter();
  const md = useMd();
  const strength = useStrengthShown().data === true;
  const links = [
    { href: "/health/weight", label: "Weight" },
    { href: "/health/food", label: "Food" },
    ...(strength ? [{ href: "/health/strength", label: "Strength" }] : []),
  ] as const;
  return (
    <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
      <Column modifiers={[fillMaxWidth()]}>
        {links.map((l) => (
          <ListItem key={l.href} modifiers={[clickable(() => router.push(l.href))]}>
            <ListItem.HeadlineContent>
              <Text color={md.onSurface}>{l.label}</Text>
            </ListItem.HeadlineContent>
          </ListItem>
        ))}
      </Column>
    </Host>
  );
}
