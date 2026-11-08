// Sample data used until each scanner is implemented in Rust.
// Every function in api.ts that still returns from here is marked "sample".
import { GB, MB } from "./format";
import type { FileGroup, FileItem, InstalledApp, LeftoverItem, QuarantineEntry, SpaceFolder, SpaceUsage } from "./types";

export const sampleApps: InstalledApp[] = [
  { id: "photoshop", name: "Adobe Photoshop 2025", publisher: "Adobe", sizeBytes: 6.8 * GB, source: "win32", installedOn: "2025-01-14", lastUsed: "3 months ago", preinstalled: false, bloatware: false, neverUsed: false, iconColor: "#e3008c", icon: null },
  { id: "xbox-bar", name: "Xbox Game Bar", publisher: "Microsoft", sizeBytes: 1.2 * GB, source: "store", installedOn: null, lastUsed: null, preinstalled: true, bloatware: false, neverUsed: true, iconColor: "#107c10", icon: null },
  { id: "spotify", name: "Spotify", publisher: "Spotify AB", sizeBytes: 740 * MB, source: "win32", installedOn: "2024-06-02", lastUsed: "today", preinstalled: false, bloatware: false, neverUsed: false, iconColor: "#1db954", icon: null },
  { id: "discord", name: "Discord", publisher: "Discord Inc.", sizeBytes: 520 * MB, source: "win32", installedOn: "2024-09-21", lastUsed: "2 weeks ago", preinstalled: false, bloatware: false, neverUsed: false, iconColor: "#5865f2", icon: null },
  { id: "mcafee", name: "McAfee LiveSafe", publisher: "McAfee", sizeBytes: 410 * MB, source: "win32", installedOn: "2024-03-01", lastUsed: null, preinstalled: true, bloatware: true, neverUsed: true, iconColor: "#ff6a00", icon: null },
  { id: "vlc", name: "VLC media player", publisher: "VideoLAN", sizeBytes: 160 * MB, source: "win32", installedOn: "2023-11-10", lastUsed: "1 month ago", preinstalled: false, bloatware: false, neverUsed: false, iconColor: "#f18a00", icon: null },
];

const U = "C:\\Users\\you";
const fileGroup = (id: string, key: string, safety: FileGroup["safety"], itemCount: number, items: FileItem[]): FileGroup => ({
  id,
  titleKey: `files.groups.${key}.title`,
  descriptionKey: `files.groups.${key}.description`,
  safety,
  sizeBytes: items.reduce((s, i) => s + i.sizeBytes, 0),
  itemCount,
  items,
});
const item = (path: string, sizeBytes: number, lastUsed: string | null = null, duplicateOf: string | null = null): FileItem => ({ path, sizeBytes, lastUsed, duplicateOf });

export const sampleFileGroups: FileGroup[] = [
  fileGroup("temp", "temp", "safe", 4821, [
    item(`${U}\\AppData\\Local\\Temp`, 1.6 * GB),
    item(`${U}\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache`, 820 * MB),
    item("C:\\Windows\\Temp", 410 * MB),
    item(`${U}\\AppData\\Local\\NVIDIA\\DXCache`, 270 * MB),
  ]),
  fileGroup("installers", "installers", "safe", 3, [
    item(`${U}\\Downloads\\Adobe_Photoshop_2025_Set-Up.exe`, 1.9 * GB, "2025-01-14"),
    item(`${U}\\Downloads\\SpotifyFullSetup.exe`, 610 * MB, "2024-06-02"),
    item(`${U}\\Downloads\\vlc-3.0.20-win64.exe`, 42 * MB, "2023-11-10"),
  ]),
  fileGroup("old-downloads", "oldDownloads", "check", 212, [
    item(`${U}\\Downloads\\Wedding video raw.mp4`, 2.4 * GB, "2023-05-20"),
    item(`${U}\\Downloads\\ubuntu-22.04-desktop-amd64.iso`, 1.2 * GB, "2023-02-11"),
    item(`${U}\\Downloads\\Tax return 2022.pdf`, 3 * MB, "2023-04-30"),
  ]),
  fileGroup("duplicates", "duplicates", "check", 38, [
    item(`${U}\\Desktop\\Holidays\\VID_20240802.mp4`, 920 * MB, "2024-09-02", `${U}\\Pictures\\Holidays 2024\\VID_20240802.mp4`),
    item(`${U}\\Desktop\\presentation-final.pptx`, 210 * MB, "2025-03-12", `${U}\\Documents\\presentation-final.pptx`),
  ]),
  fileGroup("large-files", "largeFiles", "check", 2, [
    item(`${U}\\Videos\\Game capture 2025-02-03.mp4`, 5.6 * GB, "2025-02-03"),
    item(`${U}\\Documents\\VirtualBox VMs\\Windows 10.vdi`, 3.9 * GB, "2024-12-01"),
  ]),
];

const leftover = (id: string, appName: string | null, kind: LeftoverItem["kind"], path: string, sizeBytes: number, reason: string, safety: LeftoverItem["safety"] = "safe", needsAdmin = false): LeftoverItem => ({ id, appName, kind, path, sizeBytes, reason, safety, needsAdmin });

/** What the browser fallback "finds" after uninstalling `app`. */
export function sampleAppLeftovers(app: string): LeftoverItem[] {
  const slug = app.replace(/[^A-Za-z0-9]+/g, "");
  return [
    leftover(`${slug}-local`, app, "folder", `${U}\\AppData\\Local\\${slug}`, 310 * MB, "localData"),
    leftover(`${slug}-pf`, app, "folder", `C:\\Program Files\\${app}`, 18 * MB, "programFolder", "safe", true),
    leftover(`${slug}-reg`, app, "registry", `HKCU\\Software\\${slug}`, 0, "registry"),
    leftover(`${slug}-lnk`, app, "shortcut", `${U}\\Desktop\\${app}.lnk`, 2048, "shortcut"),
    leftover(`${slug}-roaming`, app, "folder", `${U}\\AppData\\Roaming\\${slug}`, 96 * MB, "userSettings", "check"),
  ];
}

export const sampleLeftovers: LeftoverItem[] = [
  leftover("l1", "Zoom", "folder", `${U}\\AppData\\Local\\Zoom`, 640 * MB, "localData"),
  leftover("l2", "Zoom", "folder", `${U}\\AppData\\Roaming\\Zoom`, 120 * MB, "userSettings", "check"),
  leftover("l3", "Zoom", "registry", "HKCU\\Software\\Zoom", 0, "registry"),
  leftover("l4", "Skype", "folder", "C:\\ProgramData\\Skype", 210 * MB, "sharedData", "safe", true),
  leftover("l5", "Skype", "registry", "HKLM\\Software\\WOW6432Node\\Skype", 0, "registry", "safe", true),
  leftover("l6", "Old Game Launcher", "shortcut", `${U}\\Desktop\\Old Game Launcher.lnk`, 1400, "brokenShortcut"),
  leftover("l7", "Photo Studio 2019", "registry", "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\PhotoStudio", 0, "brokenEntry", "safe", true),
];

const folder = (id: string, key: string | null, label: string | null, size: number, category: SpaceFolder["category"], path: string | null): SpaceFolder => ({
  id,
  labelKey: key && `map.blocks.${key}`,
  label,
  path,
  sizeBytes: size,
  category,
});

export const sampleSpaceUsage: SpaceUsage = {
  mountPoint: "C:",
  totalBytes: 476 * GB,
  freeBytes: 64 * GB,
  folders: [
    folder("videos", "videos", null, 96 * GB, "personal", `${U}\\Videos`),
    folder("pictures", "pictures", null, 78 * GB, "personal", `${U}\\Pictures`),
    folder("documents", "documents", null, 34 * GB, "personal", `${U}\\Documents`),
    folder("appdata", "appData", null, 24 * GB, "apps", `${U}\\AppData`),
    folder("downloads", "downloads", null, 22 * GB, "personal", `${U}\\Downloads`),
    folder("folder:projects", null, "Projects", 19 * GB, "personal", `${U}\\Projects`),
    folder("folder:onedrive", "oneDrive", null, 12 * GB, "personal", `${U}\\OneDrive`),
    folder("music", "music", null, 8 * GB, "personal", `${U}\\Music`),
    folder("desktop", "desktop", null, 4.5 * GB, "personal", `${U}\\Desktop`),
    folder("caches", "caches", null, 3.1 * GB, "reclaimable", null),
    folder("recycle-bin", "recycleBin", null, 1.4 * GB, "reclaimable", null),
    folder("folder:.vscode", null, ".vscode", 620 * MB, "personal", `${U}\\.vscode`),
    folder("folder:saved games", null, "Saved Games", 180 * MB, "personal", `${U}\\Saved Games`),
  ],
};

const DAY = 24 * 60 * 60;
const daysAgo = (days: number) => Math.floor(Date.now() / 1000) - days * DAY;

/** Changed by the browser fallbacks in api.ts, so moving and restoring can be tried. */
export const sampleQuarantine: QuarantineEntry[] = [
  {
    id: "q1",
    titleKey: "files.groups.temp.title",
    title: null,
    itemCount: 1312,
    sizeBytes: 2.9 * GB,
    removedAt: daysAgo(2),
    expiresAt: daysAgo(2) + 7 * DAY,
    items: [
      { originalPath: `${U}\\AppData\\Local\\NVIDIA\\DXCache\\nv_cache_old.bin`, sizeBytes: 2.1 * GB, kind: "file" },
      { originalPath: `${U}\\AppData\\Local\\Temp\\setup-log-2025.txt`, sizeBytes: 0.8 * GB, kind: "file" },
    ],
    needsAdmin: false,
  },
  {
    id: "q2",
    titleKey: null,
    title: "Discord",
    itemCount: 3,
    sizeBytes: 406 * MB,
    removedAt: daysAgo(1),
    expiresAt: daysAgo(1) + 7 * DAY,
    items: [
      { originalPath: `${U}\\AppData\\Local\\Discord`, sizeBytes: 310 * MB, kind: "folder" },
      { originalPath: `${U}\\AppData\\Roaming\\discord`, sizeBytes: 96 * MB, kind: "folder" },
      { originalPath: "HKCU\\Software\\Discord", sizeBytes: 0, kind: "registry" },
    ],
    needsAdmin: false,
  },
];
