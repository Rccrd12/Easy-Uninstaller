<script lang="ts">
  import StatusIcon from "../components/StatusIcon.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import { deleteQuarantine, listQuarantine, restoreQuarantine, usesSampleData } from "../api";
  import { quarantine } from "../settings.svelte";
  import { fileCount, formatBytes, itemCount, relativeTime, restoreSummary } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import type { QuarantineEntry } from "../types";

  /** How many files an opened entry lists. */
  const SHOWN_ITEMS = 8;
  const DAY = 24 * 60 * 60;

  let entries = $state<QuarantineEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  /** The entry being restored or deleted. */
  let busyId = $state<string | null>(null);
  let confirmId = $state<string | null>(null);
  let openId = $state<string | null>(null);
  let notice = $state<{ error: boolean; lines: string[] } | null>(null);

  function load() {
    return listQuarantine()
      .then((e) => {
        entries = e;
        error = null;
      })
      .catch((e) => (error = String(e)))
      .finally(() => (loading = false));
  }

  $effect(() => {
    load();
  });

  const totalBytes = $derived(entries.reduce((s, e) => s + e.sizeBytes, 0));
  const totalFiles = $derived(entries.reduce((s, e) => s + e.itemCount, 0));

  const titleOf = (e: QuarantineEntry) => (e.titleKey ? t(e.titleKey) : (e.title ?? ""));
  const onlyFiles = (e: QuarantineEntry) => e.items.every((i) => i.kind === "file");
  /** "3 files", or "3 items" when the entry holds folders or registry keys. */
  const countOf = (e: QuarantineEntry) => (onlyFiles(e) ? fileCount(e.itemCount) : itemCount(e.itemCount));
  const kindIcons = { file: "", folder: "📂 ", registry: "🔑 " } as const;
  const failure = (err: unknown) => (String(err) === "declined" ? t("quarantine.declined") : t("quarantine.failed", { error: String(err) }));
  const nameOf = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const folderOf = (path: string) => path.slice(0, Math.max(path.length - nameOf(path).length - 1, 0));

  /** Where the files came from: the folder they share, or the single file's path. */
  function origin(e: QuarantineEntry): string {
    if (e.itemCount === 1 && e.items.length) return e.items[0].originalPath;
    const folders = e.items.map((i) => folderOf(i.originalPath).split("\\"));
    if (!folders.length) return "";
    let shared = folders[0];
    for (const f of folders) {
      let n = 0;
      while (n < shared.length && n < f.length && shared[n].toLowerCase() === f[n].toLowerCase()) n++;
      shared = shared.slice(0, n);
    }
    // A bare drive ("C:") says nothing useful.
    return shared.length > 1 ? shared.join("\\") : "";
  }

  function expiry(e: QuarantineEntry): string {
    const left = e.expiresAt - Date.now() / 1000;
    return left < 60 ? t("quarantine.expiresSoon") : t("quarantine.expires", { when: relativeTime(e.expiresAt) });
  }
  const expiresSoon = (e: QuarantineEntry) => e.expiresAt - Date.now() / 1000 < DAY;

  async function restore(e: QuarantineEntry) {
    busyId = e.id;
    notice = null;
    try {
      const r = await restoreQuarantine([e.id]);
      notice = { error: r.failedCount > 0, lines: restoreSummary(r) };
    } catch (err) {
      notice = { error: true, lines: [failure(err)] };
    } finally {
      busyId = null;
      await load();
    }
  }

  async function remove(e: QuarantineEntry) {
    busyId = e.id;
    notice = null;
    try {
      await deleteQuarantine(e.id);
      notice = { error: false, lines: [t("quarantine.deleted", { size: formatBytes(e.sizeBytes) })] };
    } catch (err) {
      notice = { error: true, lines: [failure(err)] };
    } finally {
      busyId = null;
      confirmId = null;
      await load();
    }
  }
</script>

<div class="head">
  <div>
    <h1>{t("quarantine.title")}</h1>
    <p class="muted sub">{t("quarantine.subtitle", { days: quarantine.keepDays })}</p>
  </div>
  {#if entries.length}
    <div class="total">{t("quarantine.summary", { count: entries.some((e) => e.items.some((i) => i.kind !== "file")) ? itemCount(totalFiles) : fileCount(totalFiles), size: formatBytes(totalBytes) })}</div>
  {/if}
</div>
{#if usesSampleData()}<SampleBanner />{/if}

{#if notice}
  <div class="card notice" class:error={notice.error} role="status" aria-live="polite">
    <StatusIcon kind={notice.error ? "warn" : "ok"} />
    <div class="notice-text">{#each notice.lines as line, i (i)}<p>{line}</p>{/each}</div>
    <button class="close" aria-label={t("common.close")} onclick={() => (notice = null)}>✕</button>
  </div>
{/if}

{#if loading}
  <!-- Reading the quarantine folder is quick: no spinner needed. -->
{:else if error}
  <div class="card message"><p>{t("quarantine.failed", { error })}</p></div>
{:else if entries.length === 0}
  <div class="card message">
    <span class="big-icon" aria-hidden="true">🛡️</span>
    <p>{t("quarantine.empty", { days: quarantine.keepDays })}</p>
  </div>
{:else}
  {#each entries as e (e.id)}
    {@const from = origin(e)}
    <div class="card entry" class:busy={busyId === e.id}>
      <div class="row">
        <div class="info">
          <div class="title">{titleOf(e)}</div>
          <div class="muted meta" title={from}>
            {countOf(e)}{#if from}{" · "}{t("quarantine.from", { path: from })}{/if}
          </div>
          <div class="meta">
            <span class="muted">{t("quarantine.removedOn", { when: relativeTime(e.removedAt) })}{" · "}</span>
            <span class:soon={expiresSoon(e)} class:muted={!expiresSoon(e)}>{expiry(e)}</span>
          </div>
        </div>
        <span class="size">{formatBytes(e.sizeBytes)}</span>
        <button class="btn btn-primary" disabled={busyId !== null} onclick={() => restore(e)} title={e.needsAdmin ? t("quarantine.permission") : undefined}>
          {busyId === e.id && confirmId !== e.id ? t("quarantine.restoring") : `${e.needsAdmin ? "🛡️" : "↩"} ${t("common.restore")}`}
        </button>
        <button class="btn btn-ghost" disabled={busyId !== null} onclick={() => (confirmId = confirmId === e.id ? null : e.id)}>
          {t("quarantine.deleteNow")}
        </button>
      </div>

      {#if confirmId === e.id}
        <div class="confirm">
          <span>{t("quarantine.confirmDelete", { size: formatBytes(e.sizeBytes) })}</span>
          <span class="grow"></span>
          <button class="btn btn-ghost" disabled={busyId !== null} onclick={() => (confirmId = null)}>{t("common.cancel")}</button>
          <button class="btn btn-danger" disabled={busyId !== null} onclick={() => remove(e)}>{t("quarantine.confirmDeleteButton")}</button>
        </div>
      {/if}

      {#if e.items.length > 1}
        <button class="link" aria-expanded={openId === e.id} onclick={() => (openId = openId === e.id ? null : e.id)}>
          {openId === e.id
            ? `▾ ${t(onlyFiles(e) ? "quarantine.hideFiles" : "quarantine.hideItems")}`
            : `▸ ${t(onlyFiles(e) ? "quarantine.showFiles" : "quarantine.showItems")}`}
        </button>
        {#if openId === e.id}
          <ul class="items">
            {#each e.items.slice(0, SHOWN_ITEMS) as item (item.originalPath)}
              <li title={item.originalPath}>
                <div class="item-text">
                  <div class="item-name">{kindIcons[item.kind]}{nameOf(item.originalPath)}</div>
                  <div class="muted item-note">{folderOf(item.originalPath)}</div>
                </div>
                <span class="item-size">{item.kind === "registry" ? "—" : formatBytes(item.sizeBytes)}</span>
              </li>
            {/each}
          </ul>
          {#if e.itemCount > SHOWN_ITEMS}
            <div class="muted more">{t("quarantine.more", { count: (e.itemCount - SHOWN_ITEMS).toLocaleString(locale()) })}</div>
          {/if}
        {/if}
      {/if}
    </div>
  {/each}
{/if}

<style>
  .head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .sub { margin: 3px 0 10px; }
  .total { flex: none; margin-top: 8px; font-size: 13px; font-weight: 600; padding: 6px 12px; border-radius: 14px; background: var(--accent-soft); color: var(--accent); }

  .entry { padding: 12px 16px; margin-bottom: 8px; }
  .entry.busy { opacity: 0.7; }
  .row { display: flex; align-items: center; gap: 12px; }
  .info { flex: 1; min-width: 0; }
  .title { font-size: 14px; font-weight: 600; }
  .meta { font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .soon { color: var(--check-fg); font-weight: 600; }
  .size { font-size: 14px; font-weight: 600; width: 76px; text-align: right; }
  .btn { flex: none; }
  .btn:disabled { opacity: 0.5; cursor: default; }

  .confirm { display: flex; align-items: center; gap: 10px; margin-top: 10px; padding: 10px 12px; border-radius: var(--radius-sm); background: var(--keep-bg); color: var(--keep-fg); font-size: 13px; font-weight: 600; }
  .grow { flex: 1; }

  .link { border: none; background: none; color: var(--accent); font-size: 12.5px; padding: 0; margin-top: 8px; }
  .items { list-style: none; margin-top: 6px; }
  .items li { display: flex; align-items: center; gap: 8px; padding: 6px 0; border-bottom: 1px solid var(--border); }
  .item-text { flex: 1; min-width: 0; }
  .item-name, .item-note { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .item-name { font-size: 12.5px; font-weight: 600; }
  .item-note { font-size: 11px; }
  .item-size { font-size: 12px; font-weight: 600; flex: none; }
  .more { font-size: 12px; margin-top: 6px; }

  .notice { display: flex; align-items: center; gap: 12px; padding: 12px 14px 12px 16px; margin-bottom: 10px; border-left: 4px solid var(--safe-fg); font-size: 13px; }
  .notice.error { border-left-color: var(--danger); }
  .notice-text { flex: 1; line-height: 1.4; }
  .notice-text p + p { color: var(--text-muted); margin-top: 2px; }
  .close { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 4px; }

  .message { padding: 22px 24px; margin-top: 8px; display: flex; align-items: center; gap: 14px; }
  .message p { flex: 1; }
  .big-icon { font-size: 26px; }
</style>
