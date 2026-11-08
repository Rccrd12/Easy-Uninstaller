// Sample data used until each scanner is implemented in Rust.
// Every function in api.ts that still returns from here is marked "sample".
import { GB, MB } from "./format";
import type { FileGroup, InstalledApp, LeftoverItem, QuarantineItem, SpaceBlock } from "./types";

export const sampleApps: InstalledApp[] = [
  { id: "photoshop", name: "Adobe Photoshop 2025", publisher: "Adobe", sizeBytes: 6.8 * GB, source: "win32", installedOn: "2025-01-14", lastUsed: "3 months ago", preinstalled: false, bloatware: false, neverUsed: false, iconColor: "#e3008c", icon: null },
  { id: "xbox-bar", name: "Xbox Game Bar", publisher: "Microsoft", sizeBytes: 1.2 * GB, source: "store", installedOn: null, lastUsed: null, preinstalled: true, bloatware: false, neverUsed: true, iconColor: "#107c10", icon: null },
  { id: "spotify", name: "Spotify", publisher: "Spotify AB", sizeBytes: 740 * MB, source: "win32", installedOn: "2024-06-02", lastUsed: "today", preinstalled: false, bloatware: false, neverUsed: false, iconColor: "#1db954", icon: null },
  { id: "discord", name: "Discord", publisher: "Discord Inc.", sizeBytes: 520 * MB, source: "win32", installedOn: "2024-09-21", lastUsed: "2 weeks ago", preinstalled: false, bloatware: false, neverUsed: false, iconColor: "#5865f2", icon: null },
  { id: "mcafee", name: "McAfee LiveSafe", publisher: "McAfee", sizeBytes: 410 * MB, source: "win32", installedOn: "2024-03-01", lastUsed: null, preinstalled: true, bloatware: true, neverUsed: true, iconColor: "#ff6a00", icon: null },
  { id: "vlc", name: "VLC media player", publisher: "VideoLAN", sizeBytes: 160 * MB, source: "win32", installedOn: "2023-11-10", lastUsed: "1 month ago", preinstalled: false, bloatware: false, neverUsed: false, iconColor: "#f18a00", icon: null },
];

export const sampleFileGroups: FileGroup[] = [
  { id: "temp", title: "Temporary files and caches", description: "Windows and your browsers recreate them on their own", safety: "safe", sizeBytes: 3.1 * GB, itemCount: 4821 },
  { id: "installers", title: "Installers already used", description: ".exe/.msi files of programs already installed", safety: "safe", sizeBytes: 2.7 * GB, itemCount: 14 },
  { id: "old-downloads", title: "Old downloads", description: "Not opened in over a year", safety: "check", sizeBytes: 4.2 * GB, itemCount: 212 },
  { id: "duplicates", title: "Duplicates", description: "Identical copies: the original is always kept", safety: "check", sizeBytes: 1.3 * GB, itemCount: 38 },
  { id: "unknown", title: "Unknown folders", description: "Large folders not linked to any installed app", safety: "check", sizeBytes: 3.8 * GB, itemCount: 3 },
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
