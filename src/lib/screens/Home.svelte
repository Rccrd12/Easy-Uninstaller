<script lang="ts">
  import SampleBanner from "../components/SampleBanner.svelte";
  import { getSystemDisk, listFileGroups, listInstalledApps, listLeftovers, QUARANTINE_DAYS, usesSampleData } from "../api";
  import { formatBytes } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import type { Screen } from "../screens";
  import type { DiskInfo } from "../types";

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

  $effect(() => {
    getSystemDisk().then((d) => (disk = d));
    listInstalledApps()
      .then((apps) => {
        appsBytes = apps.filter((a) => a.bloatware || a.neverUsed).reduce((s, a) => s + a.sizeBytes, 0);
        appsTotal = { count: apps.length, bytes: apps.reduce((s, a) => s + a.sizeBytes, 0) };
      })
      .catch(() => (appsBytes = 0));
    // The file scan takes a while: the total says "calculating" until it is done.
    listFileGroups().then(
      (g) => {
        // Review-only groups (unknown folders) are explained, not offered for removal.
        const removable = g.filter((x) => !x.reviewOnly);
        filesBytes = removable.reduce((s, x) => s + x.sizeBytes, 0);
        safeFilesBytes = removable.filter((x) => x.safety === "safe").reduce((s, x) => s + x.sizeBytes, 0);
      },
      () => (filesBytes = 0),
    );
    listLeftovers()
      .then((l) => {
        leftoversBytes = l.reduce((s, x) => s + x.sizeBytes, 0);
      })
      .catch(() => (leftoversBytes = 0));
  });

  const calculating = $derived(filesBytes === null || leftoversBytes === null || appsBytes === null);
  /** "Safe to free": safe file groups, leftovers and apps flagged as unused. */
  const reclaimable = $derived((appsBytes ?? 0) + safeFilesBytes + (leftoversBytes ?? 0));

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

  const tiles = [
    { id: "apps" as Screen, icon: "📦", tint: "var(--tint-apps)", primary: false },
    { id: "files" as Screen, icon: "📁", tint: "var(--tint-files)", primary: true },
    { id: "leftovers" as Screen, icon: "🧹", tint: "var(--tint-leftovers)", primary: false },
    { id: "map" as Screen, icon: "🗺️", tint: "var(--tint-map)", primary: false },
  ];
</script>

<h1>{t("home.greeting")}</h1>
<p class="muted sub">{t("home.subtitle")}</p>
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
    <button class="btn btn-primary cta" onclick={() => onNavigate("files")}>{t("home.freeNow")}</button>
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
      <div class="ticon" style="background:{tile.tint}">{tile.icon}</div>
      <h3>{t(`home.${tile.id}`)}</h3>
      <p class="muted">{t(`home.${tile.id}Text`)}</p>
      <div class="amount" class:calc={tileAmount(tile.id) === t("home.calculating")}>{tileAmount(tile.id)}</div>
      <button class="btn {tile.primary ? 'btn-primary' : 'btn-ghost'}" onclick={() => onNavigate(tile.id)}>{t(`home.${tile.id}Button`)}</button>
    </div>
  {/each}
</section>

<div class="card tip"><span aria-hidden="true">🛡️</span><span>{@html t("home.quarantineTip", { days: QUARANTINE_DAYS })}</span></div>

<style>
  .sub { margin: 3px 0 10px; }
  .small { font-size: 13px; }
  .hero { display: flex; gap: 32px; align-items: center; padding: 22px 26px; margin-top: 4px; }
  .big { font-size: 40px; font-weight: 700; color: var(--accent); line-height: 1.1; }
  .big.calc { font-size: 26px; color: var(--text-muted); }
  .amount.calc { font-size: 15px; font-weight: 600; color: var(--text-muted); }
  .cta { margin-top: 12px; padding: 10px 18px; font-size: 14px; }
  .disk { flex: 1; }
  .row { display: flex; justify-content: space-between; gap: 12px; }
  .bar { display: flex; height: 16px; border-radius: 8px; overflow: hidden; margin: 10px 0 8px; background: var(--track); }
  .bar i { display: block; height: 100%; }
  .legend { display: flex; gap: 16px; flex-wrap: wrap; }
  .legend b { display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 6px; }
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 14px; margin-top: 16px; }
  .tile { padding: 18px; display: flex; flex-direction: column; min-height: 230px; }
  .tile.primary { border: 2px solid var(--accent); }
  .ticon { width: 44px; height: 44px; border-radius: 12px; display: flex; align-items: center; justify-content: center; font-size: 22px; }
  .tile h3 { font-size: 16px; margin: 12px 0 4px; }
  .tile p { font-size: 12.5px; line-height: 1.45; flex: 1; }
  .amount { font-size: 20px; font-weight: 700; margin: 6px 0 10px; min-height: 26px; }
  .tip { display: flex; gap: 10px; align-items: center; margin-top: 14px; padding: 12px 16px; font-size: 13px; background: var(--tip-bg); border-color: var(--tip-border); }
</style>
