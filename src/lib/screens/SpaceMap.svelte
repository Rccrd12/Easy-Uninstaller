<script lang="ts">
  import SampleBanner from "../components/SampleBanner.svelte";
  import Icon from "../components/Icon.svelte";
  import { listInstalledApps, scanSpace, usesSampleData } from "../api";
  import { formatBytes } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import { shownPath } from "../paths";
  import type { Screen } from "../screens";
  import { blocksForView, legendFor, spaceBlocks, type MapView } from "../spacemap";
  import { squarify } from "../treemap";
  import type { SpaceBlock, SpaceProgress, SpaceUsage } from "../types";

  let { onNavigate }: { onNavigate: (s: Screen) => void } = $props();

  const views: { id: MapView; label: string }[] = [
    { id: "apps", label: "map.onlyApps" },
    { id: "files", label: "map.onlyFiles" },
    { id: "all", label: "map.everything" },
  ];

  let usage = $state<SpaceUsage | null>(null);
  let all = $state<SpaceBlock[]>([]);
  let loading = $state(true);
  let progress = $state<SpaceProgress | null>(null);
  let error = $state<string | null>(null);
  let view = $state<MapView>("all");
  let selectedId = $state<string | null>(null);
  let width = $state(800);
  let height = $state(460);

  function load(rescan = false) {
    loading = true;
    error = null;
    progress = null;
    selectedId = null;
    // Both run at the same time; the app list is usually already kept from the Apps screen.
    Promise.all([scanSpace({ rescan, onProgress: (p) => (progress = p) }), listInstalledApps(rescan)])
      .then(([u, apps]) => {
        usage = u;
        all = spaceBlocks(u, apps);
      })
      .catch((e) => (error = String(e)))
      .finally(() => (loading = false));
  }

  $effect(() => load());

  const shown = $derived(blocksForView(all, view));
  const rects = $derived(squarify(shown.map((b) => b.sizeBytes), { x: 0, y: 0, w: width, h: height }));
  const legend = $derived(legendFor(shown, view));
  const shownBytes = $derived(shown.reduce((s, b) => s + b.sizeBytes, 0));
  const selected = $derived(shown.find((b) => b.id === selectedId) ?? null);

  const name = (b: SpaceBlock) =>
    b.labelKey ? t(b.labelKey, { count: b.groupedCount.toLocaleString(locale()) }) : (b.label ?? "");
  const share = (bytes: number) =>
    usage && usage.totalBytes > 0
      ? (bytes / usage.totalBytes).toLocaleString(locale(), { style: "percent", maximumFractionDigits: bytes / usage.totalBytes < 0.01 ? 1 : 0 })
      : "";
  /** Where the user can act on a block, if anywhere. */
  const goTo = (b: SpaceBlock): Screen | null =>
    b.category === "apps" && b.id !== "appdata" ? "apps"
    : b.category === "games" ? "apps"
    : b.id === "caches" ? "files"
    : null;

  function pickView(v: MapView) {
    view = v;
    selectedId = null;
  }
</script>

<div class="head">
  <div>
    <h1>{t("map.title")}</h1>
    <p class="muted sub">
      {#if usage && !loading}
        {t("map.summary", { drive: usage.mountPoint, used: formatBytes(usage.totalBytes - usage.freeBytes), total: formatBytes(usage.totalBytes) })}
      {:else}
        {t("map.subtitle")}
      {/if}
    </p>
  </div>
  {#if !loading}
    <button class="btn btn-ghost rescan" onclick={() => load(true)}><Icon name="refresh" />{t("map.rescan")}</button>
  {/if}
</div>
{#if usesSampleData()}<SampleBanner />{/if}

{#if loading}
  <div class="card status" role="status" aria-live="polite">
    <div class="status-title">{t("map.measuring")}</div>
    <div class="sweep" aria-hidden="true"><i></i></div>
    <span class="muted">{progress ? t("map.checked", { count: progress.filesChecked.toLocaleString(locale()) }) : " "}</span>
  </div>
{:else if error}
  <div class="card status row">
    <p>{t("map.failed", { error })}</p>
    <button class="btn btn-primary" onclick={() => load(true)}>{t("map.retry")}</button>
  </div>
{:else}
  <div class="bar">
    <div class="tabs" role="tablist">
      {#each views as v (v.id)}
        <button role="tab" aria-selected={view === v.id} class:on={view === v.id} onclick={() => pickView(v.id)}>{t(v.label)}</button>
      {/each}
    </div>
    <div class="legend">
      {#each legend as c (c)}<span><i class="swatch cat-{c}"></i>{t(`categories.${c}`)}</span>{/each}
    </div>
    <span class="muted total">{t("map.shownTotal", { size: formatBytes(shownBytes) })}</span>
  </div>

  <div class="map" bind:clientWidth={width} bind:clientHeight={height}>
    {#each shown as block, i (block.id)}
      {@const r = rects[i]}
      <button class="block cat-{block.category}" class:picked={selectedId === block.id}
              style="left:{r.x}px;top:{r.y}px;width:{r.w}px;height:{r.h}px"
              title="{name(block)} · {formatBytes(block.sizeBytes)}"
              aria-pressed={selectedId === block.id}
              onclick={() => (selectedId = selectedId === block.id ? null : block.id)}>
        {#if r.w > 84 && r.h > 44}<b>{name(block)}</b><span>{formatBytes(block.sizeBytes)}</span>
        {:else if r.w > 54 && r.h > 26}<span>{formatBytes(block.sizeBytes)}</span>{/if}
      </button>
    {/each}
  </div>

  <div class="card info" aria-live="polite">
    {#if selected}
      {@const target = goTo(selected)}
      <i class="swatch big cat-{selected.category}"></i>
      <div class="info-text">
        <b>{name(selected)}</b>
        <span class="muted">
          {t(`categories.${selected.category}`)} · {formatBytes(selected.sizeBytes)} · {t("map.ofDisk", { share: share(selected.sizeBytes) })}
        </span>
        {#if selected.path}<span class="muted path" title={selected.path}>{shownPath(selected.path)}</span>
        {:else if selected.labelKey}<span class="muted path">{t(`map.hints.${selected.id.startsWith("other:") ? "other" : selected.id}`)}</span>{/if}
      </div>
      {#if target}
        <button class="btn btn-primary" onclick={() => onNavigate(target)}>{t(target === "apps" ? "map.openApps" : "map.openFiles")}</button>
      {/if}
    {:else}
      <span class="muted icon-line"><Icon name="cursor" />{t("map.pickBlock")}</span>
    {/if}
  </div>
{/if}

<style>
  .head { display: flex; align-items: flex-start; gap: 16px; }
  .sub { margin: 3px 0 10px; }
  .rescan { margin-left: auto; }
  .bar { display: flex; align-items: center; gap: 16px; margin-bottom: 10px; flex-wrap: wrap; }
  .tabs { display: inline-flex; background: var(--track); border-radius: 8px; padding: 3px; }
  .tabs button { border: none; background: none; padding: 5px 14px; font-size: 12.5px; border-radius: 6px; }
  .tabs .on { background: var(--raised); font-weight: 600; box-shadow: var(--shadow); }
  .legend { display: flex; gap: 14px; font-size: 12px; flex-wrap: wrap; }
  .total { margin-left: auto; font-size: 12.5px; }
  .swatch { display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 5px; vertical-align: -1px; }
  .swatch.big { width: 16px; height: 16px; border-radius: 4px; margin: 2px 0 0; flex: none; }

  .cat-apps { background: var(--cat-apps); }
  .cat-games { background: var(--cat-games); }
  .cat-personal { background: var(--cat-personal); }
  .cat-reclaimable { background: var(--cat-reclaimable); }
  .cat-system { background: var(--cat-system); }
  .cat-free { background: var(--cat-free); }

  .map { position: relative; height: calc(100vh - 290px); min-height: 280px; }
  .block { position: absolute; border: 2px solid var(--bg); border-radius: 8px; color: #fff; padding: 8px; display: flex; flex-direction: column; justify-content: flex-end; align-items: flex-start; text-align: left; font-size: 12px; overflow: hidden; transition: filter 0.1s; }
  .block b { font-size: 13.5px; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .block span { opacity: 0.92; }
  .block:hover { filter: brightness(1.1); }
  .block.picked { outline: 3px solid var(--text); outline-offset: -3px; }
  .block:focus-visible { outline: 3px solid var(--accent); outline-offset: -3px; }
  .block.cat-free { color: var(--text-muted); background: repeating-linear-gradient(135deg, var(--cat-free), var(--cat-free) 8px, var(--bg) 8px, var(--bg) 10px); }

  .info { display: flex; align-items: center; gap: 12px; padding: 12px 16px; margin-top: 10px; min-height: 64px; font-size: 13px; }
  .info-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .info-text b { font-size: 14px; }
  .path { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; user-select: text; }

  .status { padding: 22px 24px; margin-top: 8px; }
  .status.row { display: flex; align-items: center; gap: 14px; }
  .status.row p { flex: 1; }
  .status-title { font-size: 17px; font-weight: 600; }
  .sweep { height: 10px; border-radius: 5px; background: var(--accent-soft); position: relative; overflow: hidden; margin: 14px 0 10px; }
  .sweep i { position: absolute; inset: 0; width: 30%; border-radius: 5px; background: var(--accent); animation: sweep 1.2s ease-in-out infinite; }
  @keyframes sweep { from { transform: translateX(-100%); } to { transform: translateX(340%); } }
  .note { font-size: 12.5px; margin-top: 14px; }
</style>
