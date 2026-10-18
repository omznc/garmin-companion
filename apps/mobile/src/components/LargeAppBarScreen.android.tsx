// Material 3 large top app bar over a scrolling screen. It collapses on scroll.
import { Host } from "@expo/ui";
import { Icon, IconButton } from "@expo/ui/jetpack-compose";
import { useRouter } from "expo-router";
import type { ReactNode } from "react";
import { RefreshControl, View } from "react-native";
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

type Props = {
  title: string;
  settings?: boolean;
  refreshing?: boolean;
  onRefresh?: () => void;
  children: ReactNode;
};

export function LargeAppBarScreen({
  title,
  settings = true,
  refreshing = false,
  onRefresh,
  children,
}: Props) {
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
        refreshControl={
          onRefresh ? (
            <RefreshControl
              refreshing={refreshing}
              onRefresh={onRefresh}
              progressViewOffset={insets.top + EXPANDED}
            />
          ) : undefined
        }
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
              left: settings ? 16 : 72,
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
        {settings ? (
          <BarButton top={insets.top} side="right" />
        ) : (
          <BarButton top={insets.top} side="left" />
        )}
      </Animated.View>
    </View>
  );
}

// The right button opens Settings. The left button goes back, on the Settings screen.
function BarButton({ top, side }: { top: number; side: "left" | "right" }) {
  const router = useRouter();
  const md = useMd();
  const back = side === "left";
  return (
    <View style={{ position: "absolute", top: top + 8, [side]: 8, width: 48, height: 48 }}>
      <Host matchContents seedColor={HOST_SEED}>
        <IconButton onClick={() => (back ? router.back() : router.push("/settings"))}>
          <Icon
            source={
              back
                ? require("../../assets/icons/back.xml")
                : require("../../assets/icons/account.xml")
            }
            size={24}
            tint={md.onSurfaceVariant}
            contentDescription={back ? "Back" : "Settings"}
          />
        </IconButton>
      </Host>
    </View>
  );
}
