// Small helpers to show Windows paths the way people know them.
import { t } from "./i18n/index.svelte";

/** "C:\\Users\\you\\Pictures\\a.jpg" -> "a.jpg". */
export const nameOf = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** "C:\\Users\\you\\Pictures\\a.jpg" -> "C:\\Users\\you\\Pictures". */
export const folderOf = (path: string) => path.slice(0, Math.max(path.length - nameOf(path).length - 1, 0));

/** The folders of the user profile that Windows shows with a translated name. */
const KNOWN = ["desktop", "documents", "downloads", "music", "pictures", "videos"];
const PROFILE = /^[a-z]:\\users\\[^\\]+\\/i;

function localize(folder: string): string {
  const key = folder.toLowerCase();
  return KNOWN.includes(key) ? t(`files.review.folders.${key}`) : folder;
}

/**
 * A path as short as it can be while still clear: inside the user profile it starts
 * at the profile folder, with the name Explorer shows ("Immagini\\Vacanze 2024").
 */
export function displayPath(path: string): string {
  const m = PROFILE.exec(path);
  if (!m) return path;
  const [first, ...rest] = path.slice(m[0].length).split("\\");
  return [localize(first), ...rest].join("\\");
}

/** The top folder of the user profile a file is in ("Pictures"), or "" when outside it. */
export function topFolder(path: string): string {
  const m = PROFILE.exec(path);
  if (!m) return "";
  const rest = path.slice(m[0].length).split("\\");
  return rest.length > 1 ? rest[0] : "";
}

/** The name to show for a top folder of the profile. */
export const topFolderName = (folder: string) => localize(folder);

export type FileKind = "image" | "video" | "audio" | "document" | "archive" | "program" | "disc" | "other";

const KINDS: [FileKind, string[]][] = [
  ["image", ["jpg", "jpeg", "png", "gif", "bmp", "webp", "heic", "heif", "tif", "tiff", "dng", "cr2", "nef", "arw", "psd", "svg"]],
  ["video", ["mp4", "mov", "avi", "mkv", "wmv", "m4v", "webm", "3gp", "mts"]],
  ["audio", ["mp3", "wav", "flac", "m4a", "aac", "ogg", "wma"]],
  ["document", ["pdf", "txt", "md", "rtf", "csv", "log", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "epub"]],
  ["archive", ["zip", "7z", "rar", "gz", "tar"]],
  ["program", ["exe", "msi", "msix", "msixbundle", "appx", "bat", "cmd"]],
  ["disc", ["iso", "img", "vdi", "vhd", "vhdx", "vmdk"]],
];

export function fileKind(path: string): FileKind {
  const ext = nameOf(path).split(".").pop()?.toLowerCase() ?? "";
  return KINDS.find(([, exts]) => exts.includes(ext))?.[0] ?? "other";
}
