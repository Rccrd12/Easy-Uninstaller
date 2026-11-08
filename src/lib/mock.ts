// Sample data used until each scanner is implemented in Rust.
// Every function in api.ts that still returns from here is marked "sample".
import { GB, MB } from "./format";
import type { AiAnswer, AiModel, AiServiceStatus, FileGroup, FileItem, FolderSummary, InstalledApp, LeftoverItem, QuarantineEntry, SpaceFolder, SpaceUsage } from "./types";

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
  reviewOnly: id === "unknown-folders",
});
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

export const sampleFileGroups: FileGroup[] = [
  fileGroup("temp", "temp", "safe", 4821, [
    folderItem(`${U}\\AppData\\Local\\Temp`, 1.6 * GB, "temp"),
    folderItem(`${U}\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache`, 820 * MB, "browserCache"),
    folderItem("C:\\Windows\\Temp", 410 * MB, "temp"),
    folderItem(`${U}\\AppData\\Local\\NVIDIA\\DXCache`, 270 * MB, "shaderCache"),
  ]),
  fileGroup("recycle-bin", "recycleBin", "safe", 14, [
    binItem("K2J9QX.zip", `${U}\\Downloads\\Old project`, 840 * MB),
    binItem("8DF1LA.mp4", `${U}\\Videos\\Screen recording 2025-03-02.mp4`, 310 * MB),
    binItem("Q7ZP02.psd", `${U}\\Desktop\\poster draft.psd`, 95 * MB),
  ]),
  fileGroup("crash-reports", "crashReports", "safe", 9, [
    { ...item(`${U}\\AppData\\Local\\CrashDumps\\GameLauncher.exe.8812.dmp`, 410 * MB, "2025-06-14"), explainId: "crashDumps" },
    { ...item(`${U}\\AppData\\Local\\CrashDumps\\explorer.exe.4410.dmp`, 160 * MB, "2025-04-02"), explainId: "crashDumps" },
    { ...item(`${U}\\AppData\\Local\\Microsoft\\Windows\\WER\\ReportArchive\\AppCrash_Spotify\\Report.wer`, 70 * MB, "2025-02-20"), explainId: "crashDumps" },
  ]),
  fileGroup("thumbnails", "thumbnails", "safe", 6, [
    { ...item(`${U}\\AppData\\Local\\Microsoft\\Windows\\Explorer\\thumbcache_256.db`, 380 * MB, "2025-07-01"), explainId: "thumbCache" },
    { ...item(`${U}\\AppData\\Local\\Microsoft\\Windows\\Explorer\\thumbcache_1280.db`, 210 * MB, "2025-07-01"), explainId: "thumbCache" },
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
  fileGroup("unknown-folders", "unknownFolders", "check", 13, [
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
  ]),
];

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

// ── AI help ───────────────────────────────────────────────────────────────

export const sampleAiServices: AiServiceStatus[] = [
  { id: "claude-code", found: true, models: null, hasKey: false },
  { id: "codex", found: true, models: null, hasKey: false },
  { id: "antigravity-cli", found: false, models: null, hasKey: false },
  { id: "anthropic", found: true, models: null, hasKey: false },
  { id: "openai", found: true, models: null, hasKey: false },
  { id: "google", found: true, models: null, hasKey: false },
  { id: "openrouter", found: true, models: null, hasKey: false },
  { id: "custom", found: true, models: null, hasKey: false },
  { id: "ollama", found: true, models: 3, hasKey: false },
  { id: "lmstudio", found: true, models: 2, hasKey: false },
  { id: "unsloth", found: true, models: null, hasKey: false },
  { id: "local-custom", found: false, models: null, hasKey: false },
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
