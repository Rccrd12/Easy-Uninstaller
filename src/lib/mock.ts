// Sample data used until each scanner is implemented in Rust.
// Every function in api.ts that still returns from here is marked "sample".
import { GB, MB } from "./format";
import type { AiAnswer, AiModel, AiServiceStatus, DriveInfo, FileGroup, FolderSummary, FileItem, HistoryKind, HistoryName, InstalledApp, LeftoverItem, QuarantineEntry, QuarantineSettings, SpaceFolder, SpaceUsage, StartupApp, AppCache, NewPcAdvice, NewPcReason, UninstalledApp, OffloadFile, SearchFilter, SearchKind, SearchPlace, SearchResult, SearchSort, TidyDest, TidyFile, TidyFolder, TidyPlan } from "./types";

const remove = (reason: NewPcReason): NewPcAdvice => ({ advice: "remove", reason });
const choose = (reason: NewPcReason): NewPcAdvice => ({ advice: "choose", reason });
const app = (a: Omit<InstalledApp, "preinstalled" | "bloatware" | "newPc" | "lastUsed" | "neverUsed" | "icon" | "version" | "location" | "cacheId"> & Partial<InstalledApp>): InstalledApp => ({
  lastUsed: null,
  neverUsed: false,
  icon: null,
  version: "",
  location: null,
  cacheId: null,
  ...a,
  newPc: a.newPc ?? null,
  preinstalled: !!a.newPc,
  bloatware: a.newPc?.advice === "remove",
});

export const sampleApps: InstalledApp[] = [
  app({ id: "reg:hklm:{8F2B6C1E-4A0D-4C55-9E2F-7B1D3A5C9E01}", version: "26.2.0", location: "C:\\Program Files\\Adobe\\Adobe Photoshop 2025", name: "Adobe Photoshop 2025", publisher: "Adobe", sizeBytes: 6.8 * GB, source: "win32", installedOn: "2025-01-14", lastUsed: "3 months ago", iconColor: "#e3008c" }),
  app({ id: "store:MSTeams_24295.605.3225.8804_x64__8wekyb3d8bbwe", version: "24295.605.3225.8804", location: "C:\\Program Files\\WindowsApps\\MSTeams_24295.605.3225.8804_x64__8wekyb3d8bbwe", name: "Microsoft Teams", publisher: "Microsoft", sizeBytes: 2.4 * GB, source: "store", installedOn: "2024-03-03", lastUsed: null, iconColor: "#5b5fc7", cacheId: "teams" }),
  app({ id: "store:Microsoft.XboxGamingOverlay_7.325.10071.0_x64__8wekyb3d8bbwe", version: "7.325.10071.0", location: "C:\\Program Files\\WindowsApps\\Microsoft.XboxGamingOverlay_7.325.10071.0_x64__8wekyb3d8bbwe", name: "Xbox Game Bar", publisher: "Microsoft", sizeBytes: 1.2 * GB, source: "store", installedOn: null, neverUsed: true, iconColor: "#107c10", newPc: choose("xbox") }),
  app({ id: "reg:hkcu:Spotify", version: "1.2.52.442", location: "C:\\Users\\you\\AppData\\Roaming\\Spotify", name: "Spotify", publisher: "Spotify AB", sizeBytes: 380 * MB, source: "win32", installedOn: "2024-06-02", lastUsed: "today", iconColor: "#1db954", cacheId: "spotify" }),
  app({ id: "reg:hklm:Google Chrome", version: "131.0.6778.86", location: "C:\\Program Files\\Google\\Chrome\\Application", name: "Google Chrome", publisher: "Google LLC", sizeBytes: 420 * MB, source: "win32", installedOn: "2023-09-12", lastUsed: "today", iconColor: "#c58a00", cacheId: "chrome" }),
  app({ id: "reg:hkcu:Discord", version: "1.0.9178", location: "C:\\Users\\you\\AppData\\Local\\Discord", name: "Discord", publisher: "Discord Inc.", sizeBytes: 420 * MB, source: "win32", installedOn: "2024-09-21", lastUsed: "2 weeks ago", iconColor: "#5865f2", cacheId: "discord" }),
  app({ id: "reg:hklm32:McAfee.WPS", version: "16.0", location: "C:\\Program Files\\McAfee", name: "McAfee LiveSafe", publisher: "McAfee", sizeBytes: 410 * MB, source: "win32", installedOn: "2024-03-01", neverUsed: true, iconColor: "#c8102e", newPc: remove("trial") }),
  app({ id: "clipchamp", name: "Clipchamp", publisher: "Microsoft", sizeBytes: 310 * MB, source: "store", installedOn: "2024-03-01", iconColor: "#8764b8", newPc: choose("video") }),
  app({ id: "candy", name: "Candy Crush Saga", publisher: "king.com", sizeBytes: 280 * MB, source: "store", installedOn: "2024-03-01", iconColor: "#e3008c", newPc: remove("game") }),
  app({ id: "solitaire", name: "Microsoft Solitaire Collection", publisher: "Microsoft Studios", sizeBytes: 210 * MB, source: "store", installedOn: "2024-03-01", iconColor: "#0067c0", newPc: choose("game") }),
  app({ id: "reg:hklm:VLC media player", version: "3.0.21", location: "C:\\Program Files\\VideoLAN\\VLC", name: "VLC media player", publisher: "VideoLAN", sizeBytes: 160 * MB, source: "win32", installedOn: "2023-11-10", lastUsed: "1 month ago", iconColor: "#f18a00" }),
  app({ id: "tiktok", name: "TikTok", publisher: "TikTok Pte. Ltd.", sizeBytes: 140 * MB, source: "store", installedOn: "2024-03-01", iconColor: "#486860", newPc: choose("social") }),
  app({ id: "news", name: "Microsoft News", publisher: "Microsoft Corporation", sizeBytes: 64 * MB, source: "store", installedOn: "2024-03-01", iconColor: "#ca5010", newPc: remove("rarely") }),
  app({ id: "gethelp", name: "Get Help", publisher: "Microsoft Corporation", sizeBytes: 38 * MB, source: "store", installedOn: "2024-03-01", iconColor: "#038387", newPc: remove("rarely") }),
  app({ id: "jumpstart", name: "HP JumpStart", publisher: "HP Inc.", sizeBytes: 26 * MB, source: "store", installedOn: "2024-03-01", iconColor: "#0067c0", newPc: remove("promo") }),
  app({ id: "skype", name: "Skype", publisher: "Skype", sizeBytes: 190 * MB, source: "store", installedOn: "2024-03-01", iconColor: "#0078d4", newPc: remove("retired") }),
];

/** Caches of the sample apps. Discord starts open, to try the "close it first" step. */
export const sampleAppCaches: AppCache[] = [
  { id: "teams", cacheBytes: 1.8 * GB, fileCount: 5210, dataBytes: 210 * MB, running: false },
  { id: "spotify", cacheBytes: 1.4 * GB, fileCount: 2380, dataBytes: 120 * MB, running: false },
  { id: "discord", cacheBytes: 640 * MB, fileCount: 1904, dataBytes: 85 * MB, running: true },
  { id: "chrome", cacheBytes: 520 * MB, fileCount: 8730, dataBytes: 340 * MB, running: false },
];

/** Apps "uninstalled earlier" in the browser fallback. Times are relative to now. */
export function sampleUninstalledApps(now: number): UninstalledApp[] {
  const day = 86_400;
  return [
    { id: "teams", name: "Microsoft Teams (personal)", publisher: "Microsoft", source: "store", sizeBytes: 2.4 * GB, uninstalledAt: now - 2 * day, way: "store", site: "", link: "", iconColor: "#5b5fc7", icon: null },
    { id: "zoom", name: "Zoom Workplace", publisher: "Zoom Video Communications", source: "win32", sizeBytes: 380 * MB, uninstalledAt: now - 5 * day, way: "website", site: "zoom.us", link: "", iconColor: "#0b5cff", icon: null },
    { id: "dota", name: "Dota 2", publisher: "Valve", source: "win32", sizeBytes: 38 * GB, uninstalledAt: now - 9 * day, way: "steam", site: "", link: "", iconColor: "#a3241a", icon: null },
    { id: "7zip", name: "7-Zip 24.08 (x64)", publisher: "Igor Pavlov", source: "win32", sizeBytes: 6 * MB, uninstalledAt: now - 21 * day, way: "website", site: "7-zip.org", link: "", iconColor: "#486860", icon: null },
    { id: "discord-old", name: "Discord", publisher: "Discord Inc.", source: "win32", sizeBytes: 480 * MB, uninstalledAt: now - 30 * day, way: "website", site: "discord.com", link: "", iconColor: "#5865f2", icon: null },
    { id: "foxit", name: "Foxit PDF Reader", publisher: "Foxit Software Inc.", source: "win32", sizeBytes: 520 * MB, uninstalledAt: now - 40 * day, way: "search", site: "", link: "", iconColor: "#ca5010", icon: null },
  ];
}

const U = "C:\\Users\\you";
const D = `${U}\\Downloads`;
const P = `${U}\\Pictures`;

/** A sample file on the pretend disk. `group` is the file group it belongs to. */
export interface SampleFile {
  group: string;
  path: string;
  sizeBytes: number;
  lastUsed: string;
  modified: string;
}
const file = (group: string, path: string, sizeBytes: number, modified: string, lastUsed = modified): SampleFile => ({ group, path, sizeBytes, lastUsed, modified });

/** Files of the sample file groups, so the review can be tried in the browser. */
export const sampleFiles: SampleFile[] = [
  file("installers", `${D}\\Adobe_Photoshop_2025_Set-Up.exe`, 1.9 * GB, "2025-01-14"),
  file("installers", `${D}\\SpotifyFullSetup.exe`, 610 * MB, "2024-06-02"),
  file("installers", `${D}\\vlc-3.0.20-win64.exe`, 42 * MB, "2023-11-10"),
  file("old-downloads", `${D}\\Wedding video raw.mp4`, 2.4 * GB, "2023-05-20"),
  file("old-downloads", `${D}\\ubuntu-22.04-desktop-amd64.iso`, 1.2 * GB, "2023-02-11"),
  file("old-downloads", `${D}\\Vacation photos.zip`, 860 * MB, "2023-09-03"),
  file("old-downloads", `${D}\\Concert 2023.mov`, 690 * MB, "2023-07-15"),
  file("old-downloads", `${D}\\Driver_pack_2022.zip`, 410 * MB, "2022-10-01"),
  file("old-downloads", `${D}\\Scanned receipts.pdf`, 12 * MB, "2023-03-28"),
  file("old-downloads", `${D}\\IMG_2231.jpg`, 4.2 * MB, "2023-08-12"),
  file("old-downloads", `${D}\\IMG_2232.jpg`, 3.9 * MB, "2023-08-12"),
  file("old-downloads", `${D}\\Tax return 2022.pdf`, 3 * MB, "2023-04-30"),
  file("old-downloads", `${D}\\Holiday map.png`, 2.1 * MB, "2023-06-30"),
  file("old-downloads", `${D}\\Old resume.docx`, 0.3 * MB, "2022-11-18"),
  file("large-files", `${U}\\Videos\\Game capture 2025-02-03.mp4`, 5.6 * GB, "2025-02-03"),
  file("large-files", `${U}\\Documents\\VirtualBox VMs\\Windows 10.vdi`, 3.9 * GB, "2024-12-01"),
  file("large-files", `${U}\\Videos\\Family videos 2019.mkv`, 2.2 * GB, "2020-01-06"),
];

/** Sets of identical files, oldest first (the copy kept by default). */
export const sampleDuplicateSets: SampleFile[][] = [
  [file("duplicates", `${P}\\Holidays 2024\\VID_20240802.mp4`, 920 * MB, "2024-08-02"), file("duplicates", `${D}\\WhatsApp\\VID_20240802.mp4`, 920 * MB, "2025-01-14")],
  [
    file("duplicates", `${U}\\Documents\\Work\\presentation-final.pptx`, 210 * MB, "2025-03-10"),
    file("duplicates", `${D}\\presentation-final (1).pptx`, 210 * MB, "2025-03-12"),
    file("duplicates", `${U}\\Desktop\\presentation-final.pptx`, 210 * MB, "2025-03-14"),
  ],
  [file("duplicates", `${U}\\Music\\Albums\\Live in Rome.flac`, 48 * MB, "2022-04-09"), file("duplicates", `${D}\\Live in Rome.flac`, 48 * MB, "2024-02-20")],
  [file("duplicates", `${P}\\Wedding Luca\\Wedding_Luca_018.jpg`, 8.4 * MB, "2024-06-22"), file("duplicates", `${D}\\photos (3)\\Wedding_Luca_018.jpg`, 8.4 * MB, "2025-01-14")],
  [file("duplicates", `${P}\\iPhone\\2023\\IMG_4471.HEIC`, 6.1 * MB, "2023-09-17"), file("duplicates", `${U}\\Desktop\\to sort\\IMG_4471.HEIC`, 6.1 * MB, "2025-01-14")],
  [file("duplicates", `${P}\\Holidays 2024\\IMG_0912.jpg`, 5.2 * MB, "2024-08-03"), file("duplicates", `${U}\\Desktop\\IMG_0912.jpg`, 5.2 * MB, "2024-09-01")],
  [file("duplicates", `${U}\\Documents\\Contracts\\rental-contract.pdf`, 2.3 * MB, "2023-01-30"), file("duplicates", `${D}\\rental-contract.pdf`, 2.3 * MB, "2023-02-02")],
];

/** Sets of photos that look almost the same, largest first (the one kept by default). */
export const sampleSimilarSets: SampleFile[][] = [
  [
    file("similar-photos", `${P}\\Holidays 2024\\IMG_1041.jpg`, 6.2 * MB, "2024-08-04"),
    file("similar-photos", `${P}\\Holidays 2024\\IMG_1042.jpg`, 5.9 * MB, "2024-08-04"),
    file("similar-photos", `${P}\\Holidays 2024\\IMG_1043.jpg`, 5.7 * MB, "2024-08-04"),
    file("similar-photos", `${P}\\Holidays 2024\\IMG_1044.jpg`, 5.1 * MB, "2024-08-04"),
  ],
  [
    file("similar-photos", `${P}\\Dolomites\\DSC_0388.jpg`, 7.8 * MB, "2023-07-19"),
    file("similar-photos", `${P}\\Dolomites\\DSC_0389.jpg`, 7.6 * MB, "2023-07-19"),
    file("similar-photos", `${P}\\Dolomites\\DSC_0390.jpg`, 7.2 * MB, "2023-07-19"),
  ],
  [
    file("similar-photos", `${P}\\Wedding Luca\\Wedding_Luca_042.jpg`, 9.1 * MB, "2024-06-22"),
    file("similar-photos", `${D}\\WhatsApp Image 2024-06-23 at 18.02.jpg`, 0.4 * MB, "2024-06-23"),
  ],
  [
    file("similar-photos", `${P}\\iPhone\\2025\\IMG_2210.HEIC`, 3.4 * MB, "2025-02-16"),
    file("similar-photos", `${P}\\iPhone\\2025\\IMG_2211.HEIC`, 3.3 * MB, "2025-02-16"),
  ],
];

/** Sample photos of one set share a picture, each a little moved, like shots taken a second apart. */
const sampleLooks = new Map<string, { seed: string; nudge: number }>(
  sampleSimilarSets.flatMap((set) => set.map((f, i) => [f.path, { seed: set[0].path, nudge: i }] as const)),
);

const item = (path: string, sizeBytes: number, lastUsed: string | null = null, duplicateOf: string | null = null): FileItem => ({
  path,
  sizeBytes,
  lastUsed,
  duplicateOf,
  explainId: null,
  originalPath: null,
});
/** A folder the app knows (see src-tauri/src/known_folders.json), or null for one it does not. */
const folderItem = (path: string, sizeBytes: number, explainId: string | null): FileItem => ({ ...item(path, sizeBytes), explainId });
const binItem = (id: string, originalPath: string, sizeBytes: number): FileItem => ({
  ...item(`C:\\$Recycle.Bin\\S-1-5-21-1004\\$R${id}`, sizeBytes),
  explainId: "recycleBin",
  originalPath,
});

/** Temp folders: listed by folder, like the real scan does. */
export const sampleTempFolders: FileItem[] = [
  folderItem(`${U}\\AppData\\Local\\Temp`, 1.6 * GB, "temp"),
  folderItem(`${U}\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache`, 820 * MB, "browserCache"),
  folderItem("C:\\Windows\\Temp", 410 * MB, "temp"),
  folderItem(`${U}\\AppData\\Local\\NVIDIA\\DXCache`, 270 * MB, "shaderCache"),
];

/** Groups that are listed as found, not reviewed file by file (from "What is this?"). */
const sampleExtraGroups: [id: string, key: string, safety: FileGroup["safety"], itemCount: number, items: FileItem[]][] = [
  ["recycle-bin", "recycleBin", "safe", 14, [
    binItem("K2J9QX.zip", `${U}\\Downloads\\Old project`, 840 * MB),
    binItem("8DF1LA.mp4", `${U}\\Videos\\Screen recording 2025-03-02.mp4`, 310 * MB),
    binItem("Q7ZP02.psd", `${U}\\Desktop\\poster draft.psd`, 95 * MB),
  ]],
  ["crash-reports", "crashReports", "safe", 9, [
    { ...item(`${U}\\AppData\\Local\\CrashDumps\\GameLauncher.exe.8812.dmp`, 410 * MB, "2025-06-14"), explainId: "crashDumps" },
    { ...item(`${U}\\AppData\\Local\\CrashDumps\\explorer.exe.4410.dmp`, 160 * MB, "2025-04-02"), explainId: "crashDumps" },
    { ...item(`${U}\\AppData\\Local\\Microsoft\\Windows\\WER\\ReportArchive\\AppCrash_Spotify\\Report.wer`, 70 * MB, "2025-02-20"), explainId: "crashDumps" },
  ]],
  ["thumbnails", "thumbnails", "safe", 6, [
    { ...item(`${U}\\AppData\\Local\\Microsoft\\Windows\\Explorer\\thumbcache_256.db`, 380 * MB, "2025-07-01"), explainId: "thumbCache" },
    { ...item(`${U}\\AppData\\Local\\Microsoft\\Windows\\Explorer\\thumbcache_1280.db`, 210 * MB, "2025-07-01"), explainId: "thumbCache" },
  ]],
  ["unknown-folders", "unknownFolders", "check", 13, [
    folderItem("C:\\ProgramData\\Package Cache", 4.2 * GB, "packageCache"),
    folderItem(`${U}\\AppData\\Roaming\\Zoom`, 1.1 * GB, "zoom"),
    folderItem(`${U}\\AppData\\Local\\ABX_7731`, 820 * MB, null),
    folderItem(`${U}\\AppData\\Local\\Spotify`, 640 * MB, "spotify"),
    folderItem(`${U}\\AppData\\Local\\qbt-data`, 610 * MB, null),
    folderItem("C:\\ProgramData\\Temp_Updater_x64", 480 * MB, null),
    folderItem(`${U}\\AppData\\Local\\NVIDIA`, 450 * MB, "shaderCache"),
    folderItem(`${U}\\AppData\\Local\\npm-cache`, 380 * MB, "devCache"),
    folderItem(`${U}\\AppData\\Roaming\\anythingllm-desktop`, 360 * MB, null),
    folderItem(`${U}\\AppData\\Roaming\\Code`, 350 * MB, "vscode"),
    folderItem("C:\\ProgramData\\NVIDIA Corporation", 340 * MB, "gpuDrivers"),
    folderItem(`${U}\\AppData\\Local\\Docker`, 330 * MB, "docker"),
    folderItem(`${U}\\AppData\\Local\\uv`, 320 * MB, null),
  ]],
];

/**
 * Builds the sample groups from the sample files that `present` accepts,
 * like src-tauri/src/review.rs rebuilds the real ones.
 */
export function buildSampleGroups(present: (path: string) => boolean): FileGroup[] {
  const group = (id: string, key: string, safety: FileGroup["safety"], itemCount: number, items: FileItem[]): FileGroup => ({
    id,
    titleKey: `files.groups.${key}.title`,
    descriptionKey: `files.groups.${key}.description`,
    safety,
    sizeBytes: items.reduce((s, i) => s + i.sizeBytes, 0),
    itemCount,
    items: [...items].sort((a, b) => b.sizeBytes - a.sizeBytes),
    reviewOnly: id === "unknown-folders",
  });
  const files = (id: string): FileItem[] =>
    sampleFiles.filter((f) => f.group === id && present(f.path)).map((f) => item(f.path, f.sizeBytes, f.lastUsed));
  const temp = sampleTempFolders.filter((f) => present(f.path));
  const copies: FileItem[] = sampleDuplicateSets.flatMap((set) => {
    const left = set.filter((f) => present(f.path));
    return left.slice(1).map((f) => item(f.path, f.sizeBytes, f.lastUsed, left[0].path));
  });
  const similar: FileItem[] = sampleSimilarSets.flatMap((set) => {
    const left = set.filter((f) => present(f.path));
    return left.length > 1 ? left.slice(1).map((f) => item(f.path, f.sizeBytes, f.lastUsed)) : [];
  });
  const extra = (id: string): FileGroup => {
    const [, key, safety, count, items] = sampleExtraGroups.find((g) => g[0] === id)!;
    const left = items.filter((i) => present(i.path));
    return group(id, key, safety, left.length === items.length ? count : left.length, left);
  };
  return [
    group("temp", "temp", "safe", Math.round((4821 * temp.length) / sampleTempFolders.length), temp),
    extra("recycle-bin"),
    extra("crash-reports"),
    extra("thumbnails"),
    group("installers", "installers", "safe", files("installers").length, files("installers")),
    group("old-downloads", "oldDownloads", "check", files("old-downloads").length, files("old-downloads")),
    group("duplicates", "duplicates", "check", copies.length, copies),
    group("similar-photos", "similarPhotos", "check", similar.length, similar),
    group("large-files", "largeFiles", "check", files("large-files").length, files("large-files")),
    extra("unknown-folders"),
  ].filter((g) => g.itemCount > 0);
}

/**
 * A pretend photo for image and video files, so the previews can be tried in the
 * browser. Colors follow the file name, so each file keeps its own picture.
 */
export function samplePreview(path: string): string | null {
  if (!/\.(jpe?g|png|heic|mp4|mov|mkv)$/i.test(path)) return null;
  const look = sampleLooks.get(path);
  const name = look?.seed ?? path.split("\\").pop() ?? path;
  const nudge = look?.nudge ?? 0;
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360;
  const sky = `hsl(${h},70%,72%)`;
  const sky2 = `hsl(${(h + 40) % 360},75%,88%)`;
  const hill = `hsl(${(h + 120) % 360},40%,42%)`;
  const hill2 = `hsl(${(h + 150) % 360},45%,55%)`;
  // Shots of one moment: the camera moves a little and the person in it steps along.
  const person = look ? `<g fill="#2b2b33"><circle cx="${56 + nudge * 5}" cy="70" r="6"/><rect x="${50 + nudge * 5}" y="77" width="12" height="24" rx="5"/></g>` : "";
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120"><defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${sky}"/><stop offset="1" stop-color="${sky2}"/></linearGradient></defs><rect width="120" height="120" fill="url(#s)"/><g transform="translate(${-2 * nudge} ${nudge})"><circle cx="${30 + (h % 60)}" cy="34" r="12" fill="#fff6c8"/><path d="M-10 84 Q30 60 62 80 T130 72 V130 H-10Z" fill="${hill2}"/><path d="M-10 100 Q40 80 80 98 T130 92 V130 H-10Z" fill="${hill}"/>${person}</g></svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

const leftover = (id: string, appName: string | null, kind: LeftoverItem["kind"], path: string, sizeBytes: number, reason: string, safety: LeftoverItem["safety"] = "safe", needsAdmin = false): LeftoverItem => ({ id, appName, kind, path, sizeBytes, reason, safety, needsAdmin, unowned: false, modifiedAt: null });

/** A folder (or key) that no installed app owns, last changed `daysAgo` days ago. */
const unowned = (id: string, appName: string, kind: LeftoverItem["kind"], path: string, sizeBytes: number, reason: string, daysAgo: number | null, needsAdmin = false): LeftoverItem => ({
  ...leftover(id, appName, kind, path, sizeBytes, reason, "check", needsAdmin),
  unowned: true,
  modifiedAt: daysAgo === null ? null : Math.floor(Date.now() / 1000) - daysAgo * 86400,
});

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
  unowned("u1", "FotoMagic", "folder", "C:\\Program Files\\FotoMagic", 1.4 * GB, "programFolder", 820, true),
  unowned("u2", "FotoMagic", "folder", `${U}\\AppData\\Roaming\\FotoMagic`, 36 * MB, "userSettings", 790),
  unowned("u3", "FotoMagic", "registry", "HKCU\\Software\\FotoMagic", 0, "registry", null),
  unowned("u4", "TurboRecorder", "folder", `${U}\\AppData\\Local\\TurboRecorder`, 420 * MB, "localData", 400),
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

/** A history record as src-tauri/src/history.rs saves it (`Record`). */
export interface SampleRecord {
  id: string;
  at: number;
  kind: HistoryKind;
  names: HistoryName[];
  itemCount: number;
  deletedBytes: number;
  crossedBytes: number;
  entries: { id: string; bytes: number }[];
  estimatedBytes: number;
  restored: string[];
  restoredAt: number | null;
}

const record = (at: number, kind: HistoryKind, names: HistoryName[], itemCount: number, deletedBytes: number, rest: Partial<SampleRecord> = {}): SampleRecord => ({
  id: `${at}-${kind}`,
  at,
  kind,
  names,
  itemCount,
  deletedBytes,
  crossedBytes: 0,
  entries: [],
  estimatedBytes: 0,
  restored: [],
  restoredAt: null,
  ...rest,
});
const group = (key: string): HistoryName => ({ key: `files.groups.${key}.title` });
const hours = (h: number) => h * 60 * 60;

/** What the browser fallback shows in History, oldest first. q1 and q2 are still in Quarantine. */
export const sampleHistory: SampleRecord[] = [
  record(daysAgo(41), "appFolder", [{ text: "OldGameLauncher" }], 1, 0, { entries: [{ id: "old-1", bytes: 2.2 * GB }] }),
  record(daysAgo(40), "uninstall", [{ text: "Candy Crush Saga" }], 1, 0, { estimatedBytes: 380 * MB }),
  record(daysAgo(23), "quickClean", [group("temp"), group("recycleBin"), group("thumbnails")], 6214, 4.1 * GB, { entries: [{ id: "old-2", bytes: 2.7 * GB }] }),
  record(daysAgo(16), "files", [group("installers")], 9, 0, { entries: [{ id: "old-3", bytes: 1.2 * GB }], restored: ["old-3"], restoredAt: daysAgo(15) }),
  record(daysAgo(9), "files", [group("oldDownloads")], 46, 0, { entries: [{ id: "old-4", bytes: 3.4 * GB }] }),
  record(daysAgo(2), "files", [group("temp")], 1312, 0, { entries: [{ id: "q1", bytes: 2.9 * GB }] }),
  record(daysAgo(1) - hours(1), "uninstall", [{ text: "Skype" }], 1, 0, { estimatedBytes: 1.1 * GB }),
  record(daysAgo(1), "leftovers", [{ text: "Discord" }], 3, 0, { entries: [{ id: "q2", bytes: 406 * MB }] }),
  record(daysAgo(0) - hours(3), "quickClean", [group("temp"), group("crashReports"), group("thumbnails")], 4210, 5.3 * GB),
];

/** Changed by `setQuarantineSettings` in the browser fallback. */
export const sampleQuarantineSettings: QuarantineSettings = { keepDays: 7, drive: null };

export const sampleDrives: DriveInfo[] = [{ mountPoint: "D:", label: "Data", totalBytes: 931 * GB, freeBytes: 612 * GB }];

// ── AI help ───────────────────────────────────────────────────────────────

export const sampleAiServices: AiServiceStatus[] = [
  { id: "claude-code", found: true, models: null, hasKey: false, canStart: false },
  { id: "codex", found: true, models: null, hasKey: false, canStart: false },
  { id: "antigravity-cli", found: false, models: null, hasKey: false, canStart: false },
  { id: "anthropic", found: true, models: null, hasKey: false, canStart: false },
  { id: "openai", found: true, models: null, hasKey: false, canStart: false },
  { id: "google", found: true, models: null, hasKey: false, canStart: false },
  { id: "openrouter", found: true, models: null, hasKey: false, canStart: false },
  { id: "custom", found: true, models: null, hasKey: false, canStart: false },
  { id: "ollama", found: true, models: 3, hasKey: false, canStart: false },
  { id: "lmstudio", found: true, models: 2, hasKey: false, canStart: false },
  // Installed but not running: the app starts it.
  { id: "unsloth", found: false, models: null, hasKey: false, canStart: true },
  { id: "local-custom", found: false, models: null, hasKey: false, canStart: false },
];

const m = (id: string, label = id): AiModel => ({ id, label });
export const sampleAiModels: Record<string, AiModel[]> = {
  "claude-code": [m("sonnet", "Sonnet"), m("opus", "Opus"), m("haiku", "Haiku")],
  codex: [m("gpt-5.5", "GPT-5.5"), m("gpt-5.5-mini", "GPT-5.5 mini")],
  "antigravity-cli": [m("gemini-3.1-pro-high"), m("gemini-3.8-flash-low")],
  anthropic: [m("claude-sonnet-5-5", "Claude Sonnet 5.5"), m("claude-haiku-5-5", "Claude Haiku 5.5"), m("claude-opus-5-5", "Claude Opus 5.5")],
  openai: [m("gpt-5.5"), m("gpt-5.5-mini")],
  google: [m("gemini-3-flash"), m("gemini-3.1-pro")],
  openrouter: [m("anthropic/claude-sonnet-5.5", "Anthropic: Claude Sonnet 5.5"), m("google/gemini-3-flash", "Google: Gemini 3 Flash")],
  custom: [m("llama-4-scout")],
  ollama: [m("qwen3:8b"), m("gemma3:12b"), m("llama3.2:3b")],
  lmstudio: [m("qwen3-14b"), m("mistral-small-3.2")],
  unsloth: [m("unsloth/Qwen3-8B-GGUF")],
  "local-custom": [m("local-model")],
};

const DAY_S = 24 * 60 * 60;
const nowS = () => Math.floor(Date.now() / 1000);

/** What Rust would send about each sample unknown folder. */
export function sampleFolderSummary(path: string, sizeBytes: number): FolderSummary {
  const name = path.split("\\").pop() ?? path;
  const root = path.startsWith("C:\\ProgramData") ? "ProgramData" : path.includes("\\Roaming\\") ? "APPDATA" : "LOCALAPPDATA";
  const sets: Record<string, [string[], string[], number]> = {
    ABX_7731: [["Cache", "VideoCache"], ["data_0001.blk", "data_0002.blk", "res_main.pak", "pkg_version", "config.ini", "output_log.txt"], 1204],
    "qbt-data": [["BT_backup", "logs"], ["qBittorrent-data.conf", "watched_folders.json", "a3f1c9.fastresume", "a3f1c9.torrent"], 212],
    Temp_Updater_x64: [["pkg"], ["update.bin", "manifest.dat", "patch_001.tmp"], 37],
    "anythingllm-desktop": [["storage", "logs"], ["anythingllm.db", "documents.json", "vector-cache.bin"], 640],
    uv: [["cache", "python"], ["CACHEDIR.TAG", ".gitignore", "wheels-v5", "archive-v0"], 18_402],
  };
  const [subfolders, names, fileCount] = sets[name] ?? [[], [], 10];
  return { path: `%${root}%\\${name}`, sizeBytes, fileCount, modified: nowS() - 420 * DAY_S, subfolders, names };
}

type SampleAnswer = Pick<AiAnswer, "what" | "madeBy" | "verdict" | "confidence" | "reason" | "label" | "overridden"> & { installedApp?: string };
const sampleAnswers: Record<string, { en: SampleAnswer; it: SampleAnswer }> = {
  ABX_7731: {
    en: { what: "Almost certainly the cache of a game: hundreds of .blk and .pak files and a VideoCache folder, typical of HoYoverse games (for example Genshin Impact).", madeBy: "HoYoverse", verdict: "removable", confidence: "medium", reason: "It only holds downloaded game data, which the game downloads again if needed.", label: "Cache of a removed game", overridden: false },
    it: { what: "È quasi certamente la cache di un gioco: centinaia di file .blk e .pak e una cartella VideoCache, tipici dei giochi di HoYoverse (per esempio Genshin Impact).", madeBy: "HoYoverse", verdict: "removable", confidence: "medium", reason: "Contiene solo dati scaricati dal gioco, che li riscarica se servono.", label: "Cache di un gioco tolto", overridden: false },
  },
  "qbt-data": {
    en: { what: "Data of qBittorrent: the list of your torrents and where each download had got to.", madeBy: "qBittorrent", verdict: "keep", confidence: "high", reason: "qBittorrent is installed: without this folder it forgets your downloads.", label: "Used by qBittorrent", overridden: true, installedApp: "qBittorrent 5.1.2 (x64)" },
    it: { what: "Dati di qBittorrent: l'elenco dei tuoi torrent e il punto a cui era arrivato ogni download.", madeBy: "qBittorrent", verdict: "keep", confidence: "high", reason: "qBittorrent è installato: senza questa cartella perde i tuoi download.", label: "Usata da qBittorrent", overridden: true, installedApp: "qBittorrent 5.1.2 (x64)" },
  },
  Temp_Updater_x64: {
    en: { what: "Probably what an app's updater left behind, but the names do not say which app.", madeBy: "", verdict: "unknown", confidence: "low", reason: "Too little to tell what it belongs to.", label: "Don't know", overridden: false },
    it: { what: "Probabilmente quello che ha lasciato il programma di aggiornamento di un'app, ma i nomi non dicono quale.", madeBy: "", verdict: "unknown", confidence: "low", reason: "Troppo poco per capire di chi è.", label: "Non lo so", overridden: false },
  },
  "anythingllm-desktop": {
    en: { what: "The data of AnythingLLM, a chat app for AI models: your chats and the documents you gave it.", madeBy: "Mintplex Labs", verdict: "keep", confidence: "high", reason: "It holds your own chats and documents.", label: "AnythingLLM chats", overridden: false },
    it: { what: "I dati di AnythingLLM, un'app per chattare con i modelli AI: le tue chat e i documenti che le hai dato.", madeBy: "Mintplex Labs", verdict: "keep", confidence: "high", reason: "Contiene le tue chat e i tuoi documenti.", label: "Chat di AnythingLLM", overridden: false },
  },
  uv: {
    en: { what: "The cache of uv, a tool for Python developers: copies of downloaded packages.", madeBy: "Astral", verdict: "removable", confidence: "high", reason: "It is only a cache: uv downloads the packages again when needed.", label: "Python package cache", overridden: false },
    it: { what: "La cache di uv, uno strumento per chi programma in Python: copie dei pacchetti scaricati.", madeBy: "Astral", verdict: "removable", confidence: "high", reason: "È solo una cache: uv riscarica i pacchetti quando servono.", label: "Cache di pacchetti Python", overridden: false },
  },
};

/** The answer the sample AI gives for `path`. */
export function sampleAiAnswer(path: string, lang: string, service: string, model: string): AiAnswer {
  const name = path.split("\\").pop() ?? path;
  const set = sampleAnswers[name] ?? sampleAnswers.Temp_Updater_x64;
  const a = lang === "it" ? set.it : set.en;
  return {
    path,
    what: a.what,
    madeBy: a.madeBy,
    verdict: a.verdict,
    confidence: a.confidence,
    reason: a.reason,
    label: a.label,
    overridden: a.overridden,
    facts: { installedApp: a.installedApp ?? null, modified: nowS() - 420 * DAY_S },
    service,
    model,
    askedAt: nowS(),
  };
}

const startupApp = (a: Partial<StartupApp> & Pick<StartupApp, "id" | "name">): StartupApp => ({
  publisher: "", note: null, fileName: "", command: "", enabled: true, impact: "low", advice: "none",
  allUsers: false, iconColor: "#0067c0", icon: null, ...a,
});

/** Programs that start with Windows, for the browser fallback. */
export const sampleStartup: StartupApp[] = [
  startupApp({ id: "hkcu-run:Spotify", name: "Spotify", publisher: "Spotify AB", note: "background", fileName: "Spotify.exe", command: `"${U}\\AppData\\Roaming\\Spotify\\Spotify.exe" --autostart --minimized`, impact: "high", advice: "turnOff", iconColor: "#1db954" }),
  startupApp({ id: "store:MSTeams_8wekyb3d8bbwe\\TeamsTfwStartupTask", name: "Microsoft Teams", publisher: "Microsoft Corporation", note: "chat", command: "MSTeams_8wekyb3d8bbwe\\TeamsTfwStartupTask", impact: "high", advice: "turnOff", iconColor: "#5b5fc7" }),
  startupApp({ id: "hkcu-run:Steam", name: "Steam", publisher: "Valve Corporation", note: "games", fileName: "steam.exe", command: `"C:\\Program Files (x86)\\Steam\\steam.exe" -silent`, impact: "high", advice: "turnOff", iconColor: "#1b2838" }),
  startupApp({ id: "hklm-run32:AdobeGCInvoker-1.0", name: "Adobe GC Invoker Utility", publisher: "Adobe Inc.", note: "updater", fileName: "AGCInvokerUtility.exe", command: `"C:\\Program Files (x86)\\Common Files\\Adobe\\AdobeGCClient\\AGCInvokerUtility.exe"`, impact: "medium", advice: "turnOff", allUsers: true, iconColor: "#e1251b" }),
  startupApp({ id: "hkcu-run:OneDrive", name: "OneDrive", publisher: "Microsoft Corporation", note: "sync", fileName: "OneDrive.exe", command: `"C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe" /background`, impact: "medium", advice: "keep", iconColor: "#0364b8" }),
  startupApp({ id: "hklm-run:RtkAudUService", name: "Realtek HD Audio Universal Service", publisher: "Realtek Semiconductor", note: "driver", fileName: "RtkAudUService64.exe", command: `"C:\\Windows\\System32\\DriverStore\\FileRepository\\realtekservice.inf_amd64\\RtkAudUService64.exe" -background`, impact: "low", advice: "keep", allUsers: true, iconColor: "#486860" }),
  startupApp({ id: "hkcu-run:NoteSync", name: "NoteSync Helper", publisher: "NoteSync Ltd", fileName: "notesync-helper.exe", command: `"${U}\\AppData\\Local\\NoteSync\\notesync-helper.exe"`, impact: "low", iconColor: "#038387" }),
  startupApp({ id: "hklm-run:SecurityHealth", name: "Windows Security", publisher: "Microsoft Corporation", note: "protects", fileName: "SecurityHealthSystray.exe", command: "%windir%\\system32\\SecurityHealthSystray.exe", impact: "low", advice: "system", allUsers: true, iconColor: "#2b2b2b" }),
  startupApp({ id: "hkcu-run:Discord", name: "Discord", publisher: "Discord Inc.", note: "background", fileName: "Update.exe", command: `${U}\\AppData\\Local\\Discord\\Update.exe --processStart Discord.exe`, enabled: false, impact: "high", advice: "turnOff", iconColor: "#5865f2" }),
];

// ── Free up space without deleting ────────────────────────────────────────

const OD = `${U}\\OneDrive`;
const offloadFile = (path: string, sizeBytes: number, lastUsed: string): OffloadFile => ({ path, sizeBytes, lastUsed });

/** OneDrive files on the PC and in the cloud, largest first. The rest are many small photos. */
export const sampleOnlineFiles: OffloadFile[] = [
  offloadFile(`${OD}\\Videos\\Luca's wedding.mp4`, 3.2 * GB, "2024-07-02"),
  offloadFile(`${OD}\\Pictures\\2019\\Holidays 2019.zip`, 2.8 * GB, "2020-01-12"),
  offloadFile(`${OD}\\Documents\\Phone backup.zip`, 1.9 * GB, "2023-11-30"),
  offloadFile(`${OD}\\Videos\\Graduation.mov`, 1.1 * GB, "2022-10-21"),
  offloadFile(`${OD}\\Pictures\\Camera Roll\\VID_20230812.mp4`, 740 * MB, "2023-08-12"),
  offloadFile(`${OD}\\Documents\\Thesis\\Thesis - final.pdf`, 210 * MB, "2021-03-09"),
  offloadFile(`${OD}\\Pictures\\Camera Roll\\IMG_4471.HEIC`, 6.1 * MB, "2023-09-17"),
];
export const sampleOnlineCount = 3408;
export const sampleOnlineBytes = 12.4 * GB;

/** Videos, archives and disk images in the user's folders, largest first. */
export const sampleMoveFiles: OffloadFile[] = [
  offloadFile(`${U}\\Videos\\Game capture 2025-02-03.mp4`, 5.6 * GB, "2025-02-03"),
  offloadFile(`${U}\\Videos\\Family videos 2019.mkv`, 2.2 * GB, "2020-01-06"),
  offloadFile(`${D}\\ubuntu-22.04-desktop-amd64.iso`, 1.2 * GB, "2023-02-11"),
  offloadFile(`${D}\\Vacation photos.zip`, 860 * MB, "2023-09-03"),
  offloadFile(`${U}\\Music\\Albums\\Live in Rome.flac`, 480 * MB, "2022-04-09"),
];

// ── Finding files by a sentence, and tidying Downloads ─────────────────────

/** Files of the pretend profile the sample search looks through. */
export const sampleSearchPool: SampleFile[] = [
  ...sampleFiles,
  ...sampleDuplicateSets.flat(),
  file("", `${P}\\Sardinia 2022\\Beach - drone.mp4`, 4.1 * GB, "2023-09-02"),
  file("", `${D}\\GOPR0193.MP4`, 2.3 * GB, "2024-05-11"),
  file("", `${U}\\Desktop\\old\\Mountain_4K.mov`, 1.7 * GB, "2024-02-18"),
  file("", `${D}\\WhatsApp\\VID_20210812.mp4`, 640 * MB, "2021-08-12"),
  file("", `${U}\\Videos\\Birthday Anna.mp4`, 820 * MB, "2025-10-30"),
  file("", `${U}\\Documents\\Bills\\Electricity March 2025.pdf`, 0.4 * MB, "2025-03-28"),
  file("", `${U}\\Documents\\Bills\\Gas Q1 2025.pdf`, 0.3 * MB, "2025-04-12"),
  file("", `${P}\\Sardinia 2022\\IMG_0441.jpg`, 5.8 * MB, "2023-09-02"),
  file("", `${U}\\Music\\Podcasts\\Episode 112.mp3`, 64 * MB, "2024-01-09"),
];

/**
 * In the browser there is no AI to ask: a few words of the sentence become the filter,
 * so the screen can be tried. The real filter comes from the AI (search.rs).
 */
export function sampleUnderstand(sentence: string): SearchFilter {
  const s = sentence.toLowerCase();
  const kinds: SearchKind[] = [];
  if (/video|film|clip/.test(s)) kinds.push("video");
  if (/foto|photo|immagin|picture/.test(s)) kinds.push("image");
  if (/document|pdf|bollett|bill|fattur|invoice/.test(s)) kinds.push("document");
  if (/musica|music|canzon|song|podcast/.test(s)) kinds.push("audio");
  const size = /(\d+(?:[.,]\d+)?)\s*(mb|gb)/.exec(s);
  const minBytes = size ? Number(size[1].replace(",", ".")) * (size[2] === "gb" ? GB : MB) : null;
  const unusedDays = /anno|year/.test(s) ? 365 : /mes[ie]|month/.test(s) ? 180 : null;
  const recentDays = /settimana|week/.test(s) ? 7 : null;
  const places: SearchPlace[] = [];
  if (/download/.test(s)) places.push("downloads");
  if (/desktop/.test(s)) places.push("desktop");
  // A topic ("delle vacanze", "of the wedding") that no file name can show.
  const topic = /\b(?:delle|della|del|dei|degli|dello|of the|of my|from the|from my)\s+([a-zà-ù]+)/i.exec(sentence);
  const named = /\b(?:holiday|vacation|wedding|birthday)s?\b/i.exec(sentence);
  const unsure = topic && !/^(anno|anni|mesi|mese|settimana|year|years|months?|weeks?|download|desktop)$/i.test(topic[1]) ? [topic[0]] : named ? [named[0]] : [];
  return { kinds, minBytes, maxBytes: null, unusedDays, recentDays, places, words: [], sort: /vecch|old/.test(s) && !minBytes ? "oldest" : "size", unsure };
}

const KIND_OF: [SearchKind, RegExp][] = [
  ["image", /\.(jpe?g|png|gif|bmp|webp|heic|heif|tiff?|dng|cr2|nef|arw|psd|svg)$/i],
  ["video", /\.(mp4|mov|avi|mkv|wmv|m4v|webm|3gp|mts)$/i],
  ["audio", /\.(mp3|wav|flac|m4a|aac|ogg|wma)$/i],
  ["document", /\.(pdf|txt|md|rtf|csv|log|docx?|xlsx?|pptx?|odt|ods|odp|epub)$/i],
  ["archive", /\.(zip|7z|rar|gz|tar)$/i],
  ["program", /\.(exe|msi|msix|msixbundle|appx|bat|cmd)$/i],
  ["disc", /\.(iso|img|vdi|vhdx?|vmdk)$/i],
];
const kindOf = (path: string) => KIND_OF.find(([, re]) => re.test(path))?.[0] ?? null;
const PLACE_DIRS: Record<SearchPlace, string> = { desktop: "Desktop", documents: "Documents", downloads: "Downloads", pictures: "Pictures", videos: "Videos", music: "Music" };

/** The sample search: the same rules as search.rs, over `files`. */
export function sampleSearch(filter: SearchFilter, files: SampleFile[]): SearchResult {
  const now = Date.now();
  const age = (iso: string) => (now - new Date(`${iso}T00:00:00`).getTime()) / 86_400_000;
  const found = files.filter((f) => {
    const kind = kindOf(f.path);
    const rel = f.path.slice(U.length + 1).toLowerCase();
    if (filter.kinds.length && (!kind || !filter.kinds.includes(kind))) return false;
    if (filter.minBytes !== null && f.sizeBytes < filter.minBytes) return false;
    if (filter.maxBytes !== null && f.sizeBytes > filter.maxBytes) return false;
    if (filter.unusedDays !== null && age(f.lastUsed) < filter.unusedDays) return false;
    if (filter.recentDays !== null && age(f.modified) > filter.recentDays) return false;
    if (filter.places.length && !filter.places.some((p) => rel.startsWith(`${PLACE_DIRS[p].toLowerCase()}\\`))) return false;
    return filter.words.every((w) => rel.includes(w.toLowerCase()));
  });
  const order: Record<SearchSort, (a: SampleFile, b: SampleFile) => number> = {
    size: (a, b) => b.sizeBytes - a.sizeBytes,
    oldest: (a, b) => a.lastUsed.localeCompare(b.lastUsed),
    newest: (a, b) => b.modified.localeCompare(a.modified),
  };
  found.sort(order[filter.sort]);
  return {
    filter,
    files: found.slice(0, 200).map((f) => ({ path: f.path, sizeBytes: f.sizeBytes, lastUsed: f.lastUsed })),
    totalCount: found.length,
    totalBytes: found.reduce((s, f) => s + f.sizeBytes, 0),
    tooMany: false,
    partial: false,
  };
}

/** Files lying loose in the pretend Downloads folder. */
export const sampleLooseDownloads: SampleFile[] = [
  file("", `${D}\\Electricity bill March 2025.pdf`, 0.4 * MB, "2025-03-28"),
  file("", `${D}\\Electricity bill April 2025.pdf`, 0.4 * MB, "2025-04-27"),
  file("", `${D}\\Gas bill Q1 2025.pdf`, 0.3 * MB, "2025-04-12"),
  file("", `${D}\\Water bill 2025.pdf`, 0.2 * MB, "2025-05-03"),
  file("", `${D}\\Boarding pass FCO-LIS.pdf`, 0.2 * MB, "2025-06-10"),
  file("", `${D}\\Hotel booking Lisbon.pdf`, 0.5 * MB, "2025-06-02"),
  file("", `${D}\\Lisbon museums.pdf`, 3.1 * MB, "2025-06-05"),
  file("", `${D}\\IMG_2231.jpg`, 4.2 * MB, "2025-06-14"),
  file("", `${D}\\IMG_2232.jpg`, 3.9 * MB, "2025-06-14"),
  file("", `${D}\\IMG_2240.jpg`, 4.4 * MB, "2025-06-15"),
  file("", `${D}\\VID_20250615.mp4`, 310 * MB, "2025-06-15"),
  file("", `${D}\\Washing machine manual.pdf`, 8.2 * MB, "2024-11-20"),
  file("", `${D}\\Router setup guide.pdf`, 2.6 * MB, "2025-01-08"),
  file("", `${D}\\Quarterly report Q2.xlsx`, 1.1 * MB, "2025-07-02"),
  file("", `${D}\\Meeting notes July.docx`, 0.1 * MB, "2025-07-09"),
  file("", `${D}\\Contract draft v3.docx`, 0.2 * MB, "2025-07-15"),
  file("", `${D}\\SpotifyFullSetup.exe`, 610 * MB, "2024-06-02"),
  file("", `${D}\\vlc-3.0.20-win64.exe`, 42 * MB, "2023-11-10"),
  file("", `${D}\\Driver_pack_2022.zip`, 410 * MB, "2022-10-01"),
  file("", `${D}\\unnamed (4).png`, 0.6 * MB, "2025-02-11"),
  file("", `${D}\\doc12345.pdf`, 0.9 * MB, "2025-03-01"),
];

/** Setup files of programs the sample PC has installed. */
const SAMPLE_USED_INSTALLERS = /SpotifyFullSetup|vlc-/;

const tidyFile = (f: SampleFile): TidyFile => ({ path: f.path, sizeBytes: f.sizeBytes, modified: f.modified });

/** What the sample AI proposes for the loose downloads still there. */
export function sampleTidyPlan(present: SampleFile[], lang: string, byAi: boolean): TidyPlan {
  const it = lang === "it";
  const installers = present.filter((f) => SAMPLE_USED_INSTALLERS.test(f.path));
  const rest = present.filter((f) => !installers.includes(f));
  const groups: [string, string, TidyDest, string, string, RegExp][] = byAi
    ? [
        ["Bills 2025", "Bollette 2025", "documents", "Electricity, gas and water bills", "Bollette di luce, gas e acqua", /bill/i],
        ["Lisbon trip", "Viaggio a Lisbona", "documents", "Tickets, hotel and guides for Lisbon", "Biglietti, hotel e guide per Lisbona", /lis(bon|)|boarding/i],
        ["Lisbon photos", "Foto di Lisbona", "pictures", "Photos and a video from June 2025", "Foto e un video di giugno 2025", /(IMG_22|VID_2025)/],
        ["Manuals", "Manuali", "documents", "Instructions for home appliances", "Istruzioni degli elettrodomestici", /manual|guide/i],
        ["Work", "Lavoro", "documents", "Reports, notes and contracts", "Report, appunti e contratti", /report|meeting|contract/i],
      ]
    : [
        ["Documents", "Documenti", "downloads", "", "", /\.(pdf|docx|xlsx)$/i],
        ["Pictures", "Immagini", "downloads", "", "", /\.(jpg|png)$/i],
        ["Videos", "Video", "downloads", "", "", /\.mp4$/i],
        ["Archives", "Archivi compressi", "downloads", "", "", /\.zip$/i],
      ];
  const dirOf: Record<TidyDest, string> = { downloads: D, documents: `${U}\\Documents`, pictures: P, videos: `${U}\\Videos`, music: `${U}\\Music` };
  const taken = new Set<string>();
  const folders: TidyFolder[] = [];
  for (const [en, itName, dest, whyEn, whyIt, re] of groups) {
    const files = rest.filter((f) => !taken.has(f.path) && re.test(f.path.split("\\").pop() ?? ""));
    if (!files.length) continue;
    files.forEach((f) => taken.add(f.path));
    const name = it ? itName : en;
    folders.push({
      id: `f${folders.length + 1}`,
      name,
      dest,
      why: it ? whyIt : whyEn,
      target: `${dirOf[dest]}\\${name}`,
      exists: name === "Manuals" || name === "Manuali",
      files: files.map(tidyFile),
      sizeBytes: files.reduce((s, f) => s + f.sizeBytes, 0),
    });
  }
  return { folders, installers: installers.map(tidyFile), leftCount: rest.length - taken.size, notSentCount: 0, byAi };
}

/** The lines the sample would send to the AI. */
export function sampleTidyLines(present: SampleFile[]): string[] {
  const size = (b: number) => (b >= GB ? `${(b / GB).toFixed(1)} GB` : b >= MB ? `${Math.round(b / MB)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`);
  return present
    .filter((f) => !SAMPLE_USED_INSTALLERS.test(f.path))
    .sort((a, b) => b.modified.localeCompare(a.modified))
    .map((f, i) => `${i + 1}. ${f.path.split("\\").pop()} · ${size(f.sizeBytes)} · ${f.modified.slice(0, 7)}`);
}
