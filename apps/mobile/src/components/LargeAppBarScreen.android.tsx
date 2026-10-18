// Material 3 large top app bar over a scrolling screen. It collapses on scroll.
import { Host } from "@expo/ui";
import { Icon, IconButton } from "@expo/ui/jetpack-compose";
import { useRouter } from "expo-router";
import type { ReactNode } from "react";
import { View } from "react-native";
import Animated, {
  Extrapolation,
  interpolate,
  interpolateColor,
  useAnimatedScrollHandler,
  useAnimatedStyle,
  useSharedValue,
} from "react-native-reanimated";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { HOST_SEED, useMd } from "../theme/md";

const EXPANDED = 120;
const COLLAPSED = 64;
const RANGE = EXPANDED - COLLAPSED;

type Props = { title: string; settings?: boolean; children: ReactNode };

export function LargeAppBarScreen({ title, settings = true, children }: Props) {
  const md = useMd();
  const insets = useSafeAreaInsets();
  const scrollY = useSharedValue(0);
  const onScroll = useAnimatedScrollHandler((e) => {
    scrollY.value = e.contentOffset.y;
  });

  const barStyle = useAnimatedStyle(() => ({
    height: insets.top + interpolate(scrollY.value, [0, RANGE], [EXPANDED, COLLAPSED], "clamp"),
    backgroundColor: interpolateColor(scrollY.value, [0, RANGE], [md.surface, md.surfaceContainer]),
  }));
  const titleStyle = useAnimatedStyle(() => ({
    fontSize: interpolate(scrollY.value, [0, RANGE], [28, 22], Extrapolation.CLAMP),
    lineHeight: interpolate(scrollY.value, [0, RANGE], [36, 28], Extrapolation.CLAMP),
  }));

  return (
    <View style={{ flex: 1, backgroundColor: md.surface }}>
      <Animated.ScrollView
        onScroll={onScroll}
        scrollEventThrottle={16}
        contentContainerStyle={{ paddingTop: insets.top + EXPANDED, paddingBottom: 24 }}
      >
        {children}
      </Animated.ScrollView>
      <Animated.View style={[{ position: "absolute", top: 0, left: 0, right: 0 }, barStyle]}>
        <Animated.Text
          accessibilityRole="header"
          numberOfLines={1}
          style={[
            {
              position: "absolute",
              left: 16,
              right: 72,
              bottom: 18,
              color: md.onSurface,
              fontWeight: "400",
            },
            titleStyle,
          ]}
        >
          {title}
        </Animated.Text>
        {settings ? <SettingsButton top={insets.top} /> : null}
      </Animated.View>
    </View>
  );
}

function SettingsButton({ top }: { top: number }) {
  const router = useRouter();
  const md = useMd();
  return (
    <View style={{ position: "absolute", top: top + 8, right: 8, width: 48, height: 48 }}>
      <Host matchContents seedColor={HOST_SEED}>
        <IconButton onClick={() => router.push("/settings")}>
          <Icon
            source={require("../../assets/icons/account.xml")}
            size={24}
            tint={md.onSurfaceVariant}
            contentDescription="Settings"
          />
        </IconButton>
      </Host>
    </View>
  );
}
