// Setup on Android: Material 3 text and buttons, centred on a plain surface.
import { Host } from "@expo/ui";
import { Button, Column, Text, TextButton } from "@expo/ui/jetpack-compose";
import { fillMaxWidth, paddingAll } from "@expo/ui/jetpack-compose/modifiers";
import { useRouter } from "expo-router";
import { View } from "react-native";
import { setupSkipped, skipSetup } from "../data/account";
import { isSampleData } from "../data/source";
import { useStatus } from "../data/useAccount";
import { HOST_SEED, useMd } from "../theme/md";

export function SetupScreen() {
  const md = useMd();
  const router = useRouter();
  const { data } = useStatus();
  const skip = () => {
    skipSetup();
    router.replace("/today");
  };
  return (
    <View style={{ flex: 1, justifyContent: "center", backgroundColor: md.surface }}>
      <Host matchContents={{ vertical: true }} seedColor={HOST_SEED} style={{ width: "100%" }}>
        <Column verticalArrangement={{ spacedBy: 16 }} modifiers={[fillMaxWidth(), paddingAll(24)]}>
          <Text style={{ typography: "headlineLarge" }} color={md.onSurface}>
            Connect Garmin
          </Text>
          <Text style={{ typography: "bodyLarge" }} color={md.onSurfaceVariant}>
            {data?.signInLine ?? ""}
          </Text>
          <Button onClick={() => router.push("/sign-in")} modifiers={[fillMaxWidth()]}>
            <Text>Sign in with Garmin</Text>
          </Button>
          {isSampleData() && !setupSkipped() ? (
            <TextButton onClick={skip} modifiers={[fillMaxWidth()]}>
              <Text>Use sample data</Text>
            </TextButton>
          ) : null}
        </Column>
      </Host>
    </View>
  );
}
