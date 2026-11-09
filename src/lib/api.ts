// Single entry point for everything the UI asks the system.
// Inside the Tauri app calls go to Rust commands; in a plain browser
// (npm run dev) they fall back to sample data so the UI can be designed fast.
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { GB } from "./format";
import {
  buildSampleGroups,
  sampleAiAnswer,
  sampleAiModels,
  sampleAiServices,
  sampleFolderSummary,
  sampleApps,
  sampleAppLeftovers,
  sampleDrives,
  sampleDuplicateSets,
  sampleFiles,
  sampleLeftovers,
  samplePreview,
  sampleQuarantine,
  sampleQuarantineSettings,
  sampleLooseDownloads,
  sampleSearch,
  sampleSearchPool,
  sampleSpaceUsage,
  sampleTidyLines,
  sampleTidyPlan,
  sampleUnderstand,
  type SampleFile,
} from "./mock";
import type {
  AiAnswer,
  AiConfig,
  AiModel,
  AiPreview,
  AiServiceStatus,
  AiStartStatus,
  AiStats,
  DiskInfo,
  Exclusion,
  FileGroup,
  FileItem,
  FileSelection,
  GroupReview,
  HiddenKind,
  InstalledApp,
  LeftoverItem,
  MoveReport,
  QuarantineEntry,
  QuarantineOptions,
  QuarantineSettings,
  RestorePointOutcome,
  RestoreReport,
  ReviewFile,
  ScanProgress,
  SearchFilter,
  SearchResult,
  SpaceProgress,
  SpaceUsage,
  TidyChoice,
  TidyOverview,
  TidyPlan,
  TidyPreview,
  TidyReport,
  TidyUndoReport,
  UninstallOutcome,
  UpdateInfo,
  UpdateProgress,
} from "./types";

/** Matches `KEEP_DAYS_CHOICES` in src-tauri/src/quarantine.rs. */
export const KEEP_DAYS_CHOICES = [3, 7, 30] as const;
/**
 * File groups deleted right away instead of going to Quarantine: Windows and apps
 * recreate them. Matches `DELETE_NOW_GROUPS` in src-tauri/src/cleanup.rs.
 */
export const DELETE_NOW_GROUPS: readonly string[] = ["temp", "recycle-bin", "crash-reports", "thumbnails"];
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

/** Browser fallback: grows when sample files are removed, like a real disk would. */
let sampleFreeBytes = 64 * GB;

/** Real: implemented in src-tauri/src/disk.rs. */
export async function getSystemDisk(): Promise<DiskInfo> {
  if (isTauri()) return invoke<DiskInfo>("get_system_disk");
  return { mountPoint: "C:", totalBytes: 476 * GB, freeBytes: sampleFreeBytes };
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
  return sampleGroups();
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
  return app && app.source !== "store" ? sampleAppLeftovers(app.name).filter((l) => !sampleHidesLeftover(l.path)) : [];
}

/** Leftovers of programs no longer installed: broken shortcuts and app-list entries, and leftovers of apps uninstalled here. */
export async function listLeftovers(): Promise<LeftoverItem[]> {
  if (isTauri()) return invoke<LeftoverItem[]>("list_leftovers");
  return sampleLeftovers.filter((l) => !sampleMovedLeftovers.has(l.id) && !sampleHidesLeftover(l.path));
}

const sampleHidesLeftover = (path: string) => sampleHidden.some((e) => e.kind === "leftover" && e.path.toLowerCase() === path.toLowerCase());

const sampleMovedLeftovers = new Set<string>();

/**
 * Moves leftovers into quarantine, one entry per app. Only ids are sent: Rust moves
 * exactly what its own search found. Windows asks once for those outside the user's folder.
 */
export async function quarantineLeftovers(ids: string[], found: LeftoverItem[]): Promise<MoveReport> {
  if (isTauri()) return invoke<MoveReport>("quarantine_leftovers", { ids });
  await sleep(700);
  const now = Math.floor(Date.now() / 1000);
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 };
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
      expiresAt: now + sampleQuarantineSettings.keepDays * DAY,
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
 * Removes whole groups of the last file scan: the groups in `DELETE_NOW_GROUPS` are
 * deleted right away, every other group goes to Quarantine (one entry per group).
 * Only group ids are sent: Rust removes exactly the files its own scan found.
 * Real: src-tauri/src/cleanup.rs.
 */
export async function removeFileGroups(groupIds: string[]): Promise<MoveReport> {
  if (isTauri()) return invoke<MoveReport>("remove_file_groups", { groupIds });
  await new Promise((r) => setTimeout(r, 900));
  const now = Math.floor(Date.now() / 1000);
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 };
  for (const g of sampleGroups().filter((x) => groupIds.includes(x.id) && !x.reviewOnly)) {
    if (DELETE_NOW_GROUPS.includes(g.id)) {
      report.deletedCount += g.itemCount;
      report.deletedBytes += g.sizeBytes;
      report.freedBytes += g.sizeBytes;
      g.items.forEach((i) => sampleGone.add(i.path));
      continue;
    }
    sampleMove(report, g, g.items.map((i) => ({ path: i.path, sizeBytes: i.sizeBytes })), g.itemCount, now);
  }
  sampleFreeBytes += report.freedBytes;
  return report;
}

/** Real: src-tauri/src/quarantine.rs. The quarantine choices and the drives it can use. */
export async function getQuarantineOptions(): Promise<QuarantineOptions> {
  if (isTauri()) return invoke<QuarantineOptions>("get_quarantine_options");
  return { settings: { ...sampleQuarantineSettings }, systemDrive: "C:", drives: sampleDrives };
}

/** Saves the quarantine choices. Fails if the chosen drive cannot be written to. */
export async function setQuarantineSettings(settings: QuarantineSettings): Promise<void> {
  if (isTauri()) return invoke("set_quarantine_settings", { settings });
  Object.assign(sampleQuarantineSettings, settings);
}

/** Browser fallback: records a move in the sample quarantine and hides the files. */
function sampleMove(report: MoveReport, g: FileGroup, files: { path: string; sizeBytes: number }[], count: number, now: number) {
  const id = `${now}-${g.id}-${sampleQuarantine.length}`;
  const size = files.reduce((s, f) => s + f.sizeBytes, 0);
  sampleQuarantine.push({
    id,
    titleKey: g.titleKey,
    title: null,
    itemCount: count,
    sizeBytes: size,
    removedAt: now,
    expiresAt: now + sampleQuarantineSettings.keepDays * DAY,
    items: files.map((f) => ({ originalPath: f.path, sizeBytes: f.sizeBytes, kind: "file" })),
    needsAdmin: false,
  });
  files.forEach((f) => sampleGone.add(f.path));
  report.entryIds.push(id);
  report.movedCount += count;
  report.movedBytes += size;
  if (sampleQuarantineSettings.drive) report.freedBytes += size;
}

/**
 * Removes the files the user picked, like `removeFileGroups`: one quarantine entry per
 * group, or deleted right away for `DELETE_NOW_GROUPS`. Rust only touches files its own
 * scan found, and always leaves one copy of every duplicate set. Real: src-tauri/src/review.rs.
 */
export async function quarantineFileSelection(selections: FileSelection[]): Promise<MoveReport> {
  if (isTauri()) return invoke<MoveReport>("quarantine_file_selection", { selections });
  const now = Math.floor(Date.now() / 1000);
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 };
  const groups = sampleGroups();
  for (const sel of selections) {
    const g = groups.find((x) => x.id === sel.groupId);
    if (!g) continue;
    const wanted = new Set(sel.paths);
    let picked: SampleFile[];
    if (g.id === "duplicates") {
      picked = sampleDuplicateSets.flatMap((set) => {
        const left = set.filter((f) => !sampleGone.has(f.path));
        const chosen = left.filter((f) => wanted.has(f.path) && !sampleHides(f.path));
        return chosen.length && chosen.length >= left.length ? chosen.slice(1) : chosen;
      });
    } else {
      picked = sampleFiles.filter((f) => f.group === g.id && wanted.has(f.path) && samplePresent(f.path));
    }
    if (!picked.length) continue;
    if (DELETE_NOW_GROUPS.includes(g.id)) {
      const size = picked.reduce((s, f) => s + f.sizeBytes, 0);
      report.deletedCount += picked.length;
      report.deletedBytes += size;
      report.freedBytes += size;
      picked.forEach((f) => sampleGone.add(f.path));
      continue;
    }
    sampleMove(report, g, picked, picked.length, now);
  }
  sampleFreeBytes += report.freedBytes;
  return report;
}

/** The groups of the last scan as they are now, after a move, an undo or a hidden file. */
export async function refreshFileGroups(): Promise<FileGroup[]> {
  const groups = isTauri() ? await invoke<FileGroup[]>("current_file_groups") : sampleGroups();
  keepFileGroups(groups);
  return groups;
}

/** Every file of a group, to review them one by one. null when the group is gone. */
export async function reviewFileGroup(groupId: string): Promise<GroupReview | null> {
  if (isTauri()) return invoke<GroupReview | null>("review_file_group", { groupId });
  await new Promise((r) => setTimeout(r, 150));
  const review = (f: SampleFile): ReviewFile => ({
    path: f.path,
    sizeBytes: f.sizeBytes,
    lastUsed: f.lastUsed,
    modified: f.modified,
    canOpen: !/\.(exe|msi)$/i.test(f.path),
  });
  if (groupId === "duplicates") {
    const sets = sampleDuplicateSets
      .map((set) => set.filter((f) => samplePresent(f.path)))
      .filter((set) => set.length > 1)
      .sort((a, b) => b[0].sizeBytes * (b.length - 1) - a[0].sizeBytes * (a.length - 1));
    return {
      groupId,
      totalCount: sets.reduce((s, set) => s + set.length - 1, 0),
      totalBytes: sets.reduce((s, set) => s + set[0].sizeBytes * (set.length - 1), 0),
      files: [],
      duplicateSets: sets.map((set) => set.map(review)),
    };
  }
  const files = sampleFiles.filter((f) => f.group === groupId && samplePresent(f.path)).sort((a, b) => b.sizeBytes - a.sizeBytes);
  return { groupId, totalCount: files.length, totalBytes: files.reduce((s, f) => s + f.sizeBytes, 0), files: files.map(review), duplicateSets: [] };
}

// Browser fallback: what was moved or hidden in this session.
const sampleGone = new Set<string>();
const sampleHidden: Exclusion[] = [];
const sampleHides = (path: string) =>
  sampleHidden.some((e) => (e.kind === "file" && path === e.path) || (e.kind === "folder" && (path === e.path || path.startsWith(`${e.path}\\`))));
const samplePresent = (path: string) => !sampleGone.has(path) && !sampleHides(path);
const sampleGroups = () => buildSampleGroups(samplePresent).filter((g) => !sampleHidden.some((e) => e.kind === "group" && e.path === g.id));

/** A hidden item, ready to save (the date is set when saved). */
export const hiddenItem = (kind: HiddenKind, path: string, label: string | null = null): Exclusion => ({ kind, path, label, hiddenAt: 0 });

/** Everything the user chose to hide, oldest first. */
export async function listExclusions(): Promise<Exclusion[]> {
  if (isTauri()) return invoke<Exclusion[]>("list_exclusions");
  return [...sampleHidden];
}

/**
 * Hides files, folders, whole groups or leftovers: they stay where they are and are
 * never shown or moved again. Nothing is deleted.
 */
export async function addExclusions(items: Exclusion[]): Promise<void> {
  if (isTauri()) await invoke("add_exclusions", { items });
  else {
    const now = Math.floor(Date.now() / 1000);
    for (const item of items) {
      const i = sampleHidden.findIndex((e) => e.path === item.path && e.kind === item.kind);
      if (i >= 0) sampleHidden.splice(i, 1);
      sampleHidden.push({ ...item, hiddenAt: item.hiddenAt || now });
    }
  }
  refreshKeptScan();
}

/** Shows hidden items again. Nothing moves: they are listed again. */
export async function removeExclusions(items: Pick<Exclusion, "path" | "kind">[]): Promise<void> {
  if (isTauri()) await invoke("remove_exclusions", { items: items.map(({ path, kind }) => ({ path, kind })) });
  else {
    for (const item of items) {
      const i = sampleHidden.findIndex((e) => e.path === item.path && e.kind === item.kind);
      if (i >= 0) sampleHidden.splice(i, 1);
    }
  }
  refreshKeptScan();
}

/** After hiding or showing again, the kept scan result is rebuilt so every screen agrees. */
function refreshKeptScan() {
  if (!fileScan) return;
  const next = fileScan.then(() => (isTauri() ? invoke<FileGroup[]>("current_file_groups") : sampleGroups()));
  fileScan = next;
  next.catch(() => fileScan === next && (fileScan = null));
}

/**
 * The preview Windows Explorer shows for a photo, video or document, as an image URL;
 * null when there is none (the UI then shows a tile for the file type).
 */
export async function fileThumbnail(path: string, size: number): Promise<string | null> {
  if (isTauri()) return invoke<string | null>("file_thumbnail", { path, size });
  return samplePreview(path);
}

/** Opens a photo, video or document found by the scan in its usual app. */
export async function openScannedFile(path: string): Promise<void> {
  if (isTauri()) return invoke("open_scanned_file", { path });
}

/** Opens the file's folder in Explorer with the file selected. */
export async function showScannedFile(path: string): Promise<void> {
  if (isTauri()) return invoke("show_scanned_file", { path });
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
    expiresAt: now + sampleQuarantineSettings.keepDays * DAY,
    items: [{ originalPath: item.path, sizeBytes: item.sizeBytes, kind: "folder" }],
    needsAdmin: !/\\AppData\\/i.test(item.path),
  });
  sampleGone.add(item.path);
  return { entryIds: [id], movedCount: 1, movedBytes: item.sizeBytes, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 };
}

/** Puts the files of these entries back where they were. */
export async function restoreQuarantine(ids: string[]): Promise<RestoreReport> {
  if (isTauri()) return invoke<RestoreReport>("restore_quarantine", { ids });
  const report: RestoreReport = { restoredCount: 0, renamedCount: 0, failedCount: 0 };
  for (const id of ids) {
    const i = sampleQuarantine.findIndex((e) => e.id === id);
    if (i < 0) continue;
    const [entry] = sampleQuarantine.splice(i, 1);
    entry.items.forEach((item) => sampleGone.delete(item.originalPath));
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
  return sampleAiServices.map((s) => ({ ...s, hasKey: sampleKeys.has(s.id), ...(s.id === "unsloth" && sampleUnslothOn ? { found: true, models: 1 } : {}) }));
}

/** How many questions "Ask for all" may send at once. */
export const AI_PARALLEL_CHOICES = [1, 2, 4, 8];

const startListeners = new Set<(s: AiStartStatus) => void>();
let sampleUnslothOn = false;

/** Hears Unsloth Studio being started by the app. Returns a function that stops listening. */
export async function onAiStart(cb: (s: AiStartStatus) => void): Promise<() => void> {
  if (isTauri()) return listen<AiStartStatus>("ai-unsloth", (e) => cb(e.payload));
  startListeners.add(cb);
  return () => startListeners.delete(cb);
}

/** In the browser: Unsloth Studio "starts" in a few seconds, like the real one. */
async function sampleEnsure(config: AiConfig): Promise<void> {
  if (config.service !== "unsloth" || sampleUnslothOn || !config.autoStart || !sampleKeys.has("unsloth")) return;
  for (let s = 0; s < 4; s++) {
    startListeners.forEach((cb) => cb({ state: "starting", seconds: s }));
    await sleep(1000);
  }
  sampleUnslothOn = true;
  startListeners.forEach((cb) => cb({ state: "on", seconds: 0 }));
}

/** The models a service offers (installed ones, for a server on this PC). */
export async function aiModels(config: AiConfig): Promise<AiModel[]> {
  if (isTauri()) return invoke<AiModel[]>("ai_models", { config });
  await sampleEnsure(config);
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
  await sampleEnsure(config);
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
  await sampleEnsure(config);
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

// ── Finding files by a sentence (src-tauri/src/search.rs) ─────────────────
// Only the sentence goes to the AI; the search runs on this PC.

/** Asks the AI to turn a sentence into a search filter. Errors are i18n keys ("search.error.*", "ai.error.*"). */
export async function searchUnderstand(config: AiConfig, sentence: string, lang: string): Promise<SearchFilter> {
  if (isTauri()) return invoke<SearchFilter>("search_understand", { config, sentence, lang });
  await sampleEnsure(config);
  await sleep(1400);
  return sampleUnderstand(sentence);
}

/** Runs the search on this PC (read only). Rust keeps the files found for a later move. */
export async function searchFiles(filter: SearchFilter): Promise<SearchResult> {
  if (isTauri()) return invoke<SearchResult>("search_files", { filter });
  await sleep(500);
  return sampleSearch(filter, sampleSearchPool.filter((f) => samplePresent(f.path)));
}

/** Moves picked files of the last search to Quarantine, as one entry. Only files that search found. */
export async function quarantineFound(paths: string[], found: SearchResult): Promise<MoveReport> {
  if (isTauri()) return invoke<MoveReport>("quarantine_found", { paths });
  await sleep(600);
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 };
  const picked = found.files.filter((f) => paths.includes(f.path) && samplePresent(f.path));
  if (!picked.length) return report;
  const group = { id: "found", titleKey: "files.groups.found.title" } as FileGroup;
  sampleMove(report, group, picked, picked.length, Math.floor(Date.now() / 1000));
  return report;
}

// ── Tidying Downloads (src-tauri/src/tidy.rs) ──────────────────────────────

/** Sample tidies, newest last, so undo can be tried in the browser. */
const sampleTidyRuns: { id: string; at: number; moves: SampleFile[]; folderCount: number }[] = [];
const sampleTidied = new Set<string>();
const sampleLoose = () => sampleLooseDownloads.filter((f) => samplePresent(f.path) && !sampleTidied.has(f.path));

/** How many loose files Downloads has, and the last tidy that can be undone. */
export async function tidyOverview(): Promise<TidyOverview> {
  if (isTauri()) return invoke<TidyOverview>("tidy_overview");
  await sleep(300);
  const loose = sampleLoose();
  const last = sampleTidyRuns.at(-1);
  return {
    downloads: `C:\\Users\\you\\Downloads`,
    fileCount: loose.length,
    sizeBytes: loose.reduce((s, f) => s + f.sizeBytes, 0),
    lastRun: last ? { id: last.id, at: last.at, movedCount: last.moves.length, folderCount: last.folderCount } : null,
  };
}

/** What would be sent to the AI: one line per file (name, size, month). */
export async function tidyPreview(lang: string): Promise<TidyPreview> {
  if (isTauri()) return invoke<TidyPreview>("tidy_preview", { lang });
  await sleep(200);
  const lines = sampleTidyLines(sampleLoose());
  return { lines, question: lines.join("\n"), notSentCount: 0 };
}

/** Asks the AI to propose folders. Nothing moves until `tidyApply`. */
export async function tidyAsk(config: AiConfig, lang: string): Promise<TidyPlan> {
  if (isTauri()) return invoke<TidyPlan>("tidy_ask", { config, lang });
  await sampleEnsure(config);
  await sleep(2200);
  return sampleTidyPlan(sampleLoose(), lang, true);
}

/** One folder per kind of file, without the AI. */
export async function tidyByType(lang: string): Promise<TidyPlan> {
  if (isTauri()) return invoke<TidyPlan>("tidy_by_type", { lang });
  await sleep(400);
  return sampleTidyPlan(sampleLoose(), lang, false);
}

/**
 * Moves the files the user kept ticked into the proposed folders, and the picked
 * setup files to Quarantine. Rust takes only files of its last proposal, unchanged since.
 */
export async function tidyApply(choices: TidyChoice[], installers: string[], plan: TidyPlan): Promise<TidyReport> {
  if (isTauri()) return invoke<TidyReport>("tidy_apply", { choices, installers });
  await sleep(900);
  const quarantine: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 };
  const paths = new Set(choices.flatMap((c) => c.paths));
  const moves = sampleLooseDownloads.filter((f) => paths.has(f.path));
  moves.forEach((f) => sampleTidied.add(f.path));
  const folderCount = choices.filter((c) => c.paths.length).length;
  const id = String(Date.now());
  if (moves.length) sampleTidyRuns.push({ id, at: Math.floor(Date.now() / 1000), moves, folderCount });
  const picked = plan.installers.filter((f) => installers.includes(f.path));
  if (picked.length) sampleMove(quarantine, { id: "installers", titleKey: "files.groups.installers.title" } as FileGroup, picked, picked.length, Math.floor(Date.now() / 1000));
  return { movedCount: moves.length, movedBytes: moves.reduce((s, f) => s + f.sizeBytes, 0), skippedCount: 0, folderCount, runId: moves.length ? id : null, quarantine };
}

/** Puts the files of a tidy back in Downloads; the folders it made go away when empty. */
export async function tidyUndo(runId: string): Promise<TidyUndoReport> {
  if (isTauri()) return invoke<TidyUndoReport>("tidy_undo", { runId });
  await sleep(500);
  const i = sampleTidyRuns.findIndex((r) => r.id === runId);
  if (i < 0) throw "tidy.error.noRun";
  const [run] = sampleTidyRuns.splice(i, 1);
  run.moves.forEach((f) => sampleTidied.delete(f.path));
  return { restoredCount: run.moves.length, failedCount: 0 };
}

// ---------------------------------------------------------------------------
// Updates. Real: src-tauri/src/updater.rs (GitHub Releases, signed installers).

/**
 * Asks whether a newer version exists; null when this one is the latest.
 * Rejects with "offline" when the internet cannot be reached, or another error text.
 * In the browser, add ?update=none to the address to see the "up to date" state.
 */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  if (isTauri()) return invoke<UpdateInfo | null>("check_for_update");
  await sleep(900);
  if (new URLSearchParams(location.search).get("update") === "none") return null;
  return { version: "0.3.0", currentVersion: "0.2.0" };
}

/**
 * Downloads and installs the update found by the last check. In the app this never
 * resolves on Windows: the installer closes the app and opens the new version.
 */
export async function installUpdate(notify: (p: UpdateProgress) => void): Promise<void> {
  if (isTauri()) {
    const onProgress = new Channel<UpdateProgress>();
    onProgress.onmessage = notify;
    return invoke("install_update", { onProgress });
  }
  const total = 4.2 * 1024 * 1024;
  for (let n = 1; n <= 20; n++) {
    await sleep(150);
    notify({ downloadedBytes: (total * n) / 20, totalBytes: total });
  }
  // The browser cannot restart into a new version; keep showing "installing".
  return new Promise(() => {});
}
