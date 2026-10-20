// iOS uses `Alert.alert` through `showDialog`, so it has no dialog to draw.
// The Android file draws the Material 3 dialog.
export function DialogHost() {
  return null;
}
