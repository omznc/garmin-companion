/** Theme selection. Persisted in localStorage; "system" follows the OS. */

export type Theme = "light" | "dark" | "system";

const KEY = "garmin-companion:theme";
const media = () => window.matchMedia("(prefers-color-scheme: dark)");

export function loadTheme(): Theme {
  const v = localStorage.getItem(KEY);
  return v === "light" || v === "dark" || v === "system" ? v : "system";
}

export function resolve(theme: Theme): "light" | "dark" {
  return theme === "system" ? (media().matches ? "dark" : "light") : theme;
}

export function applyTheme(theme: Theme): void {
  document.documentElement.setAttribute("data-theme", resolve(theme));
  localStorage.setItem(KEY, theme);
}

/** Re-applies on OS change, but only while the user is on "system". */
export function watchSystemTheme(onChange: () => void): () => void {
  const m = media();
  m.addEventListener("change", onChange);
  return () => m.removeEventListener("change", onChange);
}
