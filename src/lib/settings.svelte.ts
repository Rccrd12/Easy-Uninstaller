// User preferences kept in the webview's local storage, plus the quarantine
// choices, which Rust keeps (it needs them to expire entries on start).
// Reactive: components reading `settings` or `quarantine` re-render when a value changes.
import { getQuarantineOptions, setPageZoom, setQuarantineSettings, usesSampleData } from "./api";
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
  /** Start Unsloth Studio when a question needs it, and stop it after a while. */
  autoStart: boolean;
  /** How many questions "Ask for all" sends at the same time. */
  parallel: number;
}

interface Settings {
  /** Make a Windows restore point before uninstalling apps. */
  restorePoint: boolean;
  /** Ask GitHub whether a newer version exists each time the app opens. */
  checkUpdates: boolean;
  /** Bigger text and buttons: the whole window is zoomed by `LARGE_TEXT_ZOOM`. */
  largeText: boolean;
  /** Show full paths, registry keys, versions and install folders instead of plain names. */
  technical: boolean;
  ai: AiSettings;
}

// Restore points are off by default: they need administrator rights and System Restore is often off on new PCs.
// Update checks are on: most people never press a "check" button, and only the version is asked.
const defaults: Settings = {
  restorePoint: false,
  checkUpdates: true,
  largeText: false,
  technical: false,
  ai: { mode: "off", service: "", model: "", modelLabel: "", urls: {}, confirm: true, autoStart: true, parallel: 4 },
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

/** How much "Larger text" zooms the window: text, buttons and spacing grow together. */
export const LARGE_TEXT_ZOOM = 1.2;
/** The smallest page the layout is made for: the window's minimum size in tauri.conf.json. */
const MIN_PAGE = { width: 900, height: 600 };

let appliedZoom = 1;
let listening = false;

/**
 * Applies the text size choice; call once before mounting the app and after each change.
 * The zoom shrinks the page the layout sees, so in a small window it grows less:
 * the page never gets smaller than the window's minimum size.
 */
export function applyTextSize(): void {
  // The webview zoom also shrinks innerWidth; the browser fallback (CSS zoom) does not.
  const scale = usesSampleData() ? 1 : appliedZoom;
  const fit = Math.min((window.innerWidth * scale) / MIN_PAGE.width, (window.innerHeight * scale) / MIN_PAGE.height);
  const zoom = settings.largeText ? Math.max(1, Math.min(LARGE_TEXT_ZOOM, Math.floor(fit * 100) / 100)) : 1;
  if (!listening) {
    window.addEventListener("resize", applyTextSize);
    listening = true;
  }
  if (Math.abs(zoom - appliedZoom) < 0.01) return;
  appliedZoom = zoom;
  void setPageZoom(zoom);
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
