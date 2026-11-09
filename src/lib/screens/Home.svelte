<script lang="ts">
  import QuickClean from "../components/QuickClean.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import Icon from "../components/Icon.svelte";
  import type { IconName } from "../icons";
  import { getHistory, getSystemDisk, listFileGroups, listInstalledApps, listLeftovers, listQuarantine, usesSampleData } from "../api";
  import { quarantine } from "../settings.svelte";
  import { fileCount, formatBytes, itemCount } from "../format";
  import { dayLabel, historyAmount, historyTitle } from "../history";
  import { locale, t } from "../i18n/index.svelte";
  import type { Screen } from "../screens";
  import type { DiskInfo, FileGroup, HistorySummary, QuarantineEntry } from "../types";

  /** How many recent actions the results card lists. */
  const RECENT = 3;

  let { onNavigate }: { onNavigate: (s: Screen) => void } = $props();

  let disk = $state<DiskInfo | null>(null);
  /** Apps flagged as unused or bloatware (only the sample data has these flags for now). */
  let appsBytes = $state<number | null>(null);
  /** Every installed app, shown when none is flagged so the tile says something real. */
  let appsTotal = $state({ count: 0, bytes: 0 });
  /** Files that can be removed (all groups) and the safe part of them, for the total. */
  let filesBytes = $state<number | null>(null);
  let safeFilesBytes = $state(0);
  let leftoversBytes = $state<number | null>(null);
  let safeLeftoversBytes = $state(0);
  let quickClean = $state(false);
  let history = $state<HistorySummary | null>(null);
  let inQuarantine = $state<QuarantineEntry[] | null>(null);

  /** The results and Quarantine cards: they change after every cleanup. */
  function loadResults() {
    getHistory().then((h) => (history = h), () => {});
    listQuarantine().then((q) => (inQuarantine = q), () => {});
  }

  function setFiles(groups: FileGroup[]) {
    // Review-only groups (unknown folders) are explained, not offered for removal.
    const removable = groups.filter((x) => !x.reviewOnly);
    filesBytes = removable.reduce((s, x) => s + x.sizeBytes, 0);
    safeFilesBytes = removable.filter((x) => x.safety === "safe").reduce((s, x) => s + x.sizeBytes, 0);
  }

  function refreshAfterClean(changed: boolean) {
    quickClean = false;
    if (!changed) return;
    loadResults();
    getSystemDisk().then((d) => (disk = d));
    listFileGroups().then(setFiles);
  }

  $effect(() => {
    loadResults();
    getSystemDisk().then((d) => (disk = d));
    listInstalledApps()
      .then((apps) => {
        appsBytes = apps.filter((a) => a.bloatware || a.neverUsed).reduce((s, a) => s + a.sizeBytes, 0);
        appsTotal = { count: apps.length, bytes: apps.reduce((s, a) => s + a.sizeBytes, 0) };
      })
      .catch(() => (appsBytes = 0));
    // The file scan takes a while: the total says "calculating" until it is done.
    listFileGroups().then(setFiles, () => (filesBytes = 0));
    listLeftovers()
      .then((l) => {
        leftoversBytes = l.reduce((s, x) => s + x.sizeBytes, 0);
        safeLeftoversBytes = l.filter((x) => x.safety === "safe").reduce((s, x) => s + x.sizeBytes, 0);
      })
      .catch(() => (leftoversBytes = 0));
  });

  const calculating = $derived(filesBytes === null || leftoversBytes === null || appsBytes === null);
  /** "Safe to free": safe file groups, safe leftovers and apps flagged as unused. */
  const reclaimable = $derived((appsBytes ?? 0) + safeFilesBytes + safeLeftoversBytes);

  function tileAmount(id: Screen): string {
    if (id === "apps") {
      if (appsBytes === null) return t("home.calculating");
      if (appsBytes > 0) return formatBytes(appsBytes);
      return t("home.appsInstalled", { count: appsTotal.count.toLocaleString(locale()), size: formatBytes(appsTotal.bytes) });
    }
    const bytes = id === "files" ? filesBytes : id === "leftovers" ? leftoversBytes : undefined;
    if (bytes === undefined) return "";
    return bytes === null ? t("home.calculating") : formatBytes(bytes);
  }
  const usedPct = $derived(disk ? ((disk.totalBytes - disk.freeBytes) / disk.totalBytes) * 100 : 0);
  const reclaimPct = $derived(disk ? (reclaimable / disk.totalBytes) * 100 : 0);
  const lowSpace = $derived(disk ? disk.freeBytes / disk.totalBytes < 0.15 : false);

  const quarantineBytes = $derived((inQuarantine ?? []).reduce((s, e) => s + e.sizeBytes, 0));
  const quarantineCount = $derived.by(() => {
    const entries = inQuarantine ?? [];
    const n = entries.reduce((s, e) => s + e.itemCount, 0);
    return entries.every((e) => e.items.every((i) => i.kind === "file")) ? fileCount(n) : itemCount(n);
  });

  const tiles: { id: Screen; icon: IconName; primary: boolean }[] = [
    { id: "apps", icon: "apps", primary: false },
    { id: "files", icon: "folder", primary: true },
    { id: "leftovers", icon: "broom", primary: false },
    { id: "map", icon: "map", primary: false },
  ];
</script>

<h1>{t("home.greeting")}</h1>
{#if usesSampleData()}<SampleBanner />{/if}

<section class="card hero">
  <div>
    <div class="muted small">{t("home.canFree")}</div>
    {#if calculating}
      <div class="big calc" role="status">{t("home.calculating")}</div>
      <div class="muted small">{t("home.calculatingNote")}</div>
    {:else}
      <div class="big">{formatBytes(reclaimable)}</div>
    {/if}
    <div class="ctas">
      <button class="btn btn-primary cta" onclick={() => (quickClean = true)}>{t("home.quickClean")}</button>
      <button class="btn btn-ghost cta" onclick={() => onNavigate("files")}>{t("home.seeAll")}</button>
    </div>
  </div>
  {#if disk}
    <div class="disk">
      <div class="row small">
        <b>{t("home.disk", { drive: disk.mountPoint })}</b>
        <span class="muted">{t("home.diskUsage", { used: formatBytes(disk.totalBytes - disk.freeBytes), total: formatBytes(disk.totalBytes), free: formatBytes(disk.freeBytes) })}</span>
      </div>
      <div class="bar" role="img" aria-label={t("home.diskUsage", { used: formatBytes(disk.totalBytes - disk.freeBytes), total: formatBytes(disk.totalBytes), free: formatBytes(disk.freeBytes) })}>
        <i style="width:{Math.max(usedPct - reclaimPct, 0)}%;background:{lowSpace ? 'var(--danger)' : 'var(--accent)'}"></i>
        <i style="width:{reclaimPct}%;background:var(--cat-reclaimable)"></i>
      </div>
      <div class="legend small">
        <span><b style="background:{lowSpace ? 'var(--danger)' : 'var(--accent)'}"></b>{t("home.used")} {formatBytes(disk.totalBytes - disk.freeBytes - reclaimable)}</span>
        <span><b style="background:var(--cat-reclaimable)"></b>{t("categories.reclaimable")} {calculating ? "…" : formatBytes(reclaimable)}</span>
      </div>
    </div>
  {/if}
</section>

<section class="tiles">
  {#each tiles as tile (tile.id)}
    <div class="card tile" class:primary={tile.primary}>
      <div class="ticon"><Icon name={tile.icon} size={24} /></div>
      <h3>{t(`home.${tile.id}`)}</h3>
      <p class="muted">{t(`home.${tile.id}Text`)}</p>
      <div class="amount" class:calc={tileAmount(tile.id) === t("home.calculating")}>{tileAmount(tile.id)}</div>
      <button class="btn {tile.primary ? 'btn-primary' : 'btn-ghost'}" onclick={() => onNavigate(tile.id)}>{t(`home.${tile.id}Button`)}</button>
    </div>
  {/each}
</section>

<section class="lower">
  <div class="card box">
    <div class="box-head">
      <h2 class="icon-line"><Icon name="history" size={18} />{t("home.results")}</h2>
      <button class="link" onclick={() => onNavigate("history")}>{t("home.historyLink")}<Icon name="chevronRight" size={14} /></button>
    </div>
    {#if history && history.actionCount > 0}
      <div class="freed">
        <span class="freed-big">{formatBytes(history.freedBytes)}</span>
        <span class="muted small">{t("home.resultsFreed")}</span>
      </div>
      {#if history.pendingBytes > 0}<div class="muted note">{t("home.resultsPending", { size: formatBytes(history.pendingBytes) })}</div>{/if}
      <ul class="recent">
        {#each history.items.slice(0, RECENT) as item (item.id)}
          <li>
            <span class="muted day">{dayLabel(item.at)}</span>
            <span class="what">{historyTitle(item)}</span>
            <b class:muted={item.freedBytes === 0}>{historyAmount(item)}</b>
          </li>
        {/each}
      </ul>
    {:else if history}
      <p class="muted empty">{t("home.resultsEmpty")}</p>
    {/if}
  </div>
  <div class="card box">
    <div class="box-head">
      <h2 class="icon-line"><Icon name="shieldCheck" size={18} />{t("home.quarantineTitle")}</h2>
      <button class="link" onclick={() => onNavigate("quarantine")}>{t("home.quarantineLink")}<Icon name="chevronRight" size={14} /></button>
    </div>
    {#if inQuarantine}
      <div class="q-state">{inQuarantine.length ? t("home.quarantineHolds", { count: quarantineCount, size: formatBytes(quarantineBytes) }) : t("home.quarantineEmpty")}</div>
    {/if}
  </div>
</section>

{#if quickClean}
  <QuickClean
    onClose={refreshAfterClean}
    onDetails={() => onNavigate("files")}
    onSettings={() => onNavigate("settings")}
  />
{/if}

<style>
  .small { font-size: 13px; }
  .hero { display: flex; gap: 32px; align-items: center; padding: 22px 26px; margin-top: 4px; }
  .big { font-size: 40px; font-weight: 700; color: var(--accent); line-height: 1.1; }
  .big.calc { font-size: 26px; color: var(--text-muted); }
  .amount.calc { font-size: 15px; font-weight: 600; color: var(--text-muted); }
  .ctas { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .cta { padding: 10px 18px; font-size: 14px; }
  .disk { flex: 1; }
  .row { display: flex; justify-content: space-between; gap: 12px; }
  .bar { display: flex; height: 16px; border-radius: 8px; overflow: hidden; margin: 10px 0 8px; background: var(--track); }
  .bar i { display: block; height: 100%; }
  .legend { display: flex; gap: 16px; flex-wrap: wrap; }
  .legend b { display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 6px; }
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 14px; margin-top: 16px; }
  .tile { padding: 18px; display: flex; flex-direction: column; min-height: 230px; }
  .tile.primary { border: 2px solid var(--accent); }
  .ticon { width: 44px; height: 44px; border-radius: var(--radius); display: flex; align-items: center; justify-content: center; background: var(--icon-tile-bg); color: var(--icon-tile-fg); }
  .tile h3 { font-size: 16px; margin: 12px 0 4px; }
  .tile p { font-size: 12.5px; line-height: 1.45; flex: 1; }
  .amount { font-size: 20px; font-weight: 700; margin: 6px 0 10px; min-height: 26px; }
  .lower { display: grid; grid-template-columns: 1.25fr 1fr; gap: 14px; margin-top: 14px; }
  @media (max-width: 860px) { .lower { grid-template-columns: 1fr; } }
  .box { padding: 16px 18px; }
  .box-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .box h2 { font-size: 15px; font-weight: 600; }
  .box h2 :global(.icon) { color: var(--accent); }
  .link { border: none; background: none; color: var(--accent); font-size: 13px; padding: 2px 0; display: inline-flex; align-items: center; gap: 2px; }
  .link:hover { text-decoration: underline; }
  .freed { display: flex; align-items: baseline; gap: 8px; flex-wrap: wrap; margin: 8px 0 2px; }
  .freed-big { font-size: 26px; font-weight: 700; color: var(--safe-fg); line-height: 1.15; }
  .note { font-size: 12px; }
  .recent { list-style: none; margin-top: 8px; }
  .recent li { display: flex; align-items: center; gap: 12px; padding: 7px 0; border-top: 1px solid var(--border); font-size: 13px; }
  .day { width: 64px; flex: none; }
  .what { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .recent b { flex: none; }
  .empty { font-size: 13px; line-height: 1.45; margin-top: 10px; }
  .q-state { font-size: 14px; font-weight: 600; margin: 10px 0 6px; }
</style>
