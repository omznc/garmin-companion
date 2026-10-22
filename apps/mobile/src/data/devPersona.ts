// Dev only: EXPO_PUBLIC_PERSONA picks the made-up user, as it does for Today.
// It lives apart from the fixtures, so a release build does not load them to read it.
export const devPersona = __DEV__ ? process.env.EXPO_PUBLIC_PERSONA : undefined;
