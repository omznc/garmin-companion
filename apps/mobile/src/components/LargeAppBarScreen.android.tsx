// Material 3 top app bar over a scrolling screen. The large bar collapses on scroll.
// The small bar (`small`) is for settings pages: a back button and a title in one row.
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
import { scheduleOnRN } from "react-native-worklets";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useHostTheme, useMd } from "../theme/md";

const EXPANDED = 136;
const COLLAPSED = 64;

type Props = {
  title: string;
  back?: boolean;
  /** A small bar that does not collapse. */
  small?: boolean;
  refreshing?: boolean;
  onRefresh?: () => void;
  /** Called with the scroll offset, for a FAB that collapses. */
  onScrollOffset?: (offset: number) => void;
  /** Space below the last row, for a FAB. */
  bottomSpace?: number;
  children: ReactNode;
};

export function LargeAppBarScreen({
  title,
  back = false,
  small = false,
  refreshing = false,
  onRefresh,
  onScrollOffset,
  bottomSpace = 0,
  children,
}: Props) {
  const md = useMd();
  const insets = useSafeAreaInsets();
  const expanded = small ? COLLAPSED : EXPANDED;
  const range = Math.max(expanded - COLLAPSED, 1);
  const scrollY = useSharedValue(0);
  const onScroll = useAnimatedScrollHandler((e) => {
    scrollY.value = e.contentOffset.y;
    if (onScrollOffset) scheduleOnRN(onScrollOffset, e.contentOffset.y);
  });

  const barStyle = useAnimatedStyle(() => ({
    height: insets.top + interpolate(scrollY.value, [0, range], [expanded, COLLAPSED], "clamp"),
    backgroundColor: interpolateColor(
      scrollY.value,
      [0, small ? 24 : range],
      [md.surface, md.surfaceContainer],
    ),
  }));
  const titleStyle = useAnimatedStyle(() => ({
    // Under the back button when large, beside it once the bar is small.
    left: interpolate(
      scrollY.value,
      [0, range],
      [back && small ? 72 : 20, back ? 72 : 20],
      "clamp",
    ),
    fontSize: interpolate(scrollY.value, [0, range], [small ? 22 : 34, 22], Extrapolation.CLAMP),
    lineHeight: interpolate(scrollY.value, [0, range], [small ? 28 : 42, 28], Extrapolation.CLAMP),
  }));

  return (
    <View style={{ flex: 1, backgroundColor: md.surface }}>
      <Animated.ScrollView
        onScroll={onScroll}
        // One event per frame. 16 ms holds the bar to 60 Hz on a 90 or 120 Hz
        // screen, where it steps unevenly against the list under it.
        scrollEventThrottle={1}
        refreshControl={
          onRefresh ? (
            <RefreshControl
              refreshing={refreshing}
              onRefresh={onRefresh}
              progressViewOffset={insets.top + expanded}
            />
          ) : undefined
        }
        contentContainerStyle={{
          paddingTop: insets.top + expanded,
          paddingBottom: 24 + bottomSpace,
        }}
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
              right: 72,
              bottom: small ? 18 : 16,
              color: md.onSurface,
              fontWeight: "500",
            },
            titleStyle,
          ]}
        >
          {title}
        </Animated.Text>
        {back ? <BackButton top={insets.top} /> : null}
      </Animated.View>
    </View>
  );
}

function BackButton({ top }: { top: number }) {
  const router = useRouter();
  const md = useMd();
  const host = useHostTheme();
  return (
    <View style={{ position: "absolute", top: top + 8, left: 8, width: 48, height: 48 }}>
      <Host matchContents {...host}>
        <IconButton onClick={() => router.back()}>
          <Icon
            source={require("../../assets/icons/back.xml")}
            size={24}
            tint={md.onSurface}
            contentDescription="Back"
          />
        </IconButton>
      </Host>
    </View>
  );
}
