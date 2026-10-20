import { useNavigationContainerRef } from "expo-router";
import { useEffect } from "react";
import { Platform } from "react-native";
import { BackGesture } from "../../modules/back-gesture";

// On Android, give the back gesture to the system where the app would only exit
// (a root screen with nothing pushed). Then Android plays the back-to-home preview.
// Everywhere else the app keeps Back. The check is the same `canGoBack` that Expo
// Router's own BackHandler listener asks, so the two never disagree.
export function useSystemBack(): void {
  const navigation = useNavigationContainerRef();
  useEffect(() => {
    if (Platform.OS !== "android" || !BackGesture) return;
    const update = () => {
      // Before the navigator is ready, the app keeps Back.
      const canGoBack = navigation.isReady() ? navigation.canGoBack() : true;
      BackGesture?.setSystemOwnsBack(!canGoBack);
    };
    update();
    return navigation.addListener("state", update);
  }, [navigation]);
}
