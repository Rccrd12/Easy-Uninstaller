<script lang="ts">
  import SampleBanner from "../components/SampleBanner.svelte";
  import { listInstalledApps } from "../api";
  import { formatBytes } from "../format";
  import { t } from "../i18n";
  import type { InstalledApp } from "../types";

  type Filter = "all" | "unused" | "preinstalled" | "recent" | "store";
  const filters: { id: Filter; label: string }[] = [
    { id: "all", label: "apps.filterAll" },
    { id: "unused", label: "apps.filterUnused" },
    { id: "preinstalled", label: "apps.filterPreinstalled" },
    { id: "recent", label: "apps.filterRecent" },
    { id: "store", label: "apps.filterStore" },
  ];

  let apps = $state<InstalledApp[]>([]);
  let query = $state("");
  let filter = $state<Filter>("all");
  let selected = $state<Set<string>>(new Set());

  $effect(() => {
    listInstalledApps().then((a) => (apps = a));
  });

  const matches = (a: InstalledApp): boolean => {
    switch (filter) {
      case "unused": return a.lastUsed === null;
      case "preinstalled": return a.preinstalled;
      case "store": return a.source === "store";
      case "recent": return a.installedOn !== null && a.installedOn >= "2025-01-01";
      default: return true;
    }
  };

  const visible = $derived(
    apps
      .filter((a) => a.name.toLowerCase().includes(query.trim().toLowerCase()) && matches(a))
      .toSorted((a, b) => b.sizeBytes - a.sizeBytes),
  );
  const total = $derived(apps.reduce((s, a) => s + a.sizeBytes, 0));
  const largest = $derived(Math.max(1, ...apps.map((a) => a.sizeBytes)));
  const selectedBytes = $derived(apps.filter((a) => selected.has(a.id)).reduce((s, a) => s + a.sizeBytes, 0));

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }
</script>

<h1>{t("apps.title")}</h1>
<p class="muted sub">{t("apps.summary", { count: apps.length, size: formatBytes(total) })}</p>
<SampleBanner />

<input class="search" type="search" placeholder={t("apps.search")} bind:value={query} />

<div class="chips">
  {#each filters as f (f.id)}
    <button class="chip" class:on={filter === f.id} onclick={() => (filter = f.id)}>{t(f.label)}</button>
  {/each}
</div>

{#each visible as app (app.id)}
  <div class="card row" class:sel={selected.has(app.id)}>
    <input type="checkbox" checked={selected.has(app.id)} onchange={() => toggle(app.id)} aria-label={app.name} />
    <div class="ico" style="background:{app.iconColor}">{app.name[0]}</div>
    <div class="info">
      <div class="name">{app.name}</div>
      <div class="muted meta">{app.publisher} · {app.lastUsed ? t("apps.lastUsed", { when: app.lastUsed }) : t("apps.neverUsed")}</div>
    </div>
    {#if app.bloatware}<span class="tag tag-keep">{t("apps.bloatware")}</span>
    {:else if app.lastUsed === null}<span class="tag tag-check">{t("apps.neverUsed")}</span>{/if}
    <div class="size">
      {formatBytes(app.sizeBytes)}
      <div class="meter"><i style="width:{(app.sizeBytes / largest) * 100}%"></i></div>
    </div>
    <button class="btn btn-ghost">{t("common.uninstall")}</button>
  </div>
{:else}
  <p class="muted empty">{t("apps.empty")}</p>
{/each}

{#if selected.size > 0}
  <div class="card footer">
    <b>{t("apps.selected", { count: selected.size })}</b>
    <span class="muted">· {t("apps.willFree", { size: formatBytes(selectedBytes) })}</span>
    <span class="grow"></span>
    <button class="btn btn-ghost" onclick={() => (selected = new Set())}>{t("common.cancel")}</button>
    <button class="btn btn-danger">{t("apps.uninstallSelected")}</button>
  </div>
{/if}

<style>
  .sub { margin: 3px 0 10px; }
  .search { width: 100%; font-size: 14px; padding: 11px 16px; border-radius: 8px; border: 1px solid var(--border-strong); background: var(--surface); }
  .chips { display: flex; gap: 8px; margin: 10px 0; flex-wrap: wrap; }
  .chip { padding: 5px 13px; border-radius: 16px; background: var(--surface); border: 1px solid #d9d9d9; font-size: 12.5px; }
  .chip.on { background: var(--accent); color: #fff; border-color: var(--accent); }
  .row { display: flex; align-items: center; gap: 13px; padding: 10px 14px; margin-bottom: 6px; border-width: 2px; border-color: transparent; }
  .row.sel { border-color: var(--accent); }
  .row input { width: 18px; height: 18px; accent-color: var(--accent); }
  .ico { width: 38px; height: 38px; border-radius: 9px; display: flex; align-items: center; justify-content: center; color: #fff; font-weight: 700; flex: none; }
  .info { min-width: 0; }
  .name { font-size: 14.5px; font-weight: 600; }
  .meta { font-size: 12px; }
  .size { margin-left: auto; text-align: right; font-size: 13.5px; width: 100px; flex: none; }
  .meter { height: 4px; background: #e5e5e5; border-radius: 2px; margin-top: 4px; }
  .meter i { display: block; height: 4px; border-radius: 2px; background: var(--accent); }
  .footer { position: sticky; bottom: 0; display: flex; align-items: center; gap: 12px; padding: 12px 16px; margin-top: 8px; box-shadow: 0 -4px 12px rgba(0, 0, 0, 0.05); }
  .grow { flex: 1; }
  .empty { padding: 24px 0; text-align: center; }
</style>
