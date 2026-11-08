// Single entry point for everything the UI asks the system.
// Inside the Tauri app calls go to Rust commands; in a plain browser
// (npm run dev) they fall back to sample data so the UI can be designed fast.
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { GB } from "./format";
import {
  buildSampleGroups,
  sampleApps,
  sampleDuplicateSets,
  sampleFiles,
  sampleLeftovers,
  samplePreview,
  sampleQuarantine,
  sampleSpaceUsage,
  type SampleFile,
} from "./mock";
import type {
  DiskInfo,
  Exclusion,
  FileGroup,
  FileSelection,
  GroupReview,
  InstalledApp,
  LeftoverItem,
  MoveReport,
  QuarantineEntry,
  RestorePointOutcome,
  RestoreReport,
  ReviewFile,
  ScanProgress,
  SpaceProgress,
  SpaceUsage,
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
  if (!isTauri()) return Promise.resolve(sampleApps);
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
  const steps = ["caches", "downloads", "personal", "duplicates"] as const;
  for (const [i, step] of steps.entries()) {
    for (let n = 0; n < 4; n++) {
      notify({ step, stepIndex: i + 1, stepCount: steps.length, filesChecked: n * 1250 });
      await new Promise((r) => setTimeout(r, 120));
    }
  }
  return buildSampleGroups(samplePresent);
}

/** Sample until the leftovers scanner lands. */
export async function listLeftovers(): Promise<LeftoverItem[]> {
  return sampleLeftovers;
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
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, skippedCount: 0 };
  for (const g of buildSampleGroups(samplePresent).filter((x) => groupIds.includes(x.id))) {
    sampleMove(report, g, g.items.map((i) => ({ path: i.path, sizeBytes: i.sizeBytes })), g.itemCount, now);
  }
  return report;
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
    expiresAt: now + QUARANTINE_DAYS * DAY,
    items: files.map((f) => ({ originalPath: f.path, sizeBytes: f.sizeBytes })),
  });
  files.forEach((f) => sampleGone.add(f.path));
  report.entryIds.push(id);
  report.movedCount += count;
  report.movedBytes += size;
}

/**
 * Moves the files the user picked, one quarantine entry per group. Rust only moves
 * files its own scan found, and always leaves one copy of every duplicate set.
 */
export async function quarantineFileSelection(selections: FileSelection[]): Promise<MoveReport> {
  if (isTauri()) return invoke<MoveReport>("quarantine_file_selection", { selections });
  const now = Math.floor(Date.now() / 1000);
  const report: MoveReport = { entryIds: [], movedCount: 0, movedBytes: 0, skippedCount: 0 };
  const groups = buildSampleGroups(samplePresent);
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
    if (picked.length) sampleMove(report, g, picked, picked.length, now);
  }
  return report;
}

/** The groups of the last scan as they are now, after a move, an undo or a hidden file. */
export async function refreshFileGroups(): Promise<FileGroup[]> {
  const groups = isTauri() ? await invoke<FileGroup[]>("current_file_groups") : buildSampleGroups(samplePresent);
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
const sampleHides = (path: string) => sampleHidden.some((e) => path === e.path || (e.folder && path.startsWith(`${e.path}\\`)));
const samplePresent = (path: string) => !sampleGone.has(path) && !sampleHides(path);

/** Files and folders the user asked never to see again. */
export async function listExclusions(): Promise<Exclusion[]> {
  if (isTauri()) return invoke<Exclusion[]>("list_exclusions");
  return [...sampleHidden];
}

/** Never show this file (or anything in this folder) again, in any scan. Nothing is moved. */
export async function addExclusion(exclusion: Exclusion): Promise<void> {
  if (isTauri()) return invoke("add_exclusion", { exclusion });
  const i = sampleHidden.findIndex((e) => e.path === exclusion.path);
  if (i >= 0) sampleHidden.splice(i, 1);
  sampleHidden.push(exclusion);
}

/** Shows a hidden file or folder again from the next scan on. */
export async function removeExclusion(path: string): Promise<void> {
  if (isTauri()) return invoke("remove_exclusion", { path });
  const i = sampleHidden.findIndex((e) => e.path === path);
  if (i >= 0) sampleHidden.splice(i, 1);
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
