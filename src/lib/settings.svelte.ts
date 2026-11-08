// User preferences kept in the webview's local storage.
// Reactive: components reading `settings` re-render when a value changes.

const STORAGE_KEY = "settings";

interface Settings {
  /** Make a Windows restore point before uninstalling apps. */
  restorePoint: boolean;
}

// Off by default: it needs administrator rights and System Restore is often off on new PCs.
const defaults: Settings = { restorePoint: false };

function load(): Settings {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    return { ...defaults, ...(typeof saved === "object" && saved ? saved : {}) };
  } catch {
    return { ...defaults };
  }
}

export const settings = $state<Settings>(load());

export function saveSettings(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Not persisted; the choice still applies for this session.
  }
}
