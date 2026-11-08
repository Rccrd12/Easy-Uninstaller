// Single entry point for everything the UI asks the system.
// Inside the Tauri app calls go to Rust commands; in a plain browser
// (npm run dev) they fall back to sample data so the UI can be designed fast.
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { GB } from "./format";
import { sampleAiAnswer, sampleAiModels, sampleAiServices, sampleApps, sampleAppLeftovers, sampleFileGroups, sampleFolderSummary, sampleLeftovers, sampleQuarantine, sampleSpaceUsage } from "./mock";
import type {
  AiAnswer,
  AiConfig,
  AiModel,
  AiPreview,
  AiServiceStatus,
  AiStats,
  DiskInfo,
  FileGroup,
  FileItem,
  InstalledApp,
  LeftoverItem,
  MoveReport,
  QuarantineEntry,
  RestorePointOutcome,
  RestoreReport,
  ScanProgress,
  SpaceProgress,
  SpaceUsage,
  UninstallOutcome,
} from "./types";

/** Matches `KEEP_DAYS` in src-tauri/src/quarantine.rs. */
export const QUARANTINE_DAYS = 7;
const DAY = 24 * 60 * 60;

/**
 * Matches the window title bar to the chosen theme ("system" follows Windows).
 * Nothing to do in a plain browser, where there is no native title bar.
 */
export async function setWindowTheme(theme: "system" | "light" | "dark"): Promise<void> {
  if (!isTauri()) return;
  await getCurrentWindow()
    .setTheme(theme === "system" ? null : theme)
    .catch(() => {
      // Cosmetic only: the page colors already follow the choice.
    });
}

/** True in a plain browser, where every function returns sample data. */
export function usesSampleData(): boolean {
  return !isTauri();
}

/** Real: implemented in src-tauri/src/disk.rs. */
export async function getSystemDisk(): Promise<DiskInfo> {
  if (isTauri()) return invoke<DiskInfo>("get_system_disk");
  return { mountPoint: "C:", totalBytes: 476 * GB, freeBytes: 64 * GB };
}

let appsCache: Promise<InstalledApp[]> | null = null;

/**
 * Real: implemented in src-tauri/src/apps.rs (registry + Microsoft Store).
 * Reading sizes and icons takes a moment, so the result is kept until `refresh` is true.
 */
export function listInstalledApps(refresh = false): Promise<InstalledApp[]> {
  if (!isTauri()) return Promise.resolve(sampleApps.filter((a) => !sampleRemoved.has(a.id)));
  if (refresh || !appsCache) {
    const pending = invoke<InstalledApp[]>("list_installed_apps");
    pending.catch(() => {
      if (appsCache === pending) appsCache = null;
    });
    appsCache = pending;
  }
  return appsCache;
}

// One file scan at a time: Home and Files share it, and the result is kept
// until a new scan is asked for.
let fileScan: Promise<FileGroup[]> | null = null;
const scanListeners = new Set<(p: ScanProgress) => void>();

/**
 * Real: implemented in src-tauri/src/files.rs (read only).
 * `onProgress` gets updates while a scan runs; `rescan` starts a new scan.
 */
export function listFileGroups(opts: { onProgress?: (p: ScanProgress) => void; rescan?: boolean } = {}): Promise<FileGroup[]> {
  const { onProgress, rescan = false } = opts;
  if (onProgress) scanListeners.add(onProgress);
  if (!fileScan || rescan) {
    const notify = (p: ScanProgress) => scanListeners.forEach((l) => l(p));
    const scan = isTauri() ? scanFilesNative(notify) : sampleScan(notify);
    fileScan = scan;
    // A failed scan is not kept, so the next call tries again.
    scan.catch(() => fileScan === scan && (fileScan = null));
  }
  const done = () => onProgress && scanListeners.delete(onProgress);
  fileScan.then(done, done);
  return fileScan;
}

/** Replaces the kept scan result after files were moved (or brought back), so every screen agrees. */
export function keepFileGroups(groups: FileGroup[]): void {
  fileScan = Promise.resolve(groups);
}

function scanFilesNative(notify: (p: ScanProgress) => void): Promise<FileGroup[]> {
  const onProgress = new Channel<ScanProgress>();
  onProgress.onmessage = notify;
  return invoke<FileGroup[]>("scan_files", { onProgress });
}

/** Browser fallback: plays the scan steps quickly so the progress view can be designed. */
async function sampleScan(notify: (p: ScanProgress) => void): Promise<FileGroup[]> {
  const steps = ["caches", "downloads", "personal", "duplicates", "appFolders"] as const;
  for (const [i, step] of steps.entries()) {
    for (let n = 0; n < 4; n++) {
      notify({ step, stepIndex: i + 1, stepCount: steps.length, filesChecked: n * 1250 });
      await new Promise((r) => setTimeout(r, 120));
    }
  }
  return sampleFileGroups;
}

// ---------------------------------------------------------------------------
// Uninstalling and leftovers. Real: src-tauri/src/uninstall.rs and leftovers.rs.

/** Apps "uninstalled" in the browser fallback, so the list can be tried end to end. */
const sampleRemoved = new Set<string>();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/**
 * Runs the app's own uninstaller (or removes a Store app) and waits for it.
 * Rejects with "notInstalled", "noUninstaller", "itself", or another error text.
 */
export async function uninstallApp(id: string): Promise<UninstallOutcome> {
  if (isTauri()) return invoke<UninstallOutcome>("uninstall_app", { id });
  await sleep(1800);
  sampleRemoved.add(id);
  return "removed";
}

/** Whether the app is still in the app list (asked while its uninstaller runs). */
export async function isAppInstalled(id: string): Promise<boolean> {
  if (isTauri()) return invoke<boolean>("is_app_installed", { id });
  return !sampleRemoved.has(id);
}

/** What an app just uninstalled left behind. Only after it is gone from the app list. */
export async function findAppLeftovers(id: string): Promise<LeftoverItem[]> {
  if (isTauri()) return invoke<LeftoverItem[]>("find_app_leftovers", { id });
  await sleep(900);
  const app = sampleApps.find((a) => a.id === id);
  return app && app.source !== "store" ? sampleAppLeftovers(app.name) : [];
}

/** Leftovers of programs no longer installed: broken shortcuts and app-list entries, and leftovers of apps uninstalled here. */
export async function listLeftovers(): Promise<LeftoverItem[]> {
  if (isTauri()) return invoke<LeftoverItem[]>("list_leftovers");
  return sampleLeftovers.filter((l) => !sampleMovedLeftovers.has(l.id));
}

const sampleMovedLeftovers = new Set<string>();

/**
 * Moves leftovers into quarantine, one entry per app. Only ids are sent: Rust moves
 * exactly what its own search found. Windows asks once for those outside the user's folder.
 */
export async function quarantineLeftovers(ids: string[], found: LeftoverItem[]): Promise<MoveReport> {
  if (isTauri()) return invoke<MoveReport>("quarantine_leftovers", { ids });
  await sleep(700);
  const now = Math.floor(Date.now() / 1000);
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, skippedCount: 0, declinedCount: 0 };
  const picked = found.filter((l) => ids.includes(l.id));
  const byApp = new Map<string, LeftoverItem[]>();
  for (const l of picked) byApp.set(l.appName ?? "", [...(byApp.get(l.appName ?? "") ?? []), l]);
  for (const [app, items] of byApp) {
    const id = `${now}-leftovers-${sampleQuarantine.length}`;
    const size = items.reduce((s, l) => s + l.sizeBytes, 0);
    sampleQuarantine.push({
      id,
      titleKey: app ? null : "leftovers.title",
      title: app || null,
      itemCount: items.length,
      sizeBytes: size,
      removedAt: now,
      expiresAt: now + QUARANTINE_DAYS * DAY,
      items: items.map((l) => ({ originalPath: l.path, sizeBytes: l.sizeBytes, kind: l.kind === "registry" ? "registry" : l.kind === "folder" ? "folder" : "file" })),
      needsAdmin: items.some((l) => l.needsAdmin),
    });
    items.forEach((l) => sampleMovedLeftovers.add(l.id));
    report.entryIds.push(id);
    report.movedCount += items.length;
    report.movedBytes += size;
  }
  return report;
}

// One space scan at a time, kept until a new one is asked for (like the file scan).
let spaceScan: Promise<SpaceUsage> | null = null;
const spaceListeners = new Set<(p: SpaceProgress) => void>();

/**
 * Real: implemented in src-tauri/src/space.rs (read only). Measures the folders of the
 * user profile, temporary files and the Recycle Bin; apps come from `listInstalledApps`.
 */
export function scanSpace(opts: { onProgress?: (p: SpaceProgress) => void; rescan?: boolean } = {}): Promise<SpaceUsage> {
  const { onProgress, rescan = false } = opts;
  if (onProgress) spaceListeners.add(onProgress);
  if (!spaceScan || rescan) {
    const notify = (p: SpaceProgress) => spaceListeners.forEach((l) => l(p));
    const scan = isTauri() ? scanSpaceNative(notify) : sampleSpaceScan(notify);
    spaceScan = scan;
    scan.catch(() => spaceScan === scan && (spaceScan = null));
  }
  const done = () => onProgress && spaceListeners.delete(onProgress);
  spaceScan.then(done, done);
  return spaceScan;
}

function scanSpaceNative(notify: (p: SpaceProgress) => void): Promise<SpaceUsage> {
  const onProgress = new Channel<SpaceProgress>();
  onProgress.onmessage = notify;
  return invoke<SpaceUsage>("scan_space", { onProgress });
}

async function sampleSpaceScan(notify: (p: SpaceProgress) => void): Promise<SpaceUsage> {
  for (let n = 1; n <= 5; n++) {
    notify({ filesChecked: n * 18_000 });
    await new Promise((r) => setTimeout(r, 120));
  }
  return sampleSpaceUsage;
}

/** Real: src-tauri/src/quarantine.rs. Expired entries are deleted for good before listing. */
export async function listQuarantine(): Promise<QuarantineEntry[]> {
  if (isTauri()) return invoke<QuarantineEntry[]>("list_quarantine");
  return [...sampleQuarantine].sort((a, b) => b.removedAt - a.removedAt);
}

/**
 * Moves whole groups of the last file scan into quarantine (one entry per group).
 * Only group ids are sent: Rust moves exactly the files its own scan found.
 */
export async function quarantineFileGroups(groupIds: string[]): Promise<MoveReport> {
  if (isTauri()) return invoke<MoveReport>("quarantine_file_groups", { groupIds });
  const now = Math.floor(Date.now() / 1000);
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, skippedCount: 0, declinedCount: 0 };
  for (const g of sampleFileGroups.filter((x) => groupIds.includes(x.id))) {
    const id = `${now}-${g.id}-${sampleQuarantine.length}`;
    sampleQuarantine.push({
      id,
      titleKey: g.titleKey,
      title: null,
      itemCount: g.itemCount,
      sizeBytes: g.sizeBytes,
      removedAt: now,
      expiresAt: now + QUARANTINE_DAYS * DAY,
      items: g.items.map((i) => ({ originalPath: i.path, sizeBytes: i.sizeBytes, kind: "file" })),
      needsAdmin: false,
    });
    report.entryIds.push(id);
    report.movedCount += g.itemCount;
    report.movedBytes += g.sizeBytes;
  }
  return report;
}

/**
 * Real: src-tauri/src/app_folders.rs. Moves one folder of the "unknown folders" group into
 * quarantine as one entry. Rust checks the path again; folders outside the user's
 * profile (ProgramData) make Windows ask for permission.
 */
export async function quarantineAppFolder(item: FileItem): Promise<MoveReport> {
  if (isTauri()) return invoke<MoveReport>("quarantine_app_folder", { path: item.path });
  await sleep(700);
  const now = Math.floor(Date.now() / 1000);
  const id = `${now}-folder-${sampleQuarantine.length}`;
  const name = item.path.split(/[\\/]/).pop() ?? item.path;
  sampleQuarantine.push({
    id,
    titleKey: null,
    title: name,
    itemCount: 1,
    sizeBytes: item.sizeBytes,
    removedAt: now,
    expiresAt: now + QUARANTINE_DAYS * DAY,
    items: [{ originalPath: item.path, sizeBytes: item.sizeBytes, kind: "folder" }],
    needsAdmin: !/\\AppData\\/i.test(item.path),
  });
  return { entryIds: [id], movedCount: 1, movedBytes: item.sizeBytes, skippedCount: 0, declinedCount: 0 };
}

/** Puts the files of these entries back where they were. */
export async function restoreQuarantine(ids: string[]): Promise<RestoreReport> {
  if (isTauri()) return invoke<RestoreReport>("restore_quarantine", { ids });
  const report: RestoreReport = { restoredCount: 0, renamedCount: 0, failedCount: 0 };
  for (const id of ids) {
    const i = sampleQuarantine.findIndex((e) => e.id === id);
    if (i < 0) continue;
    const entry = sampleQuarantine.splice(i, 1)[0];
    report.restoredCount += entry.itemCount;
    for (const item of entry.items) {
      const l = sampleLeftovers.find((x) => x.path === item.originalPath);
      if (l) sampleMovedLeftovers.delete(l.id);
    }
  }
  return report;
}

/** Deletes an entry for good. */
export async function deleteQuarantine(id: string): Promise<void> {
  if (isTauri()) return invoke("delete_quarantine", { id });
  const i = sampleQuarantine.findIndex((e) => e.id === id);
  if (i >= 0) sampleQuarantine.splice(i, 1);
}

/** Real: src-tauri/src/restore_point.rs. Windows asks the user for permission. */
export async function createRestorePoint(): Promise<RestorePointOutcome> {
  if (isTauri()) return invoke<RestorePointOutcome>("create_restore_point");
  await new Promise((r) => setTimeout(r, 1200));
  return "created";
}

// ── AI help (src-tauri/src/ai.rs) ──────────────────────────────────────────
// Errors are i18n keys under "ai.error", sometimes followed by "|" and the service's words.

const sampleKeys = new Set<string>();
const sampleAnswersByPath = new Map<string, AiAnswer>();
let sampleAsked = 0;
const lower = (p: string) => p.toLowerCase();

/** Every service and whether it can be used now. `urls`: the user's addresses, by service. */
export async function aiServices(urls: Record<string, string>): Promise<AiServiceStatus[]> {
  if (isTauri()) return invoke<AiServiceStatus[]>("ai_services", { urls });
  await sleep(500);
  return sampleAiServices.map((s) => ({ ...s, hasKey: sampleKeys.has(s.id) }));
}

/** The models a service offers (installed ones, for a server on this PC). */
export async function aiModels(config: AiConfig): Promise<AiModel[]> {
  if (isTauri()) return invoke<AiModel[]>("ai_models", { config });
  await sleep(400);
  const needsKey = ["anthropic", "openai", "google", "openrouter", "unsloth"].includes(config.service);
  if (needsKey && !sampleKeys.has(config.service)) throw "ai.error.noKey";
  return sampleAiModels[config.service] ?? [];
}

/** Saves an API key in Windows' Credential Manager. */
export async function aiSetKey(service: string, key: string): Promise<void> {
  if (isTauri()) return invoke("ai_set_key", { service, key });
  if (key.trim().length < 8 || /\s/.test(key.trim())) throw "ai.error.keyFormat";
  sampleKeys.add(service);
}

export async function aiRemoveKey(service: string): Promise<void> {
  if (isTauri()) return invoke("ai_remove_key", { service });
  sampleKeys.delete(service);
}

/** Sends a tiny question; resolves with how long the answer took, in milliseconds. */
export async function aiTest(config: AiConfig): Promise<number> {
  if (isTauri()) return invoke<number>("ai_test", { config });
  await sleep(1200);
  return config.service === "ollama" || config.service === "lmstudio" ? 14_000 : 6_000;
}

/** What would be sent about this folder, read again by Rust from the disk. */
export async function aiPreview(item: FileItem, lang: string): Promise<AiPreview> {
  if (isTauri()) return invoke<AiPreview>("ai_preview", { path: item.path, lang });
  await sleep(150);
  const summary = sampleFolderSummary(item.path, item.sizeBytes);
  return { summary, question: `Folder: ${summary.path}\nSubfolders: ${summary.subfolders.join(", ")}\nSome file names: ${summary.names.join(", ")}` };
}

/** Asks the AI about one unknown folder. The answer is kept on this PC. */
export async function aiAsk(config: AiConfig, item: FileItem, lang: string): Promise<AiAnswer> {
  if (isTauri()) return invoke<AiAnswer>("ai_ask", { config, path: item.path, lang });
  await sleep(1600);
  const answer = sampleAiAnswer(item.path, lang, config.service, config.model);
  sampleAnswersByPath.set(lower(item.path), answer);
  sampleAsked++;
  return answer;
}

/** The answers kept on this PC for these folders. */
export async function aiSavedAnswers(paths: string[]): Promise<AiAnswer[]> {
  if (isTauri()) return invoke<AiAnswer[]>("ai_saved_answers", { paths });
  return paths.flatMap((p) => sampleAnswersByPath.get(lower(p)) ?? []);
}

export async function aiStats(): Promise<AiStats> {
  if (isTauri()) return invoke<AiStats>("ai_stats");
  return { answers: sampleAnswersByPath.size, askedThisMonth: sampleAsked };
}

/** Forgets kept answers: those of `paths`, or all of them. */
export async function aiForgetAnswers(paths?: string[]): Promise<void> {
  if (isTauri()) return invoke("ai_forget_answers", { paths: paths ?? null });
  if (!paths) sampleAnswersByPath.clear();
  else for (const p of paths) sampleAnswersByPath.delete(lower(p));
}

/** Opens a fixed help page (where to get a key, how to install a program) in the browser. */
export async function aiOpenPage(page: string): Promise<void> {
  if (isTauri()) return invoke("ai_open_page", { page });
}
