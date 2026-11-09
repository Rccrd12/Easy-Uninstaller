// Single entry point for everything the UI asks the system.
// Inside the Tauri app calls go to Rust commands; in a plain browser
// (npm run dev) they fall back to sample data so the UI can be designed fast.
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { GB } from "./format";
import {
  buildSampleGroups,
  sampleAiAnswer,
  sampleAiModels,
  sampleAiServices,
  sampleFolderSummary,
  sampleApps,
  sampleAppCaches,
  sampleAppLeftovers,
  sampleDrives,
  sampleDuplicateSets,
  sampleFiles,
  sampleHistory,
  sampleSimilarSets,
  sampleLeftovers,
  sampleMoveFiles,
  sampleOnlineBytes,
  sampleOnlineCount,
  sampleOnlineFiles,
  samplePreview,
  sampleQuarantine,
  sampleQuarantineSettings,
  sampleLooseDownloads,
  sampleSearch,
  sampleSearchPool,
  sampleSpaceUsage,
  sampleStartup,
  sampleUninstalledApps,
  sampleTidyLines,
  sampleTidyPlan,
  sampleUnderstand,
  type SampleFile,
  type SampleRecord,
} from "./mock";
import type {
  AiAnswer,
  AiConfig,
  AiModel,
  AiPreview,
  AiServiceStatus,
  AiStartStatus,
  AiStats,
  AppCache,
  ClearCacheOutcome,
  DiskInfo,
  DriveMoveReport,
  Exclusion,
  FileGroup,
  FileItem,
  FileSelection,
  GroupReview,
  HiddenKind,
  HistoryItem,
  HistoryKind,
  HistoryName,
  HistorySummary,
  InstalledApp,
  LeftoverItem,
  MoveBackReport,
  MovedFile,
  MoveProgress,
  MoveReport,
  OffloadFile,
  OffloadScan,
  OnlineReport,
  QuarantineEntry,
  QuarantineOptions,
  QuarantineSettings,
  ReminderSettings,
  ReminderTexts,
  RestorePointOutcome,
  RestoreReport,
  ReviewFile,
  ScanProgress,
  SearchFilter,
  SearchResult,
  SpaceProgress,
  SpaceUsage,
  StartupApp,
  StartupChange,
  StartupReport,
  TidyChoice,
  TidyOverview,
  TidyPlan,
  TidyPreview,
  TidyReport,
  TidyUndoReport,
  UninstallOutcome,
  UninstalledApp,
  UpdateInfo,
  UpdateProgress,
} from "./types";

/** Matches `KEEP_DAYS_CHOICES` in src-tauri/src/quarantine.rs. */
export const KEEP_DAYS_CHOICES = [3, 7, 30] as const;
/**
 * File groups deleted right away instead of going to Quarantine: Windows and apps
 * recreate them. Matches `DELETE_NOW_GROUPS` in src-tauri/src/cleanup.rs.
 */
export const DELETE_NOW_GROUPS: readonly string[] = ["temp", "recycle-bin", "crash-reports", "thumbnails", "app-cache"];
/**
 * File groups moved only from their review, set by set, never ticked as a whole: photos
 * that look alike are still different photos. Matches `movable` in src-tauri/src/review.rs.
 */
export const REVIEW_FIRST_GROUPS: readonly string[] = ["similar-photos"];
/** Groups made of sets (one file of each always stays), with their sample sets. */
const sampleSets = (groupId: string): SampleFile[][] | null =>
  groupId === "duplicates" ? sampleDuplicateSets : groupId === "similar-photos" ? sampleSimilarSets : null;
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

/**
 * Zooms the whole page, like Ctrl + in a browser: 1 is the normal size.
 * The webview zoom keeps layout, menus and the window size consistent;
 * the browser fallback uses CSS zoom, which is close enough to design with.
 */
export async function setPageZoom(factor: number): Promise<void> {
  if (!isTauri()) {
    document.documentElement.style.zoom = factor === 1 ? "" : String(factor);
    return;
  }
  await getCurrentWebview()
    .setZoom(factor)
    .catch(() => {
      // The text stays at its normal size.
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

// Programs that start with Windows. Real: src-tauri/src/startup.rs.

/** The sample list, switched on and off in the browser fallback. */
let sampleStartupState: StartupApp[] | null = null;

/** Programs that start with Windows: on first, biggest impact first. */
export async function listStartupApps(): Promise<StartupApp[]> {
  if (isTauri()) return invoke<StartupApp[]>("list_startup_apps");
  await new Promise((r) => setTimeout(r, 300));
  sampleStartupState ??= sampleStartup.map((a) => ({ ...a }));
  return sampleStartupState.map((a) => ({ ...a }));
}

/**
 * Turns programs on or off at startup, like Task Manager: nothing is uninstalled or removed.
 * Programs for all users need the Windows permission prompt (one for all of them).
 */
export async function setStartupApps(changes: StartupChange[]): Promise<StartupReport> {
  if (isTauri()) return invoke<StartupReport>("set_startup_apps", { changes });
  await new Promise((r) => setTimeout(r, 400));
  const report: StartupReport = { changed: [], failed: [], declined: [] };
  for (const c of changes) {
    const app = sampleStartupState?.find((a) => a.id === c.id);
    if (!app || app.advice === "system") report.failed.push(c.id);
    else {
      app.enabled = c.enabled;
      report.changed.push(c.id);
    }
  }
  return report;
}

let cachesScan: Promise<AppCache[]> | null = null;

/**
 * The caches of well-known apps (Teams, Spotify, Discord, browsers...) on this PC.
 * Real: src-tauri/src/app_cache.rs (read only). Kept until `refresh` is true.
 */
export function listAppCaches(refresh = false): Promise<AppCache[]> {
  if (!isTauri()) return sleep(600).then(() => sampleAppCaches.map((c) => ({ ...c })));
  if (refresh || !cachesScan) {
    const pending = invoke<AppCache[]>("list_app_caches");
    pending.catch(() => {
      if (cachesScan === pending) cachesScan = null;
    });
    cachesScan = pending;
  }
  return cachesScan;
}

/**
 * Empties an app's cache without uninstalling it. Nothing happens while the app is
 * open (`running` is true): the user closes it and tries again. The files are deleted
 * right away (group "app-cache" in `DELETE_NOW_GROUPS`): the app recreates them.
 */
export async function clearAppCache(id: string): Promise<ClearCacheOutcome> {
  if (isTauri()) {
    const out = await invoke<ClearCacheOutcome>("clear_app_cache", { id });
    cachesScan = null;
    return out;
  }
  await sleep(1200);
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 };
  const cache = sampleAppCaches.find((c) => c.id === id);
  if (!cache) throw new Error(`unknown cache: ${id}`);
  if (cache.running) {
    cache.running = false; // As if the user closed it before trying again.
    return { running: true, report, cache: null };
  }
  report.deletedCount = cache.fileCount;
  report.deletedBytes = report.freedBytes = cache.cacheBytes;
  sampleFreeBytes += cache.cacheBytes;
  for (const app of sampleApps) {
    if (app.cacheId === id && app.source === "store") app.sizeBytes = Math.max(0, app.sizeBytes - cache.cacheBytes);
  }
  cache.cacheBytes = 0;
  cache.fileCount = 0;
  return { running: false, report, cache: { ...cache } };
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
  const steps = ["caches", "downloads", "personal", "duplicates", "photos", "appFolders"] as const;
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
export async function uninstallApp(app: Pick<InstalledApp, "id" | "name" | "sizeBytes">): Promise<UninstallOutcome> {
  // Name and size are only for the history: Rust finds the app by its id.
  if (isTauri()) return invoke<UninstallOutcome>("uninstall_app", { id: app.id, name: app.name, sizeBytes: app.sizeBytes });
  await sleep(1800);
  sampleRemoved.add(app.id);
  sampleRecord("uninstall", [{ text: app.name }], { entryIds: [], movedCount: 0, movedBytes: 0, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 }, app.sizeBytes);
  return "removed";
}

/** Whether the app is still in the app list (asked while its uninstaller runs). */
export async function isAppInstalled(id: string): Promise<boolean> {
  if (isTauri()) return invoke<boolean>("is_app_installed", { id });
  return !sampleRemoved.has(id);
}

// ---------------------------------------------------------------------------
// Apps uninstalled here, to reinstall them. Real: src-tauri/src/reinstall.rs.

let sampleUninstalled: UninstalledApp[] | null = null;
const sampleUninstalledList = () => (sampleUninstalled ??= sampleUninstalledApps(Math.floor(Date.now() / 1000)));

/** The apps uninstalled with Easy Uninstaller, most recent first. */
export async function listUninstalledApps(): Promise<UninstalledApp[]> {
  if (isTauri()) return invoke<UninstalledApp[]>("list_uninstalled_apps");
  return [...sampleUninstalledList()];
}

/** Adds an app to that list once it is gone. Best effort: the uninstall itself already worked. */
export async function rememberUninstalled(app: InstalledApp): Promise<void> {
  const removed = { id: app.id, name: app.name, publisher: app.publisher, source: app.source, sizeBytes: app.sizeBytes, iconColor: app.iconColor, icon: app.icon };
  if (isTauri()) return invoke("remember_uninstalled", { removed });
  const list = sampleUninstalledList().filter((e) => e.id !== app.id && e.name.toLowerCase() !== app.name.toLowerCase());
  const way = app.source === "store" ? "store" : "search";
  sampleUninstalled = [{ ...removed, uninstalledAt: Math.floor(Date.now() / 1000), way, site: "", link: "" }, ...list];
}

/** Takes apps off the list; nothing is installed or removed. */
export async function forgetUninstalled(ids: string[]): Promise<void> {
  if (isTauri()) return invoke("forget_uninstalled", { ids });
  sampleUninstalled = sampleUninstalledList().filter((e) => !ids.includes(e.id));
}

/** Opens the way back to an app: its Store page, Steam, its website or a web search. */
export async function reinstallApp(id: string): Promise<void> {
  if (isTauri()) return invoke("reinstall_app", { id });
  await sleep(300);
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
  sampleRecord("leftovers", [...byApp.keys()].map((app) => (app ? { text: app } : { key: "leftovers.title" })), report);
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
export async function removeFileGroups(groupIds: string[], opts: { quick?: boolean } = {}): Promise<MoveReport> {
  // `quick` only says how the history names it.
  if (isTauri()) return invoke<MoveReport>("remove_file_groups", { groupIds, quick: opts.quick ?? false });
  await new Promise((r) => setTimeout(r, 900));
  const now = Math.floor(Date.now() / 1000);
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 };
  const picked = sampleGroups().filter((x) => groupIds.includes(x.id) && !x.reviewOnly);
  for (const g of picked) {
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
  sampleRecord(opts.quick ? "quickClean" : "files", picked.map((g) => ({ key: g.titleKey })), report);
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
 * scan found, and always leaves one file of every set (duplicates, similar photos). Real: src-tauri/src/review.rs.
 */
export async function quarantineFileSelection(selections: FileSelection[]): Promise<MoveReport> {
  if (isTauri()) return invoke<MoveReport>("quarantine_file_selection", { selections });
  const now = Math.floor(Date.now() / 1000);
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 };
  const groups = sampleGroups();
  const names: HistoryName[] = [];
  for (const sel of selections) {
    const g = groups.find((x) => x.id === sel.groupId);
    if (!g) continue;
    const wanted = new Set(sel.paths);
    let picked: SampleFile[];
    const sets = sampleSets(g.id);
    if (sets) {
      picked = sets.flatMap((set) => {
        const left = set.filter((f) => !sampleGone.has(f.path));
        const chosen = left.filter((f) => wanted.has(f.path) && !sampleHides(f.path));
        return chosen.length && chosen.length >= left.length ? chosen.slice(1) : chosen;
      });
    } else {
      picked = sampleFiles.filter((f) => f.group === g.id && wanted.has(f.path) && samplePresent(f.path));
    }
    if (!picked.length) continue;
    names.push({ key: g.titleKey });
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
  sampleRecord("files", names, report);
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
  const all = sampleSets(groupId);
  if (all) {
    const wasted = (set: SampleFile[]) => set.slice(1).reduce((s, f) => s + f.sizeBytes, 0);
    const sets = all
      .map((set) => set.filter((f) => samplePresent(f.path)))
      .filter((set) => set.length > 1)
      .sort((a, b) => wasted(b) - wasted(a));
    return {
      groupId,
      totalCount: sets.reduce((s, set) => s + set.length - 1, 0),
      totalBytes: sets.reduce((s, set) => s + wasted(set), 0),
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
  const report: MoveReport = { entryIds: [id], movedCount: 1, movedBytes: item.sizeBytes, deletedCount: 0, deletedBytes: 0, freedBytes: 0, skippedCount: 0, declinedCount: 0 };
  sampleRecord("appFolder", [{ text: name }], report);
  return report;
}

/** Puts the files of these entries back where they were. */
export async function restoreQuarantine(ids: string[]): Promise<RestoreReport> {
  if (isTauri()) return invoke<RestoreReport>("restore_quarantine", { ids });
  const report: RestoreReport = { restoredCount: 0, renamedCount: 0, failedCount: 0 };
  const now = Math.floor(Date.now() / 1000);
  for (const id of ids) {
    const i = sampleQuarantine.findIndex((e) => e.id === id);
    if (i < 0) continue;
    for (const r of sampleHistory.filter((r) => r.entries.some((e) => e.id === id))) {
      r.restored.push(id);
      r.restoredAt = now;
    }
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

// ── Reminders (src-tauri/src/reminders.rs) ─────────────────────────────────

/** Free space limits offered for the "almost full" notice. Matches `THRESHOLD_CHOICES` in reminders.rs. */
export const REMINDER_THRESHOLDS = [5, 10, 15, 20] as const;

const sampleReminders: ReminderSettings = { lowSpace: false, threshold: 10, monthly: false };

export async function getReminders(): Promise<ReminderSettings> {
  if (isTauri()) return invoke<ReminderSettings>("get_reminders");
  return { ...sampleReminders };
}

/**
 * Saves the reminder choices: Windows gets (or loses) a small scheduled task that checks
 * once a day. On failure nothing changes and the error is thrown.
 */
export async function setReminders(settings: ReminderSettings, texts: ReminderTexts): Promise<void> {
  if (isTauri()) return invoke("set_reminders", { settings, texts });
  await sleep(300);
  Object.assign(sampleReminders, settings);
}

/** Each time the app opens and when the language changes: the monthly reminder counts from here. */
export async function remindersOpened(texts: ReminderTexts): Promise<void> {
  if (isTauri()) return invoke("reminders_opened", { texts });
}

/** Shows a sample Windows notification right away. */
export async function previewReminder(title: string, body: string): Promise<void> {
  if (isTauri()) return invoke("preview_reminder", { title, body });
  await sleep(300);
}

// ---------------------------------------------------------------------------
// History. Real: src-tauri/src/history.rs.

/**
 * Everything Easy Uninstaller did, newest first, with the space it gave back. Files in
 * Quarantine on the same drive count as freed only once they are deleted for good.
 */
export async function getHistory(): Promise<HistorySummary> {
  if (isTauri()) return invoke<HistorySummary>("get_history");
  const items = sampleHistory.map(sampleHistoryItem).reverse();
  return {
    freedBytes: items.reduce((s, i) => s + i.freedBytes, 0),
    pendingBytes: items.reduce((s, i) => s + i.pendingBytes, 0),
    since: sampleHistory[0]?.at ?? null,
    actionCount: items.length,
    items,
  };
}

/** Forgets every action: the total starts again from zero. No file is touched. */
export async function clearHistory(): Promise<void> {
  if (isTauri()) return invoke("clear_history");
  sampleHistory.length = 0;
}

/** Browser fallback: the same rules as `Record::item` in history.rs. */
function sampleHistoryItem(r: SampleRecord): HistoryItem {
  let inQuarantine = 0;
  let restored = 0;
  let ended = 0;
  let until: number | null = null;
  for (const e of r.entries) {
    const live = sampleQuarantine.find((q) => q.id === e.id);
    if (live) {
      inQuarantine += Math.min(e.bytes, live.sizeBytes);
      until = Math.max(until ?? 0, live.expiresAt);
    } else if (r.restored.includes(e.id)) restored += e.bytes;
    else ended += e.bytes;
  }
  const crossed = Math.min(r.crossedBytes, inQuarantine);
  const pending = inQuarantine - crossed;
  return {
    id: r.id,
    at: r.at,
    kind: r.kind,
    names: r.names,
    itemCount: r.itemCount,
    freedBytes: r.deletedBytes + r.estimatedBytes + ended + crossed,
    pendingBytes: pending,
    pendingUntil: pending > 0 ? until : null,
    restoredBytes: restored,
    restoredAt: r.restoredAt,
    estimated: r.estimatedBytes > 0,
  };
}

/** Browser fallback: records an action like history.rs does. */
function sampleRecord(kind: HistoryKind, names: HistoryName[], report: MoveReport, estimatedBytes = 0) {
  const itemCount = report.movedCount + report.deletedCount + (estimatedBytes ? 1 : 0);
  if (!itemCount) return;
  const at = Math.floor(Date.now() / 1000);
  sampleHistory.push({
    id: `${at}-${sampleHistory.length}`,
    at,
    kind,
    names,
    itemCount,
    deletedBytes: report.deletedBytes,
    crossedBytes: Math.max(report.freedBytes - report.deletedBytes, 0),
    entries: report.entryIds.map((id) => ({ id, bytes: sampleQuarantine.find((q) => q.id === id)?.sizeBytes ?? 0 })),
    estimatedBytes,
    restored: [],
    restoredAt: null,
  });
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

// ---------------------------------------------------------------------------
// Free up space without deleting. Real: src-tauri/src/offload.rs.

/** Browser fallback: the pretend OneDrive and drives, changed by the functions below. */
const sampleOffload = {
  online: [...sampleOnlineFiles],
  onlineCount: sampleOnlineCount,
  onlineBytes: sampleOnlineBytes,
  freeingCount: 0,
  freeingBytes: 0,
  lastFreed: null as { count: number; bytes: number; files: OffloadFile[] } | null,
  movable: [...sampleMoveFiles],
  moved: [] as MovedFile[],
};

function sampleOffloadScan(): OffloadScan {
  const s = sampleOffload;
  return {
    oneDrive: {
      folders: ["C:\\Users\\you\\OneDrive"],
      fileCount: s.onlineCount,
      sizeBytes: s.onlineBytes,
      files: s.online,
      freeingCount: s.freeingCount,
      freeingBytes: s.freeingBytes,
    },
    moveFiles: {
      systemDrive: "C:",
      drives: sampleDrives.map((d) => ({ ...d, freeBytes: d.freeBytes - s.moved.reduce((n, m) => n + m.sizeBytes, 0) })),
      fileCount: s.movable.length,
      sizeBytes: s.movable.reduce((n, f) => n + f.sizeBytes, 0),
      files: s.movable,
      moved: [...s.moved].sort((a, b) => b.movedAt - a.movedAt),
    },
  };
}

/** Looks for OneDrive files to keep only online and big files to move. Read only. */
export async function scanOffload(): Promise<OffloadScan> {
  if (isTauri()) return invoke<OffloadScan>("scan_offload");
  await sleep(700);
  return sampleOffloadScan();
}

/** The cards as they are now, without scanning again. */
export async function currentOffload(): Promise<OffloadScan> {
  if (isTauri()) return invoke<OffloadScan>("current_offload");
  return sampleOffloadScan();
}

/** OneDrive's "Free up space" on every scanned file except `except` (paths). */
export async function makeOnlineOnly(except: string[]): Promise<OnlineReport> {
  if (isTauri()) return invoke<OnlineReport>("make_online_only", { except });
  await sleep(900);
  const s = sampleOffload;
  const kept = s.online.filter((f) => except.includes(f.path));
  const keptBytes = kept.reduce((n, f) => n + f.sizeBytes, 0);
  const report = { doneCount: s.onlineCount - kept.length, doneBytes: s.onlineBytes - keptBytes, skippedCount: 0 };
  s.lastFreed = { count: report.doneCount, bytes: report.doneBytes, files: s.online.filter((f) => !except.includes(f.path)) };
  s.freeingCount += report.doneCount;
  s.freeingBytes += report.doneBytes;
  s.online = kept;
  s.onlineCount = kept.length;
  s.onlineBytes = keptBytes;
  return report;
}

/** Undoes the last `makeOnlineOnly`: OneDrive keeps the files on this PC again. */
export async function keepOnThisPc(): Promise<OnlineReport> {
  if (isTauri()) return invoke<OnlineReport>("keep_on_this_pc");
  await sleep(600);
  const s = sampleOffload;
  const last = s.lastFreed;
  if (!last) return { doneCount: 0, doneBytes: 0, skippedCount: 0 };
  s.freeingCount -= last.count;
  s.freeingBytes -= last.bytes;
  s.online = [...s.online, ...last.files].sort((a, b) => b.sizeBytes - a.sizeBytes);
  s.onlineCount += last.count;
  s.onlineBytes += last.bytes;
  s.lastFreed = null;
  return { doneCount: last.count, doneBytes: last.bytes, skippedCount: 0 };
}

/** Moves every scanned file except `except` to `drive` ("D:"), into a folder named `folderName`. */
export async function moveToDrive(except: string[], drive: string, folderName: string, notify: (p: MoveProgress) => void): Promise<DriveMoveReport> {
  if (isTauri()) {
    const onProgress = new Channel<MoveProgress>();
    onProgress.onmessage = notify;
    return invoke<DriveMoveReport>("move_to_drive", { except, drive, folderName, onProgress });
  }
  const s = sampleOffload;
  const files = s.movable.filter((f) => !except.includes(f.path));
  const total = files.reduce((n, f) => n + f.sizeBytes, 0);
  const folder = `${drive}\\${folderName}`;
  const now = Math.floor(Date.now() / 1000);
  let done = 0;
  const ids: string[] = [];
  for (const [i, f] of files.entries()) {
    await sleep(500);
    done += f.sizeBytes;
    const id = `${now}-${i}`;
    ids.push(id);
    s.moved.push({ id, originalPath: f.path, path: f.path.replace(/^C:\\Users\\you/, folder), sizeBytes: f.sizeBytes, movedAt: now, base: folder });
    notify({ doneBytes: done, totalBytes: total, doneCount: i + 1, totalCount: files.length });
  }
  s.movable = s.movable.filter((f) => except.includes(f.path));
  return { ids, movedCount: files.length, movedBytes: total, skippedCount: 0, noRoomCount: 0, folder };
}

/** Moves files back from the other drive to where they were. */
export async function moveBackFiles(ids: string[]): Promise<MoveBackReport> {
  if (isTauri()) return invoke<MoveBackReport>("move_back_files", { ids });
  await sleep(600);
  const s = sampleOffload;
  const back = s.moved.filter((m) => ids.includes(m.id));
  s.moved = s.moved.filter((m) => !ids.includes(m.id));
  s.movable = [...s.movable, ...back.map((m) => ({ path: m.originalPath, sizeBytes: m.sizeBytes, lastUsed: "2024-01-01" }))].sort((a, b) => b.sizeBytes - a.sizeBytes);
  return { restoredCount: back.length, restoredBytes: back.reduce((n, m) => n + m.sizeBytes, 0), renamedCount: 0, failedCount: 0 };
}

/** Opens the folder of a moved file in Explorer, with the file selected. */
export async function showMovedFile(id: string): Promise<void> {
  if (isTauri()) return invoke("show_moved_file", { id });
}
