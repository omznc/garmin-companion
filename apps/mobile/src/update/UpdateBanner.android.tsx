// M3 snackbar style banner for the APK self-update. It floats above the tab bar.
import { Host } from "@expo/ui";
import {
  Card,
  Column,
  LinearWavyProgressIndicator,
  Row,
  Text,
  TextButton,
} from "@expo/ui/jetpack-compose";
import { fillMaxWidth, paddingAll, weight } from "@expo/ui/jetpack-compose/modifiers";
import { View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useHostTheme, useMd } from "../theme/md";
import { SELF_UPDATE, type UpdateState, useApkUpdate } from "./useApkUpdate";

type Md = ReturnType<typeof useMd>;

function sentence(s: Exclude<UpdateState, { at: "idle" }>): string {
  const v = s.release.version;
  switch (s.at) {
    case "available":
      return `Version ${v} is available`;
    case "downloading":
      return `Downloading version ${v}`;
    case "ready":
      return `Installing version ${v}`;
    case "blocked":
      return 'Turn on "Install unknown apps" for this app, then tap Install';
    case "failed":
      return `Update failed: ${s.message}`;
  }
}

function Actions({
  md,
  api,
  s,
}: {
  md: Md;
  api: ReturnType<typeof useApkUpdate>;
  s: Exclude<UpdateState, { at: "idle" }>;
}) {
  const colors = { contentColor: md.inversePrimary };
  if (s.at === "downloading" || s.at === "ready") return null;
  const label = s.at === "available" ? "Update" : s.at === "blocked" ? "Install" : "Retry";
  const go = () => (s.at === "blocked" ? api.install(s.release, s.path) : api.start(s.release));
  return (
    <Row horizontalArrangement={{ spacedBy: 4 }}>
      <TextButton colors={colors} onClick={api.dismiss}>
        <Text>Later</Text>
      </TextButton>
      <TextButton colors={colors} onClick={go}>
        <Text>{label}</Text>
      </TextButton>
    </Row>
  );
}

export function UpdateBanner() {
  const api = useApkUpdate();
  const md = useMd();
  const host = useHostTheme();
  const insets = useSafeAreaInsets();
  const s = api.state;
  if (!SELF_UPDATE || s.at === "idle") return null;

  const progress = s.at === "downloading" && s.total > 0 ? s.received / s.total : null;
  return (
    <View
      pointerEvents="box-none"
      style={{ position: "absolute", left: 8, right: 8, bottom: insets.bottom + 88 }}
    >
      <Host matchContents={{ vertical: true }} {...host} style={{ width: "100%" }}>
        <Card
          colors={{ containerColor: md.inverseSurface, contentColor: md.inverseOnSurface }}
          elevation={6}
        >
          <Column
            verticalArrangement={{ spacedBy: 8 }}
            modifiers={[fillMaxWidth(), paddingAll(16)]}
          >
            <Row verticalAlignment="center" horizontalArrangement={{ spacedBy: 8 }}>
              <Text
                style={{ typography: "bodyMedium" }}
                color={md.inverseOnSurface}
                modifiers={[weight(1)]}
              >
                {sentence(s)}
              </Text>
              <Actions md={md} api={api} s={s} />
            </Row>
            {s.at === "downloading" && (
              <LinearWavyProgressIndicator
                progress={progress}
                color={md.inversePrimary}
                modifiers={[fillMaxWidth()]}
              />
            )}
          </Column>
        </Card>
      </Host>
    </View>
  );
}
