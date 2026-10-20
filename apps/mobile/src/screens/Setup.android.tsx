// Welcome on Android: the scheme colours, tonal cards and Material 3 buttons.
import { Host } from "@expo/ui";
import { Button, Column, Text, TextButton } from "@expo/ui/jetpack-compose";
import { fillMaxWidth } from "@expo/ui/jetpack-compose/modifiers";
import { useRouter } from "expo-router";
import { Image, ScrollView, View } from "react-native";
import { Card, IconTile, type IconName, T } from "../components/MdKit";
import { setupSkipped, skipSetup } from "../data/account";
import { isSampleData } from "../data/source";
import { useHostTheme, useMd } from "../theme/md";

const FEATURES: { icon: IconName; title: string; line: string }[] = [
  {
    icon: "favorite",
    title: "Health trends",
    line: "Sleep, readiness and recovery, with the days around them.",
  },
  {
    icon: "directions-run",
    title: "Your activities",
    line: "Every session with its pace, heart rate and zones.",
  },
  {
    icon: "auto-awesome",
    title: "An AI coach",
    line: "Ask about your numbers. The coach can draft a workout for your watch.",
  },
];

function FeatureRow({ icon, title, line }: (typeof FEATURES)[number]) {
  const md = useMd();
  return (
    <View style={{ flexDirection: "row", alignItems: "center", gap: 16 }}>
      <IconTile name={icon} tone="primary" size={44} />
      <View style={{ flex: 1 }}>
        <T v="titleMedium">{title}</T>
        <T v="bodyMedium" color={md.onSurfaceVariant}>
          {line}
        </T>
      </View>
    </View>
  );
}

export function SetupScreen() {
  const md = useMd();
  const host = useHostTheme();
  const router = useRouter();
  const skip = () => {
    skipSetup();
    router.replace("/today");
  };
  return (
    <ScrollView
      style={{ backgroundColor: md.surface }}
      contentContainerStyle={{ flexGrow: 1, justifyContent: "center", padding: 24, gap: 24 }}
    >
      <View style={{ alignItems: "center", gap: 16 }}>
        <Image
          source={require("../../assets/icon.png")}
          style={{ width: 88, height: 88, borderRadius: 24 }}
          accessibilityLabel="Garmin Companion"
        />
        <T v="headline" align="center">
          Your Garmin data, explained.
        </T>
      </View>
      <Card tone="high" gap={20}>
        {FEATURES.map((f) => (
          <FeatureRow key={f.title} {...f} />
        ))}
      </Card>
      <Host matchContents={{ vertical: true }} {...host} style={{ width: "100%" }}>
        <Column verticalArrangement={{ spacedBy: 8 }} modifiers={[fillMaxWidth()]}>
          <Button onClick={() => router.push("/sign-in")} modifiers={[fillMaxWidth()]}>
            <Text>Sign in with Garmin</Text>
          </Button>
          {isSampleData() && !setupSkipped() ? (
            <TextButton onClick={skip} modifiers={[fillMaxWidth()]}>
              <Text>Look around with sample data</Text>
            </TextButton>
          ) : null}
        </Column>
      </Host>
      <T v="labelSmall" color={md.onSurfaceVariant} align="center">
        Your data stays on this phone. Your Garmin password goes only to Garmin.
      </T>
    </ScrollView>
  );
}
