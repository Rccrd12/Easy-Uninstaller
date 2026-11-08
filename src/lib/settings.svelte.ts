// User preferences kept in the webview's local storage, plus the quarantine
// choices, which Rust keeps (it needs them to expire entries on start).
// Reactive: components reading `settings` or `quarantine` re-render when a value changes.
import { getQuarantineOptions, setQuarantineSettings } from "./api";
import type { DriveInfo, QuarantineSettings } from "./types";

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

/** The quarantine choices, loaded from Rust by `loadQuarantine` (defaults until then). */
export const quarantine = $state<QuarantineSettings & { systemDrive: string; drives: DriveInfo[] }>({
  keepDays: 7,
  drive: null,
  systemDrive: "C:",
  drives: [],
});

export async function loadQuarantine(): Promise<void> {
  const o = await getQuarantineOptions();
  Object.assign(quarantine, o.settings, { systemDrive: o.systemDrive, drives: o.drives });
}

/** Saves new quarantine choices; on failure nothing changes and the error is thrown. */
export async function saveQuarantine(next: QuarantineSettings): Promise<void> {
  await setQuarantineSettings(next);
  Object.assign(quarantine, next);
}
