// Setup on iOS: system colours and Dynamic Type, with a prominent button.
import { useRouter } from "expo-router";
import { Button, PlatformColor, Pressable, Text, View } from "react-native";
import { skipSetup } from "../data/account";
import { isSampleData } from "../data/source";
import { useStatus } from "../data/useAccount";

export function SetupScreen() {
  const router = useRouter();
  const { data } = useStatus();
  const skip = () => {
    skipSetup();
    router.replace("/today");
  };
  return (
    <View
      style={{
        flex: 1,
        justifyContent: "center",
        padding: 24,
        gap: 16,
        backgroundColor: PlatformColor("systemGroupedBackground"),
      }}
    >
      <Text
        accessibilityRole="header"
        style={{ fontSize: 34, fontWeight: "700", color: PlatformColor("label") }}
      >
        Connect Garmin
      </Text>
      <Text style={{ fontSize: 17, color: PlatformColor("secondaryLabel") }}>
        {data?.signInLine ?? ""}
      </Text>
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
        <Text style={{ fontSize: 17, fontWeight: "600", color: "white" }}>Sign in with Garmin</Text>
      </Pressable>
      {isSampleData() ? <Button title="Use sample data" onPress={skip} /> : null}
    </View>
  );
}
