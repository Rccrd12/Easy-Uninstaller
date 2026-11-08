import { locale, t } from "./i18n/index.svelte";
import type { RestoreReport } from "./types";

const units = ["B", "KB", "MB", "GB", "TB"];

/** Formats a byte count the way Windows Explorer does (1024-based, one decimal). */
export function formatBytes(bytes: number, loc: string = locale()): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = value >= 100 || unit === 0 ? 0 : 1;
  return `${value.toLocaleString(loc, { maximumFractionDigits: digits })} ${units[unit]}`;
}

/** Formats an ISO date (YYYY-MM-DD) as a short local date, e.g. "2 Jun 2024". */
export function formatDate(iso: string, loc: string = locale()): string {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(loc, { day: "numeric", month: "short", year: "numeric" });
}

export const GB = 1024 ** 3;
export const MB = 1024 ** 2;

/** "1 file", "4,821 files", in the chosen language. */
export function fileCount(n: number): string {
  return n === 1 ? t("files.fileCountOne") : t("files.fileCount", { count: n.toLocaleString(locale()) });
}

/** "2 days ago", "in 5 days", "now", in the chosen language. `unixSecs` is in seconds. */
export function relativeTime(unixSecs: number, nowMs: number = Date.now()): string {
  const rtf = new Intl.RelativeTimeFormat(locale(), { numeric: "auto" });
  const diff = unixSecs - nowMs / 1000;
  const abs = Math.abs(diff);
  if (abs < 60) return rtf.format(0, "second");
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour");
  return rtf.format(Math.round(diff / 86400), "day");
}

/** What happened when files were put back, one sentence per case that applies. */
export function restoreSummary(r: RestoreReport): string[] {
  const lines = [t("quarantine.restored", { count: fileCount(r.restoredCount) })];
  if (r.renamedCount) lines.push(t("quarantine.renamed", { count: fileCount(r.renamedCount) }));
  if (r.failedCount) lines.push(t("quarantine.restoreFailed", { count: fileCount(r.failedCount) }));
  return lines;
}

/** "1 item", "12 items": for quarantine entries that hold folders or registry keys. */
export function itemCount(n: number): string {
  return n === 1 ? t("common.itemsOne") : t("common.items", { count: n.toLocaleString(locale()) });
}

/** "1 leftover", "5 leftovers", in the chosen language. */
export function leftoverCount(n: number): string {
  return n === 1 ? t("leftovers.countOne") : t("leftovers.count", { count: n.toLocaleString(locale()) });
}
