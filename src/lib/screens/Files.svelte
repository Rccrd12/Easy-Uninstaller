<script lang="ts">
  import ExplainCard from "../components/ExplainCard.svelte";
  import ExplainTag from "../components/ExplainTag.svelte";
  import StatusIcon from "../components/StatusIcon.svelte";
  import SafetyTag from "../components/SafetyTag.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import { keepFileGroups, listFileGroups, QUARANTINE_DAYS, quarantineAppFolder, quarantineFileGroups, restoreQuarantine, usesSampleData } from "../api";
  import { fileCount, formatBytes, restoreSummary } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import type { FileGroup, FileItem, MoveReport, ScanProgress } from "../types";

  /** How many items the details panel lists. */
  const SHOWN_ITEMS = 8;
  /** The order the scan returns groups in, used to put groups back after an undo. */
  const GROUP_ORDER = ["temp", "recycle-bin", "crash-reports", "thumbnails", "installers", "old-downloads", "duplicates", "large-files", "unknown-folders"];

  /** What the toast at the bottom says after a move or an undo. */
  type Notice =
    | { kind: "moved"; report: MoveReport; groups: FileGroup[] }
    | { kind: "folderMoved"; report: MoveReport; group: FileGroup; item: FileItem }
    | { kind: "info"; lines: string[] }
    | { kind: "error"; lines: string[] };

  let groups = $state<FileGroup[]>([]);
  let checked = $state<Set<string>>(new Set());
  let focusedId = $state<string | null>(null);
  /** The item whose "What is this?" explanation the side panel shows. */
  let explained = $state<FileItem | null>(null);
  /** Whether the side panel lists every item of the group, not just the largest. */
  let showAll = $state(false);
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
    if (focusedId !== id) showAll = false;
    focusedId = id;
    explained = null;
  }

  const sections = $derived([
    { title: "files.safeSection", items: groups.filter((g) => g.safety === "safe") },
    { title: "files.checkSection", items: groups.filter((g) => g.safety === "check") },
  ]);
  const selectedBytes = $derived(groups.filter((g) => checked.has(g.id)).reduce((s, g) => s + g.sizeBytes, 0));

  function toggle(id: string) {
    const next = new Set(checked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    checked = next;
  }

  const count = (n: number) => n.toLocaleString(locale());

  /** Moves whole groups into quarantine. Nothing is deleted: the toast offers to undo. */
  async function moveToQuarantine(ids: string[]) {
    if (!ids.length || moving) return;
    moving = true;
    notice = null;
    try {
      const report = await quarantineFileGroups(ids);
      if (report.movedCount === 0) {
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
      const back = [...groups, ...moved].sort((a, b) => GROUP_ORDER.indexOf(a.id) - GROUP_ORDER.indexOf(b.id));
      setGroups(back);
      const clean = restored.renamedCount === 0 && restored.failedCount === 0;
      notice = { kind: "info", lines: clean ? [t("files.undone")] : restoreSummary(restored) };
    } catch (e) {
      notice = { kind: "error", lines: [t("quarantine.failed", { error: String(e) })] };
    } finally {
      undoing = false;
    }
  }

  /** Moves one folder of a review-only group (unknown folders) into quarantine. */
  async function moveFolder(group: FileGroup, item: FileItem) {
    if (moving) return;
    moving = true;
    notice = null;
    const name = nameOf(item.path);
    try {
      const report = await quarantineAppFolder(item);
      if (report.movedCount === 0) {
        const key = report.declinedCount ? "files.folderDeclined" : "files.folderSkipped";
        notice = { kind: "info", lines: [t(key, { name })] };
        return;
      }
      setGroups(groups.flatMap((g) => (g.id === group.id ? withoutItem(g, item) : [g])));
      explained = null;
      notice = { kind: "folderMoved", report, group, item };
    } catch (e) {
      notice = { kind: "error", lines: [String(e) === "declined" ? t("files.folderDeclined", { name }) : t("files.moveFailed", { error: String(e) })] };
    } finally {
      moving = false;
    }
  }

  /** The group without `item`, or nothing when it was the last one. */
  function withoutItem(g: FileGroup, item: FileItem): FileGroup[] {
    const items = g.items.filter((i) => i.path !== item.path);
    if (!items.length) return [];
    return [{ ...g, items, itemCount: g.itemCount - 1, sizeBytes: g.sizeBytes - item.sizeBytes }];
  }

  async function undoFolder(report: MoveReport, group: FileGroup, item: FileItem) {
    undoing = true;
    try {
      const restored = await restoreQuarantine(report.entryIds);
      const current = groups.find((g) => g.id === group.id);
      const back: FileGroup = current
        ? { ...current, items: [...current.items, item].sort((a, b) => b.sizeBytes - a.sizeBytes), itemCount: current.itemCount + 1, sizeBytes: current.sizeBytes + item.sizeBytes }
        : { ...group, items: [item], itemCount: 1, sizeBytes: item.sizeBytes };
      setGroups([...groups.filter((g) => g.id !== group.id), back].sort((a, b) => GROUP_ORDER.indexOf(a.id) - GROUP_ORDER.indexOf(b.id)));
      const clean = restored.renamedCount === 0 && restored.failedCount === 0;
      notice = { kind: "info", lines: clean ? [t("files.undone")] : restoreSummary(restored) };
    } catch (e) {
      notice = { kind: "error", lines: [String(e) === "declined" ? t("quarantine.declined") : t("quarantine.failed", { error: String(e) })] };
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

  function movedLines(r: MoveReport): string[] {
    const lines = [t("files.moved", { count: fileCount(r.movedCount), size: formatBytes(r.movedBytes), days: QUARANTINE_DAYS })];
    if (r.skippedCount) lines.push(t("files.movedSkipped", { count: fileCount(r.skippedCount) }));
    return lines;
  }
  function folderMovedLines(r: MoveReport, item: FileItem): string[] {
    return [t("files.folderMoved", { name: nameOf(item.path), size: formatBytes(r.movedBytes), days: QUARANTINE_DAYS })];
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
  /** Items the scan found but did not send (it sends the largest 200). */
  const notListed = (g: FileGroup) => (byFolder(g) ? 0 : Math.max(g.itemCount - g.items.length, 0));
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
        <button class="btn btn-primary" disabled={selectedBytes === 0 || moving} onclick={() => moveToQuarantine([...checked])}>
          {moving ? t("files.moving") : t("common.moveToQuarantine")}
        </button>
      </div>
    {/if}
  </div>

  <aside class="card side">
    {#if focused && explained}
      {@const group = focused}
      {@const item = explained}
      {#key item.path}
        <ExplainCard {item} groupTitle={t(group.titleKey)} onBack={() => (explained = null)}
          onMove={group.reviewOnly ? () => moveFolder(group, item) : undefined} {moving} />
      {/key}
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
      <ul class="items" class:all={showAll}>
        {#each showAll ? focused.items : focused.items.slice(0, SHOWN_ITEMS) as item (item.path)}
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
      {#if focused.items.length > SHOWN_ITEMS}
        <button class="show-all" onclick={() => (showAll = !showAll)}>
          {showAll ? t("files.showLess") : t("files.showAll", { count: count(focused.items.length) })}
        </button>
      {/if}
      {#if notListed(focused) > 0 && (showAll || focused.items.length <= SHOWN_ITEMS)}
        <div class="muted more">{t("files.notListed", { count: count(notListed(focused)) })}</div>
      {/if}
      <div class="grow"></div>
      {#if focused.reviewOnly}
        <p class="muted review-note">🔎 {t("files.reviewOnly")}</p>
      {:else}
        {@const id = focused.id}
        <button class="btn btn-primary wide" disabled={moving} onclick={() => moveToQuarantine([id])}>
          {moving ? t("files.moving") : `${t("common.moveToQuarantine")} (${formatBytes(focused.sizeBytes)})`}
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
    <StatusIcon kind={n.kind === "error" ? "warn" : "ok"} />
    <div class="toast-text">
      {#each n.kind === "moved" ? movedLines(n.report) : n.kind === "folderMoved" ? folderMovedLines(n.report, n.item) : n.lines as line, i (i)}<p>{line}</p>{/each}
    </div>
    {#if n.kind === "moved" || n.kind === "folderMoved"}
      <button class="btn btn-ghost" disabled={undoing} onclick={() => (n.kind === "moved" ? undo(n.report, n.groups) : undoFolder(n.report, n.group, n.item))}>
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
  .items.all { max-height: 46vh; overflow-y: auto; padding-right: 6px; }
  .show-all { border: none; background: none; color: var(--accent); font-size: 12.5px; padding: 6px 0 0; text-align: left; cursor: pointer; }
  .wide { width: 100%; padding: 10px; margin-top: 14px; }

  .toast { position: fixed; right: 30px; bottom: 22px; max-width: 560px; display: flex; align-items: center; gap: 12px; padding: 12px 14px 12px 16px; box-shadow: var(--shadow-pop); border-left: 4px solid var(--safe-fg); z-index: 10; }
  .toast.error { border-left-color: var(--danger); }
  .toast-text { flex: 1; font-size: 13px; line-height: 1.4; }
  .toast-text p + p { color: var(--text-muted); margin-top: 2px; }
  .toast .btn { flex: none; }
  .close { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 4px; }
</style>
