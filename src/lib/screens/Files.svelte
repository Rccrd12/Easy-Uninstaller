<script lang="ts">
  import FileReview from "../components/FileReview.svelte";
  import SafetyTag from "../components/SafetyTag.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import {
    addExclusion,
    keepFileGroups,
    listFileGroups,
    QUARANTINE_DAYS,
    quarantineFileGroups,
    quarantineFileSelection,
    refreshFileGroups,
    restoreQuarantine,
    usesSampleData,
  } from "../api";
  import { fileCount, formatBytes, restoreSummary } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import { displayPath, folderOf, nameOf } from "../paths";
  import type { Exclusion, FileGroup, FileItem, FileSelection, MoveReport, ScanProgress } from "../types";

  /** How many items the details panel lists. */
  const SHOWN_ITEMS = 8;

  /** What the toast at the bottom says after a move or an undo. */
  type Notice =
    | { kind: "moved"; report: MoveReport }
    | { kind: "info"; lines: string[] }
    | { kind: "error"; lines: string[] };

  let groups = $state<FileGroup[]>([]);
  let checked = $state<Set<string>>(new Set());
  let focusedId = $state<string | null>(null);
  let scanning = $state(true);
  let progress = $state<ScanProgress | null>(null);
  let error = $state<string | null>(null);
  let moving = $state(false);
  let undoing = $state(false);
  let notice = $state<Notice | null>(null);
  /** The group being reviewed file by file, kept even when all its files are gone. */
  let reviewing = $state<FileGroup | null>(null);
  /** Changes after every move, undo or hidden file, so the review reads its files again. */
  let version = $state(0);

  const focused = $derived(groups.find((g) => g.id === focusedId) ?? null);

  /** Groups of single files can be reviewed one by one; folder groups (temp) cannot. */
  const reviewable = (g: FileGroup) => g.items.length > 0 && g.items.every((i) => i.lastUsed !== null);

  function openReview(g: FileGroup) {
    reviewing = g;
    notice = null;
  }

  function load(rescan = false) {
    scanning = true;
    error = null;
    progress = null;
    listFileGroups({ rescan, onProgress: (p) => (progress = p) })
      .then((g) => {
        groups = g;
        // Pre-select only what is safe; anything else needs the user's eyes.
        checked = new Set(g.filter((x) => x.safety === "safe").map((x) => x.id));
        if (!g.some((x) => x.id === focusedId)) focusedId = null;
      })
      .catch((e) => (error = String(e)))
      .finally(() => (scanning = false));
  }

  $effect(() => load());

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
      await afterMove(report);
    } catch (e) {
      notice = { kind: "error", lines: [t("files.moveFailed", { error: String(e) })] };
    } finally {
      moving = false;
    }
  }

  /** Moves the files picked in the review. */
  async function moveSelection(selection: FileSelection) {
    if (moving) return;
    moving = true;
    notice = null;
    try {
      await afterMove(await quarantineFileSelection([selection]));
    } catch (e) {
      notice = { kind: "error", lines: [t("files.moveFailed", { error: String(e) })] };
    } finally {
      moving = false;
    }
  }

  async function afterMove(report: MoveReport) {
    if (report.movedCount === 0) {
      notice = { kind: "info", lines: [t("files.nothingMoved")] };
      return;
    }
    notice = { kind: "moved", report };
    await refresh();
  }

  /** Reads the groups again from the last scan: what was moved, put back or hidden shows up. */
  async function refresh() {
    try {
      setGroups(await refreshFileGroups());
    } finally {
      version++;
    }
  }

  /** "Don't show again": nothing is moved, the file or folder is only left out from now on. */
  async function hide(exclusion: Exclusion) {
    try {
      await addExclusion(exclusion);
      const name = exclusion.folder ? displayPath(exclusion.path) : nameOf(exclusion.path);
      notice = { kind: "info", lines: [t("files.review.hidden", { name }), t("files.review.hiddenHint")] };
      await refresh();
    } catch (e) {
      notice = { kind: "error", lines: [String(e)] };
    }
  }

  async function undo(report: MoveReport) {
    undoing = true;
    try {
      const restored = await restoreQuarantine(report.entryIds);
      await refresh();
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
    if (reviewing) reviewing = next.find((g) => g.id === reviewing?.id) ?? reviewing;
    checked = new Set([...checked].filter((id) => next.some((g) => g.id === id)));
    if (!next.some((g) => g.id === focusedId)) focusedId = null;
  }

  function movedLines(r: MoveReport): string[] {
    const lines = [t("files.moved", { count: fileCount(r.movedCount), size: formatBytes(r.movedBytes), days: QUARANTINE_DAYS })];
    if (r.skippedCount) lines.push(t("files.movedSkipped", { count: fileCount(r.skippedCount) }));
    return lines;
  }
  /** The last two parts of a path, which say the most in little room: "…\\Pictures\\photo.jpg". */
  const shortPath = (path: string) => {
    const parts = path.split(/[\\/]/);
    return parts.length > 2 ? `…\\${parts.slice(-2).join("\\")}` : path;
  };
  const dateOf = (iso: string) => new Date(`${iso}T00:00:00`).toLocaleDateString(locale(), { year: "numeric", month: "short", day: "numeric" });

  function itemNote(item: FileItem): string {
    if (item.duplicateOf) return t("files.copyOf", { path: shortPath(item.duplicateOf) });
    const folder = folderOf(item.path);
    return item.lastUsed ? `${folder} · ${t("files.lastUsed", { date: dateOf(item.lastUsed) })}` : folder;
  }

  // Temp files are listed by folder, the other groups file by file.
  const moreItems = (g: FileGroup) => Math.max((g.id === "temp" ? g.items.length : g.itemCount) - SHOWN_ITEMS, 0);
</script>

{#if reviewing}
  {@const g = reviewing}
  <FileReview group={g} {version} busy={moving} onBack={() => (reviewing = null)} onMove={moveSelection} onHide={hide} />
{:else}
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
                 onclick={() => (focusedId = group.id)} onkeydown={(e) => e.key === "Enter" && (focusedId = group.id)}>
              <input type="checkbox" checked={checked.has(group.id)} onclick={(e) => e.stopPropagation()} onchange={() => toggle(group.id)} aria-label={t(group.titleKey)} />
              <div class="info">
                <div class="title">{t(group.titleKey)}</div>
                <div class="muted desc">{t(group.descriptionKey)} · {fileCount(group.itemCount)}</div>
              </div>
              {#if reviewable(group)}
                <button class="review-link" onclick={(e) => { e.stopPropagation(); openReview(group); }}>{t("files.review.start")} ›</button>
              {/if}
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
    {#if focused}
      <div class="label">{t("files.details")}</div>
      <h3>{t(focused.titleKey)}</h3>
      <p class="muted desc">{t(focused.descriptionKey)}</p>
      <dl>
        <dt>{t("files.size")}</dt><dd>{formatBytes(focused.sizeBytes)}</dd>
        <dt>{t("files.files")}</dt><dd>{count(focused.itemCount)}</dd>
      </dl>
      <div class="label list-label">{t("files.largest")}</div>
      <ul class="items">
        {#each focused.items.slice(0, SHOWN_ITEMS) as item (item.path)}
          <li title={item.duplicateOf ? `${item.path}\n${t("files.copyOf", { path: item.duplicateOf })}` : item.path}>
            <div class="item-text">
              <div class="item-name">{nameOf(item.path)}</div>
              <div class="muted item-note">{itemNote(item)}</div>
            </div>
            <span class="item-size">{formatBytes(item.sizeBytes)}</span>
          </li>
        {/each}
      </ul>
      {#if moreItems(focused) > 0}
        <div class="muted more">{t("files.more", { count: count(moreItems(focused)) })}</div>
      {/if}
      <div class="grow"></div>
      {#if reviewable(focused)}
        {@const g = focused}
        <button class="btn btn-ghost wide" onclick={() => openReview(g)}>🔍 {t("files.review.oneByOne")}</button>
      {/if}
      {@const id = focused.id}
      <button class="btn btn-primary wide" disabled={moving} onclick={() => moveToQuarantine([id])}>
        {moving ? t("files.moving") : `${t("common.moveToQuarantine")} (${formatBytes(focused.sizeBytes)})`}
      </button>
    {:else}
      <p class="muted">{t("files.pickGroup")}</p>
    {/if}
  </aside>
</div>
{/if}

{#if notice}
  {@const n = notice}
  <div class="card toast" class:error={n.kind === "error"} class:raised={reviewing !== null} role="status" aria-live="polite">
    <span class="toast-icon" aria-hidden="true">{n.kind === "error" ? "⚠️" : "✅"}</span>
    <div class="toast-text">
      {#each n.kind === "moved" ? movedLines(n.report) : n.lines as line, i (i)}<p>{line}</p>{/each}
    </div>
    {#if n.kind === "moved"}
      <button class="btn btn-ghost" disabled={undoing} onclick={() => undo(n.report)}>
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

  .side { width: 300px; flex: none; padding: 18px; min-height: 420px; display: flex; flex-direction: column; position: sticky; top: 0; }
  .label { font-size: 11px; font-weight: 600; letter-spacing: 0.3px; color: var(--text-muted); text-transform: uppercase; }
  .side h3 { font-size: 17px; margin: 6px 0 4px; }
  dl { display: grid; grid-template-columns: 1fr auto; row-gap: 3px; font-size: 12.5px; margin-top: 10px; border-top: 1px solid var(--border); padding-top: 8px; }
  dt { color: var(--text-muted); }
  dd { font-weight: 600; }
  .list-label { margin-top: 14px; }
  .items { list-style: none; margin-top: 6px; }
  .items li { display: flex; align-items: center; gap: 8px; padding: 6px 0; border-bottom: 1px solid var(--border); }
  .item-text { flex: 1; min-width: 0; }
  .item-name, .item-note { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .item-name { font-size: 12.5px; font-weight: 600; }
  .item-note { font-size: 11px; }
  .item-size { font-size: 12px; font-weight: 600; flex: none; }
  .more { font-size: 12px; margin-top: 6px; }
  .wide { width: 100%; padding: 10px; margin-top: 14px; }
  .wide + .wide { margin-top: 8px; }
  .review-link { border: none; background: none; color: var(--accent); font-size: 12.5px; font-weight: 600; padding: 4px 6px; border-radius: var(--radius-sm); white-space: nowrap; }
  .review-link:hover { background: var(--accent-soft); }

  .toast { position: fixed; right: 30px; bottom: 22px; max-width: 560px; display: flex; align-items: center; gap: 12px; padding: 12px 14px 12px 16px; box-shadow: var(--shadow-pop); border-left: 4px solid var(--safe-fg); z-index: 10; }
  .toast.error { border-left-color: var(--danger); }
  /* Above the review's sticky footer, so its buttons stay reachable. */
  .toast.raised { bottom: 96px; }
  .toast-icon { font-size: 18px; }
  .toast-text { flex: 1; font-size: 13px; line-height: 1.4; }
  .toast-text p + p { color: var(--text-muted); margin-top: 2px; }
  .toast .btn { flex: none; }
  .close { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 4px; }
</style>
