// User preferences kept in the webview's local storage, plus the quarantine
// choices, which Rust keeps (it needs them to expire entries on start).
// Reactive: components reading `settings` or `quarantine` re-render when a value changes.
import { getQuarantineOptions, setQuarantineSettings } from "./api";
import type { AiMode, DriveInfo, QuarantineSettings } from "./types";

const STORAGE_KEY = "settings";

/** The AI help (src/lib/ai.svelte.ts). The API keys are not here: Windows keeps them. */
export interface AiSettings {
  /** Off until the user turns it on. */
  mode: AiMode;
  /** A service id from src-tauri/src/ai/services.rs, e.g. "claude-code" or "ollama". */
  service: string;
  model: string;
  /** How the service names that model ("Claude Sonnet 5.5"). */
  modelLabel: string;
  /** Addresses typed by the user, by service ("other service", servers on this PC). */
  urls: Record<string, string>;
  /** Show what is sent before each question. */
  confirm: boolean;
}

interface Settings {
  /** Make a Windows restore point before uninstalling apps. */
  restorePoint: boolean;
  ai: AiSettings;
}

// Off by default: it needs administrator rights and System Restore is often off on new PCs.
const defaults: Settings = {
  restorePoint: false,
  ai: { mode: "off", service: "", model: "", modelLabel: "", urls: {}, confirm: true },
};

function load(): Settings {
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    const saved = typeof parsed === "object" && parsed ? parsed : {};
    const ai = typeof saved.ai === "object" && saved.ai ? saved.ai : {};
    return { ...defaults, ...saved, ai: { ...defaults.ai, ...ai, urls: { ...(ai.urls ?? {}) } } };
  } catch {
    return { ...defaults, ai: { ...defaults.ai, urls: {} } };
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
