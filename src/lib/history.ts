// How a history row reads: shared by the History screen and the Home card.
import { fileCount, formatBytes, itemCount } from "./format";
import type { IconName } from "./icons";
import { locale, t } from "./i18n/index.svelte";
import type { HistoryItem, HistoryKind, HistoryName } from "./types";

export const kindIcons: Record<HistoryKind, IconName> = {
  quickClean: "sparkle",
  files: "folder",
  leftovers: "broom",
  appFolder: "folderOpen",
  uninstall: "apps",
};

const nameOf = (n: HistoryName) => ("key" in n ? t(n.key) : n.text);

/** "Discord", "Discord, Zoom", "Discord, Zoom and 2 more". */
export function joinNames(names: HistoryName[], shown = 2): string {
  // "Leftovers" says nothing next to "Leftovers cleaned up".
  const list = names.filter((n) => !("key" in n && n.key === "leftovers.title")).map(nameOf);
  const head = list.slice(0, shown).join(", ");
  return list.length > shown ? t("history.moreNames", { names: head, count: list.length - shown }) : head;
}

/** The row title: "Quick clean", "Uninstalled Discord", "Removed: Old downloads". */
export function historyTitle(item: HistoryItem): string {
  if (item.kind === "quickClean") return t("history.kind.quickClean");
  const names = joinNames(item.names);
  if (!names && (item.kind === "files" || item.kind === "leftovers")) return t(`history.kind.${item.kind}NoName`);
  return t(`history.kind.${item.kind}`, { names });
}

/** The line under the title: what was removed and how much of it. */
export function historyDetail(item: HistoryItem): string {
  switch (item.kind) {
    case "uninstall":
      return t("history.uninstallDetail");
    case "quickClean":
      return [joinNames(item.names, 3), fileCount(item.itemCount)].filter(Boolean).join(" · ");
    case "files":
      return fileCount(item.itemCount);
    default:
      return itemCount(item.itemCount);
  }
}

/** "3 Nov", or "3 Nov 2025" for another year. */
export function shortDate(unixSecs: number): string {
  const d = new Date(unixSecs * 1000);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString(locale(), { day: "numeric", month: "short", ...(sameYear ? {} : { year: "numeric" }) });
}

/** "Today", "Yesterday", or a short date. */
export function dayLabel(unixSecs: number, now = new Date()): string {
  const d = new Date(unixSecs * 1000);
  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (d.getTime() >= midnight) return t("history.today");
  if (d.getTime() >= midnight - 86_400_000) return t("history.yesterday");
  return shortDate(unixSecs);
}

/** "14:05" in the chosen language's style. */
export const timeOf = (unixSecs: number) => new Date(unixSecs * 1000).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });

/** "November 2026", capitalized also in languages that write months in lower case. */
export function monthLabel(unixSecs: number): string {
  const s = new Date(unixSecs * 1000).toLocaleDateString(locale(), { month: "long", year: "numeric" });
  return s.charAt(0).toLocaleUpperCase(locale()) + s.slice(1);
}

/** What the action removed and did not put back: freed now, or freed when Quarantine ends. */
export function historyAmount(item: HistoryItem): string {
  const bytes = item.freedBytes + item.pendingBytes;
  if (!bytes) return "—";
  return item.estimated ? t("history.about", { size: formatBytes(bytes) }) : formatBytes(bytes);
}
