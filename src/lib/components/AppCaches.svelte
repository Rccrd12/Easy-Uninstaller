<script lang="ts">
  import Icon from "./Icon.svelte";
  import StatusIcon from "./StatusIcon.svelte";
  import { clearAppCache } from "../api";
  import { fileCount, formatBytes } from "../format";
  import { t } from "../i18n/index.svelte";
  import type { AppCache, InstalledApp } from "../types";

  let {
    apps,
    caches,
    measuring,
    onCleared,
  }: {
    apps: InstalledApp[];
    caches: Map<string, AppCache>;
    /** The caches are still being measured. */
    measuring: boolean;
    /** A cache measured again after it was emptied, and the bytes freed. */
    onCleared: (cache: AppCache, freed: number) => void;
  } = $props();

  interface Row {
    app: InstalledApp;
    cache: AppCache;
  }

  /** Installed apps with a cache we know how to empty, biggest cache first. */
  const rows = $derived(
    apps
      .flatMap((app): Row[] => {
        const cache = app.cacheId ? caches.get(app.cacheId) : undefined;
        return cache ? [{ app, cache }] : [];
      })
      .toSorted((a, b) => b.cache.cacheBytes - a.cache.cacheBytes),
  );
  const total = $derived(rows.reduce((s, r) => s + r.cache.cacheBytes, 0));
  const withCache = $derived(rows.filter((r) => r.cache.cacheBytes > 0));

  /** Cache ids being emptied right now. */
  let busy = $state<Set<string>>(new Set());
  /** Cache ids left alone because their app was open. */
  let open = $state<Set<string>>(new Set());
  let result = $state<{ freed: number; open: number; skipped: number; failed: number } | null>(null);

  async function clearOne(r: Row): Promise<{ freed: number; open: boolean; skipped: number; failed: boolean }> {
    busy = new Set(busy).add(r.cache.id);
    try {
      const out = await clearAppCache(r.cache.id);
      const nextOpen = new Set(open);
      if (out.running) nextOpen.add(r.cache.id);
      else nextOpen.delete(r.cache.id);
      open = nextOpen;
      if (out.cache) onCleared(out.cache, out.report.freedBytes);
      return { freed: out.report.freedBytes, open: out.running, skipped: out.report.skippedCount, failed: false };
    } catch {
      return { freed: 0, open: false, skipped: 0, failed: true };
    } finally {
      const next = new Set(busy);
      next.delete(r.cache.id);
      busy = next;
    }
  }

  async function clear(list: Row[]) {
    result = null;
    const sum = { freed: 0, open: 0, skipped: 0, failed: 0 };
    for (const r of list) {
      const out = await clearOne(r);
      sum.freed += out.freed;
      sum.skipped += out.skipped;
      if (out.open) sum.open++;
      if (out.failed) sum.failed++;
    }
    result = sum;
  }
</script>

{#if measuring && !rows.length}
  <div class="card status" role="status"><span class="spinner" aria-hidden="true"></span>{t("cache.measuring")}</div>
{:else if !rows.length}
  <p class="muted empty">{t("cache.none")}</p>
{:else}
  <div class="card hero">
    <span class="tile"><Icon name="broom" size={24} /></span>
    <div class="grow">
      <b>{withCache.length ? t("cache.total", { size: formatBytes(total), count: withCache.length }) : t("cache.allEmpty")}</b>
      <div class="muted">{t("cache.what")}</div>
    </div>
    <button class="btn btn-primary" disabled={busy.size > 0 || !withCache.length} onclick={() => clear(withCache)}>
      {busy.size ? t("cache.clearing") : t("cache.clearAll")}
    </button>
  </div>

  {#if result}
    <div class="card note" role="status">
      <StatusIcon kind={result.failed || result.open ? "warn" : "ok"} />
      <span>
        {t("cache.freed", { size: formatBytes(result.freed) })}
        {#if result.open}{" "}{result.open === 1 ? t("cache.openSkippedOne") : t("cache.openSkipped", { count: result.open })}{/if}
        {#if result.skipped}{" "}{t("cache.inUse", { files: fileCount(result.skipped) })}{/if}
        {#if result.failed}{" "}{t("cache.failed", { count: result.failed })}{/if}
      </span>
    </div>
  {/if}

  {#each rows as r (r.cache.id)}
    <div class="card row">
      {#if r.app.icon}
        <img class="ico" src={r.app.icon} alt="" width="32" height="32" />
      {:else}
        <div class="ico" style="background:{r.app.iconColor}">{r.app.name[0]}</div>
      {/if}
      <div class="info">
        <div class="name">{r.app.name}</div>
        {#if open.has(r.cache.id)}
          <div class="meta warn">{t("cache.isOpen")}</div>
        {:else}
          <div class="muted meta">{r.cache.cacheBytes ? fileCount(r.cache.fileCount) : t("cache.empty")}</div>
        {/if}
      </div>
      <div class="size">{r.cache.cacheBytes ? formatBytes(r.cache.cacheBytes) : "–"}</div>
      <button class="btn btn-ghost" disabled={busy.has(r.cache.id) || !r.cache.cacheBytes} onclick={() => clear([r])}>
        {busy.has(r.cache.id) ? t("cache.clearingOne") : t("cache.clear")}
      </button>
    </div>
  {/each}
{/if}

<style>
  .hero { display: flex; align-items: center; gap: 14px; padding: 14px 16px; margin-bottom: 12px; flex-wrap: wrap; }
  .hero .muted { font-size: 13px; margin-top: 2px; }
  .tile { width: 42px; height: 42px; border-radius: 10px; display: flex; align-items: center; justify-content: center; background: var(--accent-soft); color: var(--accent); flex: none; }
  .grow { flex: 1; min-width: 200px; }
  .note { display: flex; align-items: center; gap: 10px; padding: 10px 14px; margin-bottom: 12px; font-size: 13.5px; }
  .row { display: flex; align-items: center; gap: 13px; padding: 10px 14px; margin-bottom: 6px; }
  .ico { width: 38px; height: 38px; border-radius: 9px; display: flex; align-items: center; justify-content: center; color: #fff; font-weight: 700; flex: none; }
  .info { min-width: 0; flex: 1; }
  .name { font-size: 14.5px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { font-size: 12px; }
  .meta.warn { color: var(--keep-fg); font-weight: 600; }
  .size { text-align: right; font-size: 13.5px; width: 90px; flex: none; font-variant-numeric: tabular-nums; }
  .status { display: flex; align-items: center; gap: 10px; padding: 14px 16px; }
  .empty { padding: 20px 4px; }
</style>
