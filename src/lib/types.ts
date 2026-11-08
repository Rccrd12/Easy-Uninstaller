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
}

export type ScanStep = "caches" | "downloads" | "personal" | "duplicates";

export interface ScanProgress {
  step: ScanStep;
  /** 1-based. */
  stepIndex: number;
  stepCount: number;
  filesChecked: number;
}

export interface LeftoverItem {
  id: string;
  appName: string;
  path: string;
  sizeBytes: number;
  kind: "folder" | "registry" | "shortcut";
}

export type MapCategory = "apps" | "games" | "personal" | "system" | "reclaimable";

export interface SpaceBlock {
  id: string;
  label: string;
  sizeBytes: number;
  category: MapCategory;
}

export interface QuarantineItem {
  id: string;
  label: string;
  originalPath: string;
  sizeBytes: number;
  removedOn: string;
  expiresInDays: number;
}
