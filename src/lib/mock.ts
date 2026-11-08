// Sample data used until each scanner is implemented in Rust.
// Every function in api.ts that still returns from here is marked "sample".
import { GB, MB } from "./format";
import type { FileGroup, FileItem, InstalledApp, LeftoverItem, QuarantineItem, SpaceBlock } from "./types";

export const sampleApps: InstalledApp[] = [
  { id: "photoshop", name: "Adobe Photoshop 2025", publisher: "Adobe", sizeBytes: 6.8 * GB, source: "win32", installedOn: "2025-01-14", lastUsed: "3 months ago", preinstalled: false, bloatware: false, iconColor: "#e3008c" },
  { id: "xbox-bar", name: "Xbox Game Bar", publisher: "Microsoft", sizeBytes: 1.2 * GB, source: "store", installedOn: null, lastUsed: null, preinstalled: true, bloatware: false, iconColor: "#107c10" },
  { id: "spotify", name: "Spotify", publisher: "Spotify AB", sizeBytes: 740 * MB, source: "win32", installedOn: "2024-06-02", lastUsed: "today", preinstalled: false, bloatware: false, iconColor: "#1db954" },
  { id: "discord", name: "Discord", publisher: "Discord Inc.", sizeBytes: 520 * MB, source: "win32", installedOn: "2024-09-21", lastUsed: "2 weeks ago", preinstalled: false, bloatware: false, iconColor: "#5865f2" },
  { id: "mcafee", name: "McAfee LiveSafe", publisher: "McAfee", sizeBytes: 410 * MB, source: "win32", installedOn: "2024-03-01", lastUsed: null, preinstalled: true, bloatware: true, iconColor: "#ff6a00" },
  { id: "vlc", name: "VLC media player", publisher: "VideoLAN", sizeBytes: 160 * MB, source: "win32", installedOn: "2023-11-10", lastUsed: "1 month ago", preinstalled: false, bloatware: false, iconColor: "#f18a00" },
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

export const sampleLeftovers: LeftoverItem[] = [
  { id: "l1", appName: "Zoom", path: "C:\\Users\\you\\AppData\\Roaming\\Zoom", sizeBytes: 640 * MB, kind: "folder" },
  { id: "l2", appName: "Skype", path: "C:\\ProgramData\\Skype", sizeBytes: 210 * MB, kind: "folder" },
  { id: "l3", appName: "Skype", path: "HKCU\\Software\\Skype", sizeBytes: 0, kind: "registry" },
  { id: "l4", appName: "Old Game Launcher", path: "C:\\Users\\you\\Desktop\\Launcher.lnk", sizeBytes: 0, kind: "shortcut" },
];

export const sampleSpace: SpaceBlock[] = [
  { id: "photos", label: "Photos and videos", sizeBytes: 62 * GB, category: "personal" },
  { id: "windows", label: "Windows", sizeBytes: 57 * GB, category: "system" },
  { id: "cod", label: "Call of Duty", sizeBytes: 48 * GB, category: "games" },
  { id: "docs", label: "Documents", sizeBytes: 21 * GB, category: "personal" },
  { id: "other-apps", label: "Other apps", sizeBytes: 15 * GB, category: "apps" },
  { id: "vs", label: "Visual Studio", sizeBytes: 9 * GB, category: "apps" },
  { id: "photoshop", label: "Adobe Photoshop", sizeBytes: 6.8 * GB, category: "apps" },
  { id: "old-dl", label: "Old downloads", sizeBytes: 4.2 * GB, category: "reclaimable" },
  { id: "unknown", label: "Unknown folders", sizeBytes: 3.8 * GB, category: "reclaimable" },
  { id: "cache", label: "Caches and temp", sizeBytes: 3.1 * GB, category: "reclaimable" },
];

export const sampleQuarantine: QuarantineItem[] = [
  { id: "q1", label: "nv_cache_old", originalPath: "C:\\Users\\you\\AppData\\Local\\nv_cache_old", sizeBytes: 2.9 * GB, removedOn: "2 days ago", expiresInDays: 5 },
  { id: "q2", label: "Discord leftovers", originalPath: "C:\\Users\\you\\AppData\\Local\\Discord", sizeBytes: 406 * MB, removedOn: "yesterday", expiresInDays: 6 },
];
