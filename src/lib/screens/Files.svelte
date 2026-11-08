<script lang="ts">
  import ExplainCard from "../components/ExplainCard.svelte";
  import ExplainTag from "../components/ExplainTag.svelte";
  import SafetyTag from "../components/SafetyTag.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import { DELETE_NOW_GROUPS, keepFileGroups, listFileGroups, removeFileGroups, restoreQuarantine, usesSampleData } from "../api";
  import { quarantine } from "../settings.svelte";
  import { fileCount, formatBytes, removedLines, restoreSummary } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import type { FileGroup, FileItem, MoveReport, ScanProgress } from "../types";

  /** How many items the details panel lists. */
  const SHOWN_ITEMS = 8;
  /** The order the scan returns groups in, used to put groups back after an undo. */
  const GROUP_ORDER = ["temp", "recycle-bin", "crash-reports", "thumbnails", "installers", "old-downloads", "duplicates", "large-files", "unknown-folders"];

  /** What the toast at the bottom says after a move or an undo. */
  type Notice =
    | { kind: "moved"; report: MoveReport; groups: FileGroup[] }
    | { kind: "info"; lines: string[] }
    | { kind: "error"; lines: string[] };

  let groups = $state<FileGroup[]>([]);
  let checked = $state<Set<string>>(new Set());
  let focusedId = $state<string | null>(null);
  /** The item whose "What is this?" explanation the side panel shows. */
  let explained = $state<FileItem | null>(null);
  let scanning = $state(true);
  let progress = $state<ScanProgress | null>(null);
  let error = $state<string | null>(null);
  let moving = $state(false);
  let undoing = $state(false);
  let notice = $state<Notice | null>(null);

  const focused = $derived(groups.find((g) => g.id === focusedId) ?? null);

  function load(rescan = false) {
    scanning = true;
    error = null;
    progress = null;
    listFileGroups({ rescan, onProgress: (p) => (progress = p) })
      .then((g) => {
        groups = g;
        // Pre-select only what is safe; anything else needs the user's eyes.
        checked = new Set(g.filter((x) => x.safety === "safe" && !x.reviewOnly).map((x) => x.id));
        if (!g.some((x) => x.id === focusedId)) focusedId = null;
      })
      .catch((e) => (error = String(e)))
      .finally(() => (scanning = false));
  }

  $effect(() => load());

  function focus(id: string) {
    focusedId = id;
    explained = null;
  }

  const sections = $derived([
    { title: "files.safeSection", items: groups.filter((g) => g.safety === "safe") },
    { title: "files.checkSection", items: groups.filter((g) => g.safety === "check") },
  ]);
  const selectedBytes = $derived(groups.filter((g) => checked.has(g.id)).reduce((s, g) => s + g.sizeBytes, 0));
  const selectionDeletes = $derived([...checked].some((id) => DELETE_NOW_GROUPS.includes(id)));
  const selectionMoves = $derived([...checked].some((id) => !DELETE_NOW_GROUPS.includes(id)));
  /** What the button does: "Delete", "Move to quarantine", or both ("Free up"). */
  function actionLabel(deletes: boolean, moves: boolean, size: number): string {
    if (deletes && moves) return t("files.freeUp", { size: formatBytes(size) });
    return deletes ? t("files.deleteNow", { size: formatBytes(size) }) : `${t("common.moveToQuarantine")} (${formatBytes(size)})`;
  }

  function toggle(id: string) {
    const next = new Set(checked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    checked = next;
  }

  const count = (n: number) => n.toLocaleString(locale());

  /**
   * Removes whole groups: temporary files and caches are deleted, the rest goes to
   * Quarantine, and the toast offers to undo that part.
   */
  async function remove(ids: string[]) {
    if (!ids.length || moving) return;
    moving = true;
    notice = null;
    try {
      const report = await removeFileGroups(ids);
      if (report.movedCount === 0 && report.deletedCount === 0) {
        notice = { kind: "info", lines: [t("files.nothingMoved")] };
        return;
      }
      const moved = groups.filter((g) => ids.includes(g.id));
      setGroups(groups.filter((g) => !ids.includes(g.id)));
      notice = { kind: "moved", report, groups: moved };
    } catch (e) {
      notice = { kind: "error", lines: [t("files.moveFailed", { error: String(e) })] };
    } finally {
      moving = false;
    }
  }

  async function undo(report: MoveReport, moved: FileGroup[]) {
    undoing = true;
    try {
      const restored = await restoreQuarantine(report.entryIds);
      // Deleted groups cannot come back; scanning again finds what is left of them.
      const back = [...groups, ...moved.filter((g) => !DELETE_NOW_GROUPS.includes(g.id))].sort((a, b) => GROUP_ORDER.indexOf(a.id) - GROUP_ORDER.indexOf(b.id));
      setGroups(back);
      const clean = restored.renamedCount === 0 && restored.failedCount === 0;
      notice = { kind: "info", lines: clean ? [t("files.undone")] : restoreSummary(restored) };
    } catch (e) {
      notice = { kind: "error", lines: [t("quarantine.failed", { error: String(e) })] };
    } finally {
      undoing = false;
    }
  }

  /** Updates the list here and the scan result kept for the other screens. */
  function setGroups(next: FileGroup[]) {
    groups = next;
    keepFileGroups(next);
    checked = new Set([...checked].filter((id) => next.some((g) => g.id === id)));
    if (!next.some((g) => g.id === focusedId)) focusedId = null;
    if (!focusedId) explained = null;
  }

  const nameOf = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const folderOf = (path: string) => path.slice(0, Math.max(path.length - nameOf(path).length - 1, 0));
  /** The last two parts of a path, which say the most in little room: "…\\Pictures\\photo.jpg". */
  const shortPath = (path: string) => {
    const parts = path.split(/[\\/]/);
    return parts.length > 2 ? `…\\${parts.slice(-2).join("\\")}` : path;
  };
  const dateOf = (iso: string) => new Date(`${iso}T00:00:00`).toLocaleDateString(locale(), { year: "numeric", month: "short", day: "numeric" });

  function itemNote(item: FileItem): string {
    if (item.duplicateOf) return t("files.copyOf", { path: shortPath(item.duplicateOf) });
    if (item.originalPath) return t("files.wasIn", { path: folderOf(item.originalPath) });
    // "C:\\Users\\you\\AppData\\Local" says less than "AppData\\Local" in a narrow panel.
    const folder = folderOf(item.path).replace(/^.*?[\\/](AppData[\\/])/i, "$1");
    return item.lastUsed ? `${folder} · ${t("files.lastUsed", { date: dateOf(item.lastUsed) })}` : folder;
  }

  /** Groups listed by folder or Recycle Bin entry (no dates) rather than file by file. */
  const byFolder = (g: FileGroup) => g.items.every((i) => i.lastUsed === null);
  const moreItems = (g: FileGroup) => Math.max((byFolder(g) ? g.items.length : g.itemCount) - SHOWN_ITEMS, 0);
  /** Unknown folders count folders; Recycle Bin entries count deleted items; the rest count files. */
  function countLabel(g: FileGroup): string {
    if (g.reviewOnly) return g.itemCount === 1 ? t("files.folderCountOne") : t("files.folderCount", { count: count(g.itemCount) });
    if (g.id === "recycle-bin") return g.items.length === 1 ? t("files.entryCountOne") : t("files.entryCount", { count: count(g.items.length) });
    return fileCount(g.itemCount);
  }
  /** Folders get a "What is this?" tag and open their explanation. */
  const explainable = (g: FileGroup, item: FileItem) => g.reviewOnly || (item.explainId !== null && byFolder(g) && !item.originalPath);
</script>

<div class="layout">
  <div class="col">
    <div class="head">
      <div>
        <h1>{t("files.title")}</h1>
        <p class="muted sub">{t("files.subtitle")}</p>
      </div>
      {#if !scanning}
        <button class="btn btn-ghost" onclick={() => load(true)}>↻ {t("files.rescan")}</button>
      {/if}
    </div>
    {#if usesSampleData()}<SampleBanner />{/if}

    {#if scanning}
      <div class="card scan" role="status" aria-live="polite">
        <div class="scan-title">{t("files.scanning")}</div>
        <div class="steps" aria-hidden="true">
          {#each Array(progress?.stepCount ?? 4) as _, i (i)}
            <i class:done={progress && i + 1 < progress.stepIndex} class:current={(progress?.stepIndex ?? 1) === i + 1}></i>
          {/each}
        </div>
        {#if progress}
          <div class="scan-row">
            <b>{t("files.stepOf", { n: progress.stepIndex, total: progress.stepCount })} · {t(`files.steps.${progress.step}`)}</b>
            <span class="muted">{t("files.checked", { count: count(progress.filesChecked) })}</span>
          </div>
        {/if}
        <p class="muted note">🛡️ {t("files.scanNote")}</p>
      </div>
    {:else if error}
      <div class="card message">
        <p>{t("files.failed", { error })}</p>
        <button class="btn btn-primary" onclick={() => load(true)}>{t("files.retry")}</button>
      </div>
    {:else if groups.length === 0}
      <div class="card message"><span class="big-icon" aria-hidden="true">✨</span><p>{t("files.empty")}</p></div>
    {:else}
      {#each sections as section (section.title)}
        {#if section.items.length}
          <h2>{t(section.title)}</h2>
          {#each section.items as group (group.id)}
            <div class="card group" class:focused={focusedId === group.id} role="button" tabindex="0"
                 onclick={() => focus(group.id)} onkeydown={(e) => e.key === "Enter" && focus(group.id)}>
              {#if group.reviewOnly}
                <span class="no-check" aria-hidden="true">🔎</span>
              {:else}
                <input type="checkbox" checked={checked.has(group.id)} onclick={(e) => e.stopPropagation()} onchange={() => toggle(group.id)} aria-label={t(group.titleKey)} />
              {/if}
              <div class="info">
                <div class="title">{t(group.titleKey)}</div>
                <div class="muted desc">{t(group.descriptionKey)} · {countLabel(group)}</div>
              </div>
              <SafetyTag safety={group.safety} />
              <span class="size">{formatBytes(group.sizeBytes)}</span>
            </div>
          {/each}
        {/if}
      {/each}

      <div class="footer">
        <b>{t("files.selected", { size: formatBytes(selectedBytes) })}</b>
        <span class="grow"></span>
        <button class="btn btn-primary" disabled={selectedBytes === 0 || moving} onclick={() => remove([...checked])}>
          {moving ? t("files.moving") : actionLabel(selectionDeletes, selectionMoves, selectedBytes)}
        </button>
      </div>
      {#if selectionDeletes}
        <p class="muted how">{t("files.howRemoved", { days: quarantine.keepDays })}</p>
      {/if}
    {/if}
  </div>

  <aside class="card side">
    {#if focused && explained}
      <ExplainCard item={explained} groupTitle={t(focused.titleKey)} onBack={() => (explained = null)} />
    {:else if focused}
      <div class="label">{t("explain.title")}</div>
      <h3>{t(focused.titleKey)}</h3>
      <p class="muted desc">{t(focused.descriptionKey)}</p>
      <h4>{t("explain.what")}</h4>
      <p class="about">{t(focused.titleKey.replace(/title$/, "what"))}</p>
      <h4>{t("explain.ifRemovedGroup")}</h4>
      <p class="about">{t(focused.titleKey.replace(/title$/, "ifRemoved"))}</p>
      <dl>
        <dt>{t("files.size")}</dt><dd>{formatBytes(focused.sizeBytes)}</dd>
        {#if !focused.reviewOnly}<dt>{t("files.files")}</dt><dd>{count(focused.itemCount)}</dd>{/if}
      </dl>
      <div class="label list-label">{t("files.largest")}</div>
      <ul class="items">
        {#each focused.items.slice(0, SHOWN_ITEMS) as item (item.path)}
          {@const canExplain = explainable(focused, item)}
          <li title={item.duplicateOf ? `${item.path}\n${t("files.copyOf", { path: item.duplicateOf })}` : (item.originalPath ?? item.path)}>
            <svelte:element this={canExplain ? "button" : "div"} class="item-row" class:clickable={canExplain}
              onclick={canExplain ? () => (explained = item) : undefined} role={canExplain ? undefined : "presentation"}>
              <div class="item-text">
                <div class="item-name">{nameOf(item.originalPath ?? item.path)}</div>
                <div class="muted item-note">{itemNote(item)}</div>
              </div>
              {#if canExplain}<ExplainTag explainId={item.explainId} />{/if}
              <span class="item-size">{formatBytes(item.sizeBytes)}</span>
            </svelte:element>
          </li>
        {/each}
      </ul>
      {#if moreItems(focused) > 0}
        <div class="muted more">{t("files.more", { count: count(moreItems(focused)) })}</div>
      {/if}
      <div class="grow"></div>
      {#if focused.reviewOnly}
        <p class="muted review-note">🔎 {t("files.reviewOnly")}</p>
      {:else}
        {@const id = focused.id}
        {@const deletes = DELETE_NOW_GROUPS.includes(id)}
        {#if deletes}<p class="muted how">{t("files.deletedRightAway")}</p>{/if}
        <button class="btn btn-primary wide" disabled={moving} onclick={() => remove([id])}>
          {moving ? t("files.moving") : actionLabel(deletes, !deletes, focused.sizeBytes)}
        </button>
      {/if}
    {:else}
      <p class="muted">{t("files.pickGroup")}</p>
    {/if}
  </aside>
</div>

{#if notice}
  {@const n = notice}
  <div class="card toast" class:error={n.kind === "error"} role="status" aria-live="polite">
    <span class="toast-icon" aria-hidden="true">{n.kind === "error" ? "⚠️" : "✅"}</span>
    <div class="toast-text">
      {#each n.kind === "moved" ? removedLines(n.report, quarantine.keepDays) : n.lines as line, i (i)}<p>{line}</p>{/each}
    </div>
    {#if n.kind === "moved" && n.report.entryIds.length}
      <button class="btn btn-ghost" disabled={undoing} onclick={() => undo(n.report, n.groups)}>
        {undoing ? t("files.undoing") : `↩ ${t("files.undo")}`}
      </button>
    {/if}
    <button class="close" aria-label={t("common.close")} onclick={() => (notice = null)}>✕</button>
  </div>
{/if}

<style>
  .layout { display: flex; gap: 18px; align-items: flex-start; }
  .col { flex: 1; min-width: 0; }
  .head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .head .btn { flex: none; margin-top: 4px; }
  .sub { margin: 3px 0 10px; }
  h2 { font-size: 12px; font-weight: 600; color: var(--text-muted); letter-spacing: 0.3px; text-transform: uppercase; margin: 14px 0 6px; }
  .group { display: flex; align-items: center; gap: 12px; padding: 11px 14px; margin-bottom: 7px; border-width: 2px; border-color: transparent; cursor: pointer; }
  .group.focused { border-color: var(--accent); }
  .group input { width: 18px; height: 18px; accent-color: var(--accent); }
  .no-check { width: 18px; text-align: center; font-size: 14px; }
  .info { min-width: 0; flex: 1; }
  .title { font-size: 14px; font-weight: 600; }
  .desc { font-size: 12px; }
  .size { font-size: 14px; font-weight: 600; width: 76px; text-align: right; }
  .footer { display: flex; align-items: center; gap: 10px; margin-top: 14px; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .grow { flex: 1; }
  .how { font-size: 12px; margin-top: 8px; text-align: right; }
  .side .how { text-align: left; margin-top: 12px; }

  .scan { padding: 22px 24px; margin-top: 8px; }
  .scan-title { font-size: 17px; font-weight: 600; }
  .steps { display: flex; gap: 6px; margin: 14px 0 10px; }
  .steps i { flex: 1; height: 10px; border-radius: 5px; background: var(--border); }
  .steps i.done { background: var(--accent); }
  .steps i.current { background: var(--accent-soft); position: relative; overflow: hidden; }
  .steps i.current::after { content: ""; position: absolute; inset: 0; width: 40%; border-radius: 5px; background: var(--accent); animation: sweep 1.2s ease-in-out infinite; }
  @keyframes sweep { from { transform: translateX(-100%); } to { transform: translateX(250%); } }
  .scan-row { display: flex; justify-content: space-between; gap: 12px; font-size: 13px; flex-wrap: wrap; }
  .note { font-size: 12.5px; margin-top: 14px; }
  .message { padding: 22px 24px; margin-top: 8px; display: flex; align-items: center; gap: 14px; }
  .message p { flex: 1; }
  .big-icon { font-size: 26px; }

  .side { width: 330px; flex: none; padding: 18px; min-height: 420px; display: flex; flex-direction: column; position: sticky; top: 0; }
  .label { font-size: 11px; font-weight: 600; letter-spacing: 0.3px; color: var(--text-muted); text-transform: uppercase; }
  .side h3 { font-size: 17px; margin: 6px 0 4px; }
  .side h4 { font-size: 12.5px; margin-top: 10px; }
  .about { font-size: 12.5px; line-height: 1.45; margin-top: 2px; }
  .review-note { font-size: 12.5px; margin-top: 14px; }
  dl { display: grid; grid-template-columns: 1fr auto; row-gap: 3px; font-size: 12.5px; margin-top: 10px; border-top: 1px solid var(--border); padding-top: 8px; }
  dt { color: var(--text-muted); }
  dd { font-weight: 600; }
  .list-label { margin-top: 14px; }
  .items { list-style: none; margin-top: 6px; }
  .items li { border-bottom: 1px solid var(--border); }
  .item-row { display: flex; align-items: center; gap: 8px; padding: 6px 0; width: 100%; border: none; background: none; color: inherit; font: inherit; text-align: left; }
  .item-row.clickable { cursor: pointer; padding: 6px 6px; margin: 0 -6px; width: calc(100% + 12px); border-radius: var(--radius-sm); }
  .item-row.clickable:hover { background: var(--hover); }
  .item-text { flex: 1; min-width: 0; }
  .item-name, .item-note { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .item-name { font-size: 12.5px; font-weight: 600; }
  .item-note { font-size: 11px; }
  .item-size { font-size: 12px; font-weight: 600; flex: none; }
  .more { font-size: 12px; margin-top: 6px; }
  .wide { width: 100%; padding: 10px; margin-top: 14px; }

  .toast { position: fixed; right: 30px; bottom: 22px; max-width: 560px; display: flex; align-items: center; gap: 12px; padding: 12px 14px 12px 16px; box-shadow: var(--shadow-pop); border-left: 4px solid var(--safe-fg); z-index: 10; }
  .toast.error { border-left-color: var(--danger); }
  .toast-icon { font-size: 18px; }
  .toast-text { flex: 1; font-size: 13px; line-height: 1.4; }
  .toast-text p + p { color: var(--text-muted); margin-top: 2px; }
  .toast .btn { flex: none; }
  .close { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 4px; }
</style>
