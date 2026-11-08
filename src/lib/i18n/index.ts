import en from "./en.json";

// Minimal i18n: one JSON dictionary per language, dotted keys, {param} placeholders.
// To add a language, create e.g. it.json with the same keys and register it below.
type Dict = { [key: string]: string | Dict };

const dictionaries: Record<string, Dict> = { en };

let current: Dict = en;

export function setLanguage(lang: string): void {
  current = dictionaries[lang] ?? en;
}

export const availableLanguages = Object.keys(dictionaries);

function lookup(dict: Dict, key: string): string | undefined {
  let node: string | Dict | undefined = dict;
  for (const part of key.split(".")) {
    if (typeof node !== "object") return undefined;
    node = node[part];
  }
  return typeof node === "string" ? node : undefined;
}

export function t(key: string, params: Record<string, string | number> = {}): string {
  const text = lookup(current, key) ?? lookup(en, key) ?? key;
  return text.replace(/\{(\w+)\}/g, (_, name: string) => String(params[name] ?? `{${name}}`));
}
