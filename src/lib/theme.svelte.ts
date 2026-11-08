import { setWindowTheme } from "./api";

// Color theme: follow Windows by default, or force light or dark.
// The choice is kept in the webview's local storage, like the language.
export const themes = ["system", "light", "dark"] as const;
export type ThemePreference = (typeof themes)[number];

const STORAGE_KEY = "theme";
const systemDark = window.matchMedia("(prefers-color-scheme: dark)");

function initialPreference(): ThemePreference {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (themes.some((t) => t === saved)) return saved as ThemePreference;
  } catch {
    // Storage unavailable: follow the system.
  }
  return "system";
}

// Reactive: components reading getTheme() re-render when it changes.
const state = $state({ preference: initialPreference() });

/** Puts the light or dark palette on <html>; theme.css does the rest. */
function apply(): void {
  const dark = state.preference === "dark" || (state.preference === "system" && systemDark.matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

export function getTheme(): ThemePreference {
  return state.preference;
}

export function setTheme(preference: ThemePreference): void {
  state.preference = preference;
  try {
    localStorage.setItem(STORAGE_KEY, preference);
  } catch {
    // Not persisted; the choice still applies for this session.
  }
  void setWindowTheme(preference);
  apply();
}

/** Call once before mounting the app, so the first frame already has the right colors. */
export function initTheme(): void {
  apply();
  void setWindowTheme(state.preference);
  // When following the system, switch as soon as Windows changes mode.
  systemDark.addEventListener("change", apply);
}
