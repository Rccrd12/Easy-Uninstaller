<script lang="ts">
  import SampleBanner from "../components/SampleBanner.svelte";
  import { getSpaceMap } from "../api";
  import { formatBytes } from "../format";
  import { t } from "../i18n/index.svelte";
  import { squarify } from "../treemap";
  import type { MapCategory, SpaceBlock } from "../types";

  type View = "apps" | "files" | "all";
  const views: { id: View; label: string }[] = [
    { id: "apps", label: "map.onlyApps" },
    { id: "files", label: "map.onlyFiles" },
    { id: "all", label: "map.everything" },
  ];
  const categories: MapCategory[] = ["apps", "games", "personal", "reclaimable", "system"];
  const appCategories: MapCategory[] = ["apps", "games"];

  let blocks = $state<SpaceBlock[]>([]);
  let view = $state<View>("all");
  let width = $state(800);
  let height = $state(460);

  $effect(() => {
    getSpaceMap().then((b) => (blocks = b));
  });

  const shown = $derived(
    blocks.filter((b) => view === "all" || (view === "apps") === appCategories.includes(b.category)),
  );
  const rects = $derived(squarify(shown.map((b) => b.sizeBytes), { x: 0, y: 0, w: width, h: height }));
</script>

<div class="head">
  <div>
    <h1>{t("map.title")}</h1>
    <p class="muted sub">{t("map.subtitle")}</p>
  </div>
  <div class="tabs" role="tablist">
    {#each views as v (v.id)}
      <button role="tab" aria-selected={view === v.id} class:on={view === v.id} onclick={() => (view = v.id)}>{t(v.label)}</button>
    {/each}
  </div>
</div>
<SampleBanner />

<div class="legend">
  {#each categories as c (c)}<span><i style="background:var(--cat-{c})"></i>{t(`categories.${c}`)}</span>{/each}
</div>

<div class="map" bind:clientWidth={width} bind:clientHeight={height}>
  {#each shown as block, i (block.id)}
    {@const r = rects[i]}
    <button class="block" style="left:{r.x}px;top:{r.y}px;width:{r.w}px;height:{r.h}px;background:var(--cat-{block.category})"
            title="{block.label} · {formatBytes(block.sizeBytes)}">
      {#if r.w > 90 && r.h > 44}<b>{block.label}</b><span>{formatBytes(block.sizeBytes)}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .head { display: flex; align-items: flex-start; gap: 16px; }
  .sub { margin: 3px 0 10px; }
  .tabs { margin-left: auto; display: inline-flex; background: #e6e8eb; border-radius: 8px; padding: 3px; }
  .tabs button { border: none; background: none; padding: 5px 14px; font-size: 12.5px; border-radius: 6px; }
  .tabs .on { background: var(--surface); font-weight: 600; box-shadow: var(--shadow); }
  .legend { display: flex; gap: 14px; font-size: 12px; margin-bottom: 8px; flex-wrap: wrap; }
  .legend i { display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 5px; }
  .map { position: relative; height: calc(100vh - 230px); min-height: 300px; }
  .block { position: absolute; border: 2px solid var(--bg); border-radius: 8px; color: #fff; padding: 9px; display: flex; flex-direction: column; justify-content: flex-end; align-items: flex-start; text-align: left; font-size: 12px; overflow: hidden; transition: filter 0.1s; }
  .block:hover { filter: brightness(1.1); }
  .block b { font-size: 14px; }
</style>
