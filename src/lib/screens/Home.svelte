<script lang="ts">
  import QuickClean from "../components/QuickClean.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import { getSystemDisk, listFileGroups, listInstalledApps, listLeftovers } from "../api";
  import { quarantine } from "../settings.svelte";
  import { formatBytes } from "../format";
  import { t } from "../i18n/index.svelte";
  import type { Screen } from "../screens";
  import type { DiskInfo } from "../types";

  let { onNavigate }: { onNavigate: (s: Screen) => void } = $props();

  let disk = $state<DiskInfo | null>(null);
  let appsBytes = $state(0);
  let filesBytes = $state(0);
  let leftoversBytes = $state(0);
  let quickClean = $state(false);

  function refreshAfterClean(changed: boolean) {
    quickClean = false;
    if (!changed) return;
    getSystemDisk().then((d) => (disk = d));
    listFileGroups().then((g) => (filesBytes = g.reduce((s, x) => s + x.sizeBytes, 0)));
  }

  $effect(() => {
    getSystemDisk().then((d) => (disk = d));
    // Reclaimable from apps = apps flagged as unused or bloatware.
    listInstalledApps().then((apps) => (appsBytes = apps.filter((a) => a.bloatware || a.neverUsed).reduce((s, a) => s + a.sizeBytes, 0)));
    // Review-only groups (unknown folders) are explained, not offered for removal.
    listFileGroups().then((g) => (filesBytes = g.filter((x) => !x.reviewOnly).reduce((s, x) => s + x.sizeBytes, 0)));
    listLeftovers().then((l) => (leftoversBytes = l.reduce((s, x) => s + x.sizeBytes, 0)));
  });

  const reclaimable = $derived(appsBytes + filesBytes + leftoversBytes);
  const usedPct = $derived(disk ? ((disk.totalBytes - disk.freeBytes) / disk.totalBytes) * 100 : 0);
  const reclaimPct = $derived(disk ? (reclaimable / disk.totalBytes) * 100 : 0);
  const lowSpace = $derived(disk ? disk.freeBytes / disk.totalBytes < 0.15 : false);

  const tiles = $derived([
    { id: "apps" as Screen, icon: "📦", tint: "var(--tint-apps)", amount: appsBytes, primary: false },
    { id: "files" as Screen, icon: "📁", tint: "var(--tint-files)", amount: filesBytes, primary: true },
    { id: "leftovers" as Screen, icon: "🧹", tint: "var(--tint-leftovers)", amount: leftoversBytes, primary: false },
    { id: "map" as Screen, icon: "🗺️", tint: "var(--tint-map)", amount: null, primary: false },
  ]);
</script>

<h1>{t("home.greeting")}</h1>
<p class="muted sub">{t("home.subtitle")}</p>
<SampleBanner />

<section class="card hero">
  <div>
    <div class="muted small">{t("home.canFree")}</div>
    <div class="big">{formatBytes(reclaimable)}</div>
    <div class="ctas">
      <button class="btn btn-primary cta" onclick={() => (quickClean = true)}>{t("home.quickClean")}</button>
      <button class="btn btn-ghost cta" onclick={() => onNavigate("files")}>{t("home.seeAll")}</button>
    </div>
    <div class="muted small hint">{t("home.quickCleanHint")}</div>
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
        <span><b style="background:var(--cat-reclaimable)"></b>{t("categories.reclaimable")} {formatBytes(reclaimable)}</span>
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
      <div class="amount">{tile.amount === null ? "" : formatBytes(tile.amount)}</div>
      <button class="btn {tile.primary ? 'btn-primary' : 'btn-ghost'}" onclick={() => onNavigate(tile.id)}>{t(`home.${tile.id}Button`)}</button>
    </div>
  {/each}
</section>

<div class="card tip"><span aria-hidden="true">🛡️</span><span>{@html t("home.quarantineTip", { days: quarantine.keepDays })}</span></div>

{#if quickClean}
  <QuickClean
    onClose={refreshAfterClean}
    onDetails={() => onNavigate("files")}
    onSettings={() => onNavigate("settings")}
  />
{/if}

<style>
  .sub { margin: 3px 0 10px; }
  .small { font-size: 13px; }
  .hero { display: flex; gap: 32px; align-items: center; padding: 22px 26px; margin-top: 4px; }
  .big { font-size: 40px; font-weight: 700; color: var(--accent); line-height: 1.1; }
  .ctas { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .cta { padding: 10px 18px; font-size: 14px; }
  .hint { margin-top: 6px; font-size: 12px; }
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
