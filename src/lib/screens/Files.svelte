<script lang="ts">
  import SafetyTag from "../components/SafetyTag.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import { listFileGroups } from "../api";
  import { formatBytes } from "../format";
  import { t } from "../i18n/index.svelte";
  import type { FileGroup } from "../types";

  let groups = $state<FileGroup[]>([]);
  let checked = $state<Set<string>>(new Set());
  let focused = $state<FileGroup | null>(null);

  $effect(() => {
    listFileGroups().then((g) => {
      groups = g;
      // Pre-select only what is safe; anything else needs the user's eyes.
      checked = new Set(g.filter((x) => x.safety === "safe").map((x) => x.id));
    });
  });

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
</script>

<div class="layout">
  <div class="col">
    <h1>{t("files.title")}</h1>
    <p class="muted sub">{t("files.subtitle")}</p>
    <SampleBanner />

    {#each sections as section (section.title)}
      {#if section.items.length}
        <h2>{t(section.title)}</h2>
        {#each section.items as group (group.id)}
          <div class="card group" class:focused={focused?.id === group.id} role="button" tabindex="0"
               onclick={() => (focused = group)} onkeydown={(e) => e.key === "Enter" && (focused = group)}>
            <input type="checkbox" checked={checked.has(group.id)} onclick={(e) => e.stopPropagation()} onchange={() => toggle(group.id)} aria-label={group.title} />
            <div class="info">
              <div class="title">{group.title}</div>
              <div class="muted desc">{group.description}</div>
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
      <button class="btn btn-primary" disabled={selectedBytes === 0}>{t("common.moveToQuarantine")}</button>
    </div>
  </div>

  <aside class="card side">
    {#if focused}
      <div class="label">{t("files.details")}</div>
      <h3>{focused.title}</h3>
      <p class="muted desc">{focused.description}</p>
      <dl>
        <dt>{t("files.size")}</dt><dd>{formatBytes(focused.sizeBytes)}</dd>
      </dl>
      <div class="grow"></div>
      <button class="btn btn-primary wide">{t("common.moveToQuarantine")} ({formatBytes(focused.sizeBytes)})</button>
      <button class="btn btn-ghost wide">{t("common.openFolder")}</button>
    {:else}
      <p class="muted">{t("files.pickGroup")}</p>
    {/if}
  </aside>
</div>

<style>
  .layout { display: flex; gap: 18px; align-items: flex-start; }
  .col { flex: 1; min-width: 0; }
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
  .footer .btn:disabled { opacity: 0.5; cursor: default; }
  .grow { flex: 1; }
  .side { width: 300px; flex: none; padding: 18px; min-height: 420px; display: flex; flex-direction: column; position: sticky; top: 0; }
  .label { font-size: 11px; font-weight: 600; letter-spacing: 0.3px; color: var(--text-muted); text-transform: uppercase; }
  .side h3 { font-size: 17px; margin: 6px 0 4px; }
  dl { display: grid; grid-template-columns: 1fr auto; font-size: 12.5px; margin-top: 10px; border-top: 1px solid #eee; padding-top: 8px; }
  dt { color: var(--text-muted); }
  dd { font-weight: 600; }
  .wide { width: 100%; padding: 10px; margin-top: 8px; }
</style>
