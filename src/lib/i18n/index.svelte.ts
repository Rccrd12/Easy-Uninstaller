import en from "./en.json";
import it from "./it.json";

// Minimal i18n: one JSON dictionary per language, dotted keys, {param} placeholders.
// To add a language, create a JSON file with the same keys as en.json and register it below.
type Dict = { [key: string]: string | Dict };

export const languages = [
  { code: "en", name: "English", dict: en as Dict },
  { code: "it", name: "Italiano", dict: it as Dict },
] as const;

export type Language = (typeof languages)[number]["code"];

const STORAGE_KEY = "language";

function initialLanguage(): Language {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (languages.some((l) => l.code === saved)) return saved as Language;
  } catch {
    // Storage unavailable: fall back to the system language.
  }
  const system = navigator.language.slice(0, 2);
  return languages.some((l) => l.code === system) ? (system as Language) : "en";
}

// Reactive: components calling t() re-render when the language changes.
const state = $state({ lang: initialLanguage() });

export function getLanguage(): Language {
  return state.lang;
}

export function setLanguage(lang: Language): void {
  state.lang = lang;
  document.documentElement.lang = lang;
  try {
    localStorage.setItem(STORAGE_KEY, lang);
  } catch {
    // Not persisted; the choice still applies for this session.
  }
}

function lookup(dict: Dict, key: string): string | undefined {
  let node: string | Dict | undefined = dict;
  for (const part of key.split(".")) {
    if (typeof node !== "object") return undefined;
    node = node[part];
  }
  return typeof node === "string" ? node : undefined;
}

export function t(key: string, params: Record<string, string | number> = {}): string {
  const dict = languages.find((l) => l.code === state.lang)?.dict ?? en;
  const text = lookup(dict, key) ?? lookup(en, key) ?? key;
  return text.replace(/\{(\w+)\}/g, (_, name: string) => String(params[name] ?? `{${name}}`));
}

/** Locale used for number formatting, following the chosen language. */
export function locale(): string {
  return state.lang === "it" ? "it-IT" : "en-US";
}
