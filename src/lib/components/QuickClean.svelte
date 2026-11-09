<script lang="ts">
  import SafetyTag from "./SafetyTag.svelte";
  import Icon from "./Icon.svelte";
  import type { IconName } from "../icons";
  import { DELETE_NOW_GROUPS, getSystemDisk, keepFileGroups, listFileGroups, removeFileGroups, restoreQuarantine } from "../api";
  import { fileCount, formatBytes, restoreSummary } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import { quarantine } from "../settings.svelte";
  import type { DiskInfo, FileGroup, MoveReport, ScanProgress } from "../types";

  let {
    onClose,
    onDetails,
    onSettings,
  }: {
    /** Called when the dialog closes; `changed` is true when something was removed or restored. */
    onClose: (changed: boolean) => void;
    /** "Let me see the details first": opens the Files screen. */
    onDetails: () => void;
    onSettings: () => void;
  } = $props();

  type Phase = "scanning" | "ready" | "cleaning" | "done" | "error";

  const ICONS: Record<string, IconName> = { temp: "delete", installers: "download", "recycle-bin": "recycle", "crash-reports": "receipt", thumbnails: "image" };

  let phase = $state<Phase>("scanning");
  let progress = $state<ScanProgress | null>(null);
  let error = $state<string | null>(null);
  /** Every group of the scan, so the kept result can be updated after cleaning. */
  let all = $state<FileGroup[]>([]);
  let checked = $state<Set<string>>(new Set());
  let report = $state<MoveReport | null>(null);
  let removed = $state<FileGroup[]>([]);
  let diskBefore = $state<DiskInfo | null>(null);
  let diskAfter = $state<DiskInfo | null>(null);
  let undoing = $state(false);
  let undoLines = $state<string[] | null>(null);
  let changed = false;

  /** Only what is safe: anything that needs a look stays on the Files screen. */
  const safe = $derived(all.filter((g) => g.safety === "safe" && !g.reviewOnly && g.itemCount > 0));
  const deletesNow = (id: string) => DELETE_NOW_GROUPS.includes(id);
  const selected = $derived(safe.filter((g) => checked.has(g.id)));
  const totalBytes = $derived(selected.reduce((s, g) => s + g.sizeBytes, 0));
  /** With the quarantine on another drive, moving a file frees its space at once. */
  const nowBytes = $derived(selected.filter((g) => deletesNow(g.id) || quarantine.drive).reduce((s, g) => s + g.sizeBytes, 0));
  const heldBytes = $derived(totalBytes - nowBytes);
  /** After cleaning: moved, but still on the same drive until the quarantine lets go. */
  const heldAfter = $derived(report ? report.movedBytes - (report.freedBytes - report.deletedBytes) : 0);

  $effect(() => {
    listFileGroups({ onProgress: (p) => (progress = p) })
      .then((g) => {
        all = g;
        checked = new Set(g.filter((x) => x.safety === "safe").map((x) => x.id));
        phase = "ready";
      })
      .catch((e) => {
        error = String(e);
        phase = "error";
      });
  });

  function toggle(id: string) {
    const next = new Set(checked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    checked = next;
  }

  async function clean() {
    const ids = selected.map((g) => g.id);
    if (!ids.length) return;
    phase = "cleaning";
    diskBefore = await getSystemDisk().catch(() => null);
    try {
      report = await removeFileGroups(ids, { quick: true });
      changed = true;
      removed = selected;
      keepFileGroups(all.filter((g) => !ids.includes(g.id)));
      diskAfter = await getSystemDisk().catch(() => null);
      phase = "done";
    } catch (e) {
      error = String(e);
      phase = "error";
    }
  }

  /** Puts back what went to Quarantine. Deleted caches cannot come back, and need not. */
  async function undo() {
    if (!report) return;
    undoing = true;
    try {
      const r = await restoreQuarantine(report.entryIds);
      const back = removed.filter((g) => !deletesNow(g.id));
      const left = all.filter((g) => !removed.includes(g) || back.includes(g));
      keepFileGroups(left);
      undoLines = restoreSummary(r);
      if (report.deletedCount) undoLines.push(t("quick.undoDeleted"));
      diskAfter = await getSystemDisk().catch(() => diskAfter);
    } catch (e) {
      undoLines = [t("quarantine.failed", { error: String(e) })];
    } finally {
      undoing = false;
    }
  }

  function close() {
    if (phase !== "cleaning") onClose(changed);
  }

  const until = (days: number) =>
    new Date(Date.now() + days * 86_400_000).toLocaleDateString(locale(), { day: "numeric", month: "long" });
  const count = (n: number) => n.toLocaleString(locale());
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={close}></div>
<div class="card dialog" role="dialog" aria-modal="true" aria-labelledby="qc-title">
  <div class="top">
    <h2 id="qc-title">{t("quick.title")}</h2>
    {#if phase !== "cleaning"}
      <button class="x" aria-label={t("common.close")} onclick={close}><Icon name="dismiss" size={18} /></button>
    {/if}
  </div>

  {#if phase === "scanning"}
    <div class="busy" role="status" aria-live="polite">
      <span class="spinner" aria-hidden="true"></span>
      <div>
        <b>{t("quick.scanning")}</b>
        {#if progress}
          <div class="muted small">
            {t("files.stepOf", { n: progress.stepIndex, total: progress.stepCount })} · {t(`files.steps.${progress.step}`)} ·
            {t("files.checked", { count: count(progress.filesChecked) })}
          </div>
        {/if}
      </div>
    </div>
  {:else if phase === "error"}
    <p class="lead">{t("quick.failed", { error: error ?? "" })}</p>
    <div class="actions"><button class="btn btn-primary btn-lg" onclick={close}>{t("common.close")}</button></div>
  {:else if phase === "ready" && safe.length === 0}
    <div class="empty"><span class="empty-icon"><Icon name="checkCircle" size={26} /></span><p>{t("quick.empty")}</p></div>
    <div class="actions">
      <button class="btn btn-primary btn-lg" onclick={close}>{t("common.close")}</button>
      <button class="btn btn-ghost btn-lg" onclick={onDetails}>{t("quick.seeFiles")}</button>
    </div>
  {:else if phase === "ready" || phase === "cleaning"}
    <ul class="lines">
      {#each safe as group (group.id)}
        <li>
          <label>
            <input type="checkbox" checked={checked.has(group.id)} disabled={phase === "cleaning"} onchange={() => toggle(group.id)} />
            <span class="icon"><Icon name={ICONS[group.id] ?? "document"} size={20} /></span>
            <span class="grow">
              <b>{t(group.titleKey)}</b>
              <span class="muted small block">{t(group.descriptionKey)} · {fileCount(group.itemCount)}</span>
            </span>
            <SafetyTag safety={group.safety} />
            <b class="size">{formatBytes(group.sizeBytes)}</b>
          </label>
        </li>
      {/each}
    </ul>

    <div class="split">
      <div>
        <div class="muted small">{t("quick.freeNow")}</div>
        <div class="num">{formatBytes(nowBytes)}</div>
      </div>
      {#if heldBytes > 0}
        <div class="vr"></div>
        <div>
          <div class="muted small">{t("quick.held", { days: quarantine.keepDays })}</div>
          <div class="num plain">{formatBytes(heldBytes)}</div>
        </div>
      {/if}
    </div>

    <div class="actions">
      <button class="btn btn-primary btn-lg" disabled={totalBytes === 0 || phase === "cleaning"} onclick={clean}>
        {phase === "cleaning" ? t("quick.cleaning") : t("quick.free", { size: formatBytes(totalBytes) })}
      </button>
      {#if phase === "ready"}
        <button class="btn btn-ghost btn-lg" onclick={onDetails}>{t("quick.details")}</button>
      {/if}
    </div>
  {:else if phase === "done" && report}
    <div class="result" role="status">
      <span class="check"><Icon name="check" size={24} /></span>
      <div>
        <div class="muted small">{t("quick.doneTitle")}</div>
        <div class="num">{formatBytes(report.freedBytes)}</div>
        <div class="muted small">{report.freedBytes > 0 ? t("quick.freed") : t("quick.nothingFreed")}</div>
      </div>
    </div>
    <ul class="facts">
      {#if heldAfter > 0 && !undoLines}
        <li><Icon name="quarantine" />{t("quick.heldDone", { size: formatBytes(heldAfter), date: until(quarantine.keepDays) })}</li>
      {/if}
      {#if report.skippedCount}
        <li><Icon name="pause" />{t("files.movedSkipped", { count: fileCount(report.skippedCount) })}</li>
      {/if}
      {#if diskAfter}
        <li><Icon name="drive" />{diskBefore && diskAfter.freeBytes > diskBefore.freeBytes
            ? t("quick.disk", { drive: diskAfter.mountPoint, free: formatBytes(diskAfter.freeBytes), before: formatBytes(diskBefore.freeBytes) })
            : t("quick.diskNow", { drive: diskAfter.mountPoint, free: formatBytes(diskAfter.freeBytes) })}</li>
      {/if}
      {#each undoLines ?? [] as line, i (i)}<li><Icon name="undo" />{line}</li>{/each}
    </ul>
    <div class="actions">
      <button class="btn btn-primary btn-lg" onclick={close}>{t("common.close")}</button>
      {#if report.entryIds.length && !undoLines}
        <button class="btn btn-ghost btn-lg" disabled={undoing} onclick={undo}>{#if !undoing}<Icon name="undo" />{/if}{undoing ? t("files.undoing") : t("quick.undo")}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgba(20, 24, 30, 0.42); z-index: 20; }
  .dialog {
    position: fixed; left: 50%; top: 50%; transform: translate(-50%, -50%); z-index: 21;
    width: min(640px, calc(100vw - 48px)); max-height: calc(100vh - 48px); overflow-y: auto;
    padding: 22px 24px; box-shadow: var(--shadow-pop); background: var(--surface);
  }
  .top { display: flex; align-items: center; justify-content: space-between; }
  h2 { font-size: 20px; }
  .x { border: none; background: none; color: var(--text-muted); padding: 4px 6px; display: flex; border-radius: var(--radius-sm); }
  .x:hover { background: var(--hover); }
  .lead { margin: 4px 0 14px; font-size: 13.5px; }
  .small { font-size: 12.5px; }
  .block { display: block; }
  .grow { flex: 1; min-width: 0; }

  .lines { list-style: none; }
  .lines li { border-top: 1px solid var(--border); }
  .lines label { display: flex; align-items: center; gap: 12px; padding: 10px 0; cursor: pointer; }
  .lines input { width: 18px; height: 18px; accent-color: var(--accent); flex: none; }
  .icon { width: 26px; display: flex; justify-content: center; flex: none; color: var(--text-muted); }
  .size { width: 72px; text-align: right; flex: none; }

  .split { display: flex; gap: 18px; background: var(--bg); border-radius: var(--radius); padding: 14px 18px; margin-top: 10px; }
  .split > div:not(.vr) { flex: 1; }
  .vr { width: 1px; background: var(--border); }
  .num { font-size: 26px; font-weight: 700; color: var(--accent); margin: 2px 0; }
  .num.plain { color: var(--text); }

  .actions { display: flex; align-items: center; gap: 12px; margin-top: 18px; flex-wrap: wrap; }
  .btn-lg { padding: 10px 18px; font-size: 14px; }
  .btn:disabled { opacity: 0.6; cursor: default; }

  .busy { display: flex; align-items: center; gap: 14px; padding: 22px 4px 10px; }
  .spinner { width: 26px; height: 26px; border-radius: 50%; border: 3px solid var(--accent-soft); border-top-color: var(--accent); animation: spin 0.9s linear infinite; flex: none; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .empty { display: flex; align-items: center; gap: 12px; padding: 18px 0 4px; font-size: 14px; }
  .empty-icon { display: flex; color: var(--safe-fg); }

  .result { display: flex; align-items: center; gap: 16px; background: var(--bg); border-radius: var(--radius); padding: 16px 18px; margin-top: 12px; }
  .check { width: 44px; height: 44px; border-radius: 50%; background: var(--safe-bg); color: var(--safe-fg); display: flex; align-items: center; justify-content: center; flex: none; }
  .result .num { font-size: 32px; }
  .facts { list-style: none; margin-top: 12px; font-size: 13px; line-height: 1.45; }
  .facts li { padding: 5px 0; display: flex; gap: 8px; align-items: baseline; }
  .facts li :global(.icon) { color: var(--text-muted); align-self: center; }
</style>
