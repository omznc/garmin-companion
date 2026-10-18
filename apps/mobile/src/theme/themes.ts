// The seed colours that a person can choose. Compose builds a full Material 3 scheme
// from the seed. Ember is the colour of the desktop app.
export const THEMES = [
  { id: "ember", label: "Ember", seed: "#b0563a" },
  { id: "violet", label: "Violet", seed: "#6f4ebe" },
  { id: "indigo", label: "Indigo", seed: "#4a5ad0" },
  { id: "blue", label: "Blue", seed: "#2f6fd0" },
  { id: "teal", label: "Teal", seed: "#0f8a8a" },
  { id: "green", label: "Green", seed: "#3f8f4f" },
  { id: "amber", label: "Amber", seed: "#c28a12" },
  { id: "crimson", label: "Crimson", seed: "#c2364a" },
  { id: "magenta", label: "Magenta", seed: "#b8338f" },
  { id: "slate", label: "Slate", seed: "#5a6a7a" },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];
