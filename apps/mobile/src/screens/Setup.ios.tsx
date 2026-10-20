// Welcome on iOS: system colours and Dynamic Type, with a prominent button.
import { useRouter } from "expo-router";
import { SymbolView, type SFSymbol } from "expo-symbols";
import { Image, PlatformColor, Pressable, ScrollView, Text, View } from "react-native";
import { setupSkipped, skipSetup } from "../data/account";
import { isSampleData } from "../data/source";

const FEATURES: { symbol: SFSymbol; title: string; line: string }[] = [
  {
    symbol: "heart.fill",
    title: "Health trends",
    line: "Sleep, readiness and recovery, with the days around them.",
  },
  {
    symbol: "figure.run",
    title: "Your activities",
    line: "Every session with its pace, heart rate and zones.",
  },
  {
    symbol: "sparkles",
    title: "An AI coach",
    line: "Ask about your numbers. The coach can draft a workout for your watch.",
  },
];

export function SetupScreen() {
  const router = useRouter();
  const skip = () => {
    skipSetup();
    router.replace("/today");
  };
  return (
    <ScrollView
      style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
      contentContainerStyle={{ flexGrow: 1, justifyContent: "center", padding: 24, gap: 24 }}
    >
      <View style={{ alignItems: "center", gap: 16 }}>
        <Image
          source={require("../../assets/icon.png")}
          style={{ width: 88, height: 88, borderRadius: 20 }}
          accessibilityLabel="Garmin Companion"
        />
        <Text
          accessibilityRole="header"
          style={{
            fontSize: 34,
            fontWeight: "700",
            textAlign: "center",
            color: PlatformColor("label"),
          }}
        >
          Your Garmin data, explained.
        </Text>
      </View>
      <View
        style={{
          gap: 20,
          padding: 20,
          borderRadius: 16,
          borderCurve: "continuous",
          backgroundColor: PlatformColor("secondarySystemGroupedBackground"),
        }}
      >
        {FEATURES.map((f) => (
          <View key={f.title} style={{ flexDirection: "row", alignItems: "center", gap: 16 }}>
            <SymbolView name={f.symbol} size={28} tintColor={PlatformColor("systemOrange")} />
            <View style={{ flex: 1 }}>
              <Text style={{ fontSize: 17, fontWeight: "600", color: PlatformColor("label") }}>
                {f.title}
              </Text>
              <Text style={{ fontSize: 15, color: PlatformColor("secondaryLabel") }}>{f.line}</Text>
            </View>
          </View>
        ))}
      </View>
      <View style={{ gap: 8 }}>
        <Pressable
          accessibilityRole="button"
          onPress={() => router.push("/sign-in")}
          style={{
            backgroundColor: PlatformColor("systemOrange"),
            borderRadius: 14,
            paddingVertical: 14,
            alignItems: "center",
          }}
        >
          <Text style={{ fontSize: 17, fontWeight: "600", color: "white" }}>
            Sign in with Garmin
          </Text>
        </Pressable>
        {isSampleData() && !setupSkipped() ? (
          <Pressable accessibilityRole="button" onPress={skip} style={{ paddingVertical: 12 }}>
            <Text
              style={{ fontSize: 17, textAlign: "center", color: PlatformColor("systemOrange") }}
            >
              Look around with sample data
            </Text>
          </Pressable>
        ) : null}
      </View>
      <Text style={{ fontSize: 13, textAlign: "center", color: PlatformColor("secondaryLabel") }}>
        Your data stays on this phone. Your Garmin password goes only to Garmin.
      </Text>
    </ScrollView>
  );
}
