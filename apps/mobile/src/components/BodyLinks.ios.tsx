// Links from Health to Weight, Food and Strength. Strength shows only for a gym profile.
import { useRouter } from "expo-router";
import { PlatformColor, Pressable, Text, View } from "react-native";
import { useStrengthShown } from "../data/body";

export function BodyLinks() {
  const router = useRouter();
  const strength = useStrengthShown().data === true;
  const links = [
    { href: "/health/weight", label: "Weight" },
    { href: "/health/food", label: "Food" },
    ...(strength ? [{ href: "/health/strength", label: "Strength" }] : []),
  ] as const;
  return (
    <View
      style={{
        marginTop: 24,
        borderRadius: 26,
        overflow: "hidden",
        backgroundColor: PlatformColor("secondarySystemGroupedBackground"),
      }}
    >
      {links.map((l, i) => (
        <Pressable
          key={l.href}
          accessibilityRole="button"
          onPress={() => router.push(l.href)}
          style={{
            flexDirection: "row",
            justifyContent: "space-between",
            minHeight: 44,
            alignItems: "center",
            paddingHorizontal: 16,
            borderBottomWidth: i === links.length - 1 ? 0 : 0.5,
            borderBottomColor: PlatformColor("separator"),
          }}
        >
          <Text style={{ fontSize: 17, color: PlatformColor("label") }}>{l.label}</Text>
          <Text style={{ fontSize: 17, color: PlatformColor("tertiaryLabel") }}>{">"}</Text>
        </Pressable>
      ))}
    </View>
  );
}
