// The Material 3 extended FAB that opens the coach. It shows "Ask" at the top of a list
// and collapses to the icon while the list scrolls down.
import MaterialIcons from "@expo/vector-icons/MaterialIcons";
import { useEffect, useRef, useState } from "react";
import { Pressable, View } from "react-native";
import Animated, {
  Easing,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from "react-native-reanimated";
import { useMd } from "../theme/md";
import { useReduceMotion } from "../theme/motion";
import { T } from "./MdKit";

const SIZE = 56;
const MARGIN = 16;
/** The space a list adds below its last row, so the FAB hides nothing. */
export const FAB_CLEARANCE = SIZE + MARGIN * 2;
const ICON_GAP = 8;
const DURATION_MS = 200;
const EASE = Easing.bezier(0.2, 0, 0, 1);
/** A scroll move smaller than this is the jitter of a finger. */
const SCROLL_SLOP = 8;

/** True at the top of a list and on a scroll up. False on a scroll down. */
export function useFabExtended() {
  const [extended, setExtended] = useState(true);
  const last = useRef(0);
  const onScroll = (offset: number) => {
    if (offset <= 0) {
      last.current = 0;
      setExtended(true);
      return;
    }
    const delta = offset - last.current;
    if (Math.abs(delta) < SCROLL_SLOP) return;
    last.current = offset;
    setExtended(delta < 0);
  };
  return { extended, onScroll };
}

export function AskFab({ extended, onPress }: { extended: boolean; onPress: () => void }) {
  const md = useMd();
  const reduceMotion = useReduceMotion();
  // The width of the label with its gap, measured once while hidden.
  const [labelWidth, setLabelWidth] = useState(0);
  const progress = useSharedValue(extended ? 1 : 0);
  useEffect(() => {
    const target = extended ? 1 : 0;
    progress.set(
      reduceMotion ? target : withTiming(target, { duration: DURATION_MS, easing: EASE }),
    );
  }, [extended, reduceMotion, progress]);
  const labelStyle = useAnimatedStyle(() => ({
    width: progress.get() * labelWidth,
    opacity: progress.get(),
  }));
  return (
    <View pointerEvents="box-none" style={{ position: "absolute", right: MARGIN, bottom: MARGIN }}>
      <View
        pointerEvents="none"
        importantForAccessibility="no-hide-descendants"
        style={{ position: "absolute", opacity: 0, flexDirection: "row" }}
        onLayout={(event) => setLabelWidth(event.nativeEvent.layout.width)}
      >
        <View style={{ width: ICON_GAP }} />
        <T v="label">Ask</T>
      </View>
      <View
        style={{
          height: SIZE,
          minWidth: SIZE,
          borderRadius: 16,
          overflow: "hidden",
          backgroundColor: md.primaryContainer,
          elevation: 3,
        }}
      >
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Ask the coach"
          onPress={onPress}
          android_ripple={{ color: md.onPrimaryContainer }}
          style={{
            flexDirection: "row",
            alignItems: "center",
            height: SIZE,
            minWidth: SIZE,
            paddingHorizontal: (SIZE - 24) / 2,
          }}
        >
          <MaterialIcons name="chat" size={24} color={md.onPrimaryContainer} />
          <Animated.View style={[{ flexDirection: "row", overflow: "hidden" }, labelStyle]}>
            <View style={{ width: ICON_GAP }} />
            <T v="label" color={md.onPrimaryContainer} lines={1}>
              Ask
            </T>
          </Animated.View>
        </Pressable>
      </View>
    </View>
  );
}
