import MaterialIcons from "@expo/vector-icons/MaterialIcons";
import { useRouter } from "expo-router";
import type { ComponentProps } from "react";
import { Pressable, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useMd } from "../theme/md";

const HEIGHT = 64;

type IconName = ComponentProps<typeof MaterialIcons>["name"];

function BarButton({
  icon,
  label,
  onPress,
}: {
  icon: IconName;
  label: string;
  onPress: () => void;
}) {
  const md = useMd();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      onPress={onPress}
      android_ripple={{ color: `${md.onSurfaceVariant}33`, borderless: true, radius: 24 }}
      style={{ width: 48, height: 48, alignItems: "center", justifyContent: "center" }}
    >
      <MaterialIcons name={icon} size={24} color={md.onSurfaceVariant} />
    </Pressable>
  );
}

// The small Material 3 top app bar of the Ask screens: back, title, and one action.
export function AppBar({
  title,
  back = false,
  action,
}: {
  title: string;
  back?: boolean;
  action?: { icon: IconName; label: string; onPress: () => void };
}) {
  const md = useMd();
  const router = useRouter();
  const insets = useSafeAreaInsets();
  return (
    <View
      style={{
        paddingTop: insets.top,
        height: insets.top + HEIGHT,
        paddingHorizontal: 8,
        flexDirection: "row",
        alignItems: "center",
        backgroundColor: md.surface,
      }}
    >
      {back ? (
        <BarButton
          icon="arrow-back"
          label="Back"
          onPress={() => (router.canGoBack() ? router.back() : router.replace("/today"))}
        />
      ) : null}
      <Text
        accessibilityRole="header"
        numberOfLines={1}
        style={{
          flex: 1,
          marginLeft: back ? 8 : 12,
          color: md.onSurface,
          fontSize: 22,
          lineHeight: 28,
          fontWeight: "500",
        }}
      >
        {title}
      </Text>
      {action ? <BarButton {...action} /> : null}
    </View>
  );
}
