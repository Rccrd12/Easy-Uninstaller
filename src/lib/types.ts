// Shapes shared between the UI and the Rust backend. Field names match the
// serde output of the Rust structs (camelCase).

export interface DiskInfo {
  mountPoint: string;
  totalBytes: number;
  freeBytes: number;
}

export type AppSource = "win32" | "msi" | "store";

export interface InstalledApp {
  id: string;
  name: string;
  publisher: string;
  sizeBytes: number;
  source: AppSource;
  /** ISO date, or null when Windows does not record it. */
  installedOn: string | null;
  /** Human readable, approximate. null when unknown: Windows has no reliable "last used" value. */
  lastUsed: string | null;
  /** True only when we know the app was never opened. */
  neverUsed: boolean;
  preinstalled: boolean;
  bloatware: boolean;
  /** Background of the letter tile shown when there is no icon. */
  iconColor: string;
  /** PNG data URL, or null to show the letter tile. */
  icon: string | null;
}

/** How safe it is to remove something. */
export type Safety = "safe" | "check" | "personal";

export interface FileItem {
  path: string;
  sizeBytes: number;
  /** Last time the file was opened or changed, "YYYY-MM-DD"; null for folders. */
  lastUsed: string | null;
  /** For duplicates: the copy that is kept. */
  duplicateOf: string | null;
  /** A folder the app can explain: an id from src-tauri/src/known_folders.json. */
  explainId: string | null;
  /** For Recycle Bin items: where the file was before it was deleted. */
  originalPath: string | null;
}

export interface FileGroup {
  id: string;
  /** i18n keys, translated by the UI. */
  titleKey: string;
  descriptionKey: string;
  safety: Safety;
  sizeBytes: number;
  /** Number of files in the group. */
  itemCount: number;
  /** Largest first, at most 200. Temp files are listed one folder per item. */
  items: FileItem[];
  /** Only shown and explained: it cannot be selected or moved as a whole. */
  reviewOnly: boolean;
}

export type ScanStep = "caches" | "downloads" | "personal" | "duplicates" | "appFolders";

export interface ScanProgress {
  step: ScanStep;
  /** 1-based. */
  stepIndex: number;
  stepCount: number;
  filesChecked: number;
}

export type LeftoverKind = "folder" | "registry" | "shortcut";

/** Something a program left behind. Mirrors `LeftoverItem` in src-tauri/src/leftovers.rs. */
export interface LeftoverItem {
  id: string;
  /** The app it belonged to; for a broken shortcut, the shortcut's name. */
  appName: string | null;
  kind: LeftoverKind;
  /** A path, or a registry key such as HKCU\Software\App. */
  path: string;
  /** 0 for registry keys. */
  sizeBytes: number;
  /** i18n key under `leftovers.reasons`. */
  reason: string;
  /** "check" for the user's own settings of the app: not selected by default. */
  safety: Safety;
  /** Outside the user's folder (Program Files, ProgramData, HKLM): Windows asks for permission. */
  needsAdmin: boolean;
  /** Belongs to no installed app, but no uninstall is known (removed by hand or elsewhere): for the user to check. */
  unowned: boolean;
  /** Last change inside the folder, in seconds since 1970, when known. */
  modifiedAt: number | null;
}

/** How an uninstall ended. Mirrors `UninstallOutcome` in src-tauri/src/uninstall.rs. */
export type UninstallOutcome =
  /** Gone from the app list. */
  | "removed"
  /** The uninstaller closed but the app is still listed: it may still be working, or was stopped. */
  | "stillInstalled"
  /** Cancelled in the app's own uninstaller. */
  | "cancelled"
  /** The Windows permission prompt was answered no. */
  | "declined";

export type MapCategory = "apps" | "games" | "personal" | "system" | "reclaimable" | "free";

/** A block of the space map, built by src/lib/spacemap.ts. */
export interface SpaceBlock {
  id: string;
  /** i18n key, or null to show `label` as is (an app or folder name). */
  labelKey: string | null;
  label: string | null;
  sizeBytes: number;
  category: MapCategory;
  path: string | null;
  /** How many items a grouped "Other ..." block holds; 0 otherwise. */
  groupedCount: number;
}

/** A measured folder. Mirrors `SpaceFolder` in src-tauri/src/space.rs. */
export interface SpaceFolder {
  id: string;
  labelKey: string | null;
  label: string | null;
  path: string | null;
  sizeBytes: number;
  category: "apps" | "personal" | "reclaimable";
}

/** Mirrors `SpaceUsage` in src-tauri/src/space.rs. */
export interface SpaceUsage {
  mountPoint: string;
  totalBytes: number;
  freeBytes: number;
  /** Biggest first. */
  folders: SpaceFolder[];
}

export interface SpaceProgress {
  filesChecked: number;
}

export interface QuarantinedFile {
  /** A path, or a registry key such as HKCU\Software\App. */
  originalPath: string;
  sizeBytes: number;
  kind: "file" | "folder" | "registry";
}

/** One move into quarantine (for example, one file group). */
export interface QuarantineEntry {
  id: string;
  /** i18n key of the title, or a plain title (an app name) when there is no key. */
  titleKey: string | null;
  title: string | null;
  itemCount: number;
  sizeBytes: number;
  /** Unix seconds. */
  removedAt: number;
  expiresAt: number;
  /** Largest first, at most 50. */
  items: QuarantinedFile[];
  /** Made with administrator rights: Windows asks for permission to restore or delete it. */
  needsAdmin: boolean;
}

/** What removing file groups did. Mirrors `MoveReport` in src-tauri/src/quarantine.rs. */
export interface MoveReport {
  /** One entry per group moved; restoring them undoes the move. */
  entryIds: string[];
  /** Moved into quarantine. */
  movedCount: number;
  movedBytes: number;
  /** Deleted right away (temporary files and caches only). */
  deletedCount: number;
  deletedBytes: number;
  /** Space really given back: deleted, plus moved to a quarantine on another drive. */
  freedBytes: number;
  /** Left in place: in use, changed since the scan, or gone. */
  skippedCount: number;
  /** Left in place because the Windows permission prompt was answered no. */
  declinedCount: number;
}

/** Mirrors `QuarantineSettings` in src-tauri/src/quarantine.rs. */
export interface QuarantineSettings {
  /** 3, 7 or 30. */
  keepDays: number;
  /** "D:", or null for the app's own folder on the system drive. */
  drive: string | null;
}

/** Another fixed drive. Mirrors `DriveInfo` in src-tauri/src/disk.rs. */
export interface DriveInfo {
  mountPoint: string;
  label: string;
  totalBytes: number;
  freeBytes: number;
}

/** Mirrors `QuarantineOptions` in src-tauri/src/quarantine.rs. */
export interface QuarantineOptions {
  settings: QuarantineSettings;
  systemDrive: string;
  drives: DriveInfo[];
}

export interface RestoreReport {
  restoredCount: number;
  /** Came back as "name (restored)" because the original name is taken. */
  renamedCount: number;
  /** Still in quarantine. */
  failedCount: number;
}

export type RestorePointOutcome = "created" | "declined" | "disabled" | "failed";

/** A file as the review lists it. Mirrors `ReviewFile` in src-tauri/src/review.rs. */
export interface ReviewFile {
  path: string;
  sizeBytes: number;
  /** "YYYY-MM-DD": the later of last opened and last changed. */
  lastUsed: string;
  /** "YYYY-MM-DD". */
  modified: string;
  /** A photo, video or document that "Open" may show; never a program. */
  canOpen: boolean;
}

/** Every file of one group, for reviewing them one by one. Mirrors `GroupReview`. */
export interface GroupReview {
  groupId: string;
  /** Files that can be moved: for duplicates, the extra copies. */
  totalCount: number;
  totalBytes: number;
  /** Largest first, at most 2,000. Empty for duplicates. */
  files: ReviewFile[];
  /** Duplicates only: sets of identical files, each oldest first, most space first. */
  duplicateSets: ReviewFile[][];
}

/** Some files of one group, picked by the user. */
export interface FileSelection {
  groupId: string;
  paths: string[];
}

/** What a hidden item is. A folder covers everything inside it. */
export type HiddenKind = "file" | "folder" | "group" | "leftover";

/** Something the user chose to hide. Mirrors `Exclusion` in src-tauri/src/review.rs. */
export interface Exclusion {
  /** A path for files and folders, an id for groups and leftovers. */
  path: string;
  kind: HiddenKind;
  /** Unix seconds; 0 until saved. */
  hiddenAt: number;
  /** An i18n key (groups) or an app name (leftovers), when the path says little. */
  label: string | null;
}

// ── AI help (src-tauri/src/ai.rs) ──────────────────────────────────────────

/** Where the AI runs: off, an online service, or a model server on this PC. */
export type AiMode = "off" | "online" | "local";

/** Which service and model the user picked. Mirrors `AiConfig`. */
export interface AiConfig {
  service: string;
  model: string;
  /** For "other service" and servers on this PC: their address. */
  url: string;
  /** Start Unsloth Studio when it is not running, and stop it after a while. */
  autoStart: boolean;
}

export interface AiModel {
  id: string;
  label: string;
}

export interface AiServiceStatus {
  id: string;
  /** The program is installed, or the server on this PC answers. */
  found: boolean;
  /** How many models a server on this PC has, when it says. */
  models: number | null;
  /** A key is saved in Windows' Credential Manager. */
  hasKey: boolean;
  /** The app can start it when it is not running (Unsloth Studio, installed). */
  canStart: boolean;
}

/** Unsloth Studio being started by the app. Mirrors `ai::unsloth::StartStatus`. */
export interface AiStartStatus {
  state: "starting" | "on" | "off";
  /** How long the start has taken so far. */
  seconds: number;
}

/** What is sent about one folder: names and numbers only. Mirrors `FolderSummary`. */
export interface FolderSummary {
  /** `%LOCALAPPDATA%\Name`: never the user's own folder path. */
  path: string;
  sizeBytes: number;
  fileCount: number;
  /** Newest change inside, seconds since 1970. */
  modified: number | null;
  subfolders: string[];
  names: string[];
}

export interface AiPreview {
  summary: FolderSummary;
  /** The exact text sent. */
  question: string;
}

export type AiVerdict = "removable" | "keep" | "unknown";
export type AiConfidence = "low" | "medium" | "high";

/** What the app checked on this PC, shown apart from the AI's answer. */
export interface AiFacts {
  installedApp: string | null;
  modified: number | null;
}

export interface AiAnswer {
  path: string;
  what: string;
  madeBy: string;
  verdict: AiVerdict;
  confidence: AiConfidence;
  reason: string;
  /** Two to four words, for the tag. */
  label: string;
  /** The AI said "removable" but an installed app matches, so it shows as "keep". */
  overridden: boolean;
  facts: AiFacts;
  service: string;
  model: string;
  askedAt: number;
}

export interface AiStats {
  answers: number;
  askedThisMonth: number;
}

/** The version to install. Mirrors `UpdateInfo` in src-tauri/src/updater.rs. */
export interface UpdateInfo {
  version: string;
  currentVersion: string;
  /** A beta (Settings > Beta versions): changes not released yet. */
  beta: boolean;
  /** Older than the beta running: going back to the release. */
  back: boolean;
  /** What a beta holds, one change per line. */
  notes: string[];
}

/** Sent while an update downloads. Mirrors `UpdateProgress` in src-tauri/src/updater.rs. */
export interface UpdateProgress {
  downloadedBytes: number;
  /** null when the size is not known. */
  totalBytes: number | null;
}
