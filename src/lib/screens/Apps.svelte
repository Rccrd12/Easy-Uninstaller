<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import StartupApps from "../components/StartupApps.svelte";
  import UninstallDialog from "../components/UninstallDialog.svelte";
  import { listInstalledApps, usesSampleData } from "../api";
  import { formatBytes, formatDate } from "../format";
  import { t } from "../i18n/index.svelte";
  import type { InstalledApp } from "../types";

  type Filter = "all" | "unused" | "preinstalled" | "recent" | "store";
  const filters: { id: Filter; label: string }[] = [
    { id: "all", label: "apps.filterAll" },
    { id: "unused", label: "apps.filterUnused" },
    { id: "preinstalled", label: "apps.filterPreinstalled" },
    { id: "recent", label: "apps.filterRecent" },
    { id: "store", label: "apps.filterStore" },
  ];

  type Tab = "installed" | "startup";
  let tab = $state<Tab>("installed");
  /** Programs that start with Windows, once the tab has counted them. */
  let startupCount = $state<number | null>(null);

  let apps = $state<InstalledApp[]>([]);
  let status = $state<"loading" | "ready" | "error">("loading");
  let query = $state("");
  let filter = $state<Filter>("all");
  let selected = $state<Set<string>>(new Set());

  function load(refresh = false) {
    status = "loading";
    listInstalledApps(refresh).then(
      (a) => {
        apps = a;
        status = "ready";
      },
      () => (status = "error"),
    );
  }

  $effect(() => load());

  const RECENT_DAYS = 90;
  const recentSince = new Date(Date.now() - RECENT_DAYS * 86_400_000).toISOString().slice(0, 10);

  const matches = (a: InstalledApp, f: Filter): boolean => {
    switch (f) {
      case "unused": return a.neverUsed;
      case "preinstalled": return a.preinstalled;
      case "store": return a.source === "store";
      case "recent": return a.installedOn !== null && a.installedOn >= recentSince;
      default: return true;
    }
  };

  // Only offer filters that would show something (e.g. Windows does not tell us usage yet).
  const shownFilters = $derived(filters.filter((f) => f.id === "all" || f.id === filter || apps.some((a) => matches(a, f.id))));

  const visible = $derived(
    apps
      .filter((a) => a.name.toLowerCase().includes(query.trim().toLowerCase()) && matches(a, filter))
      .toSorted((a, b) => b.sizeBytes - a.sizeBytes),
  );

  function detail(a: InstalledApp): string {
    if (a.lastUsed) return t("apps.lastUsed", { when: a.lastUsed });
    if (a.neverUsed) return t("apps.neverUsed");
    if (a.installedOn) return t("apps.installedOn", { date: formatDate(a.installedOn) });
    return "";
  }
  const total = $derived(apps.reduce((s, a) => s + a.sizeBytes, 0));
  const largest = $derived(Math.max(1, ...apps.map((a) => a.sizeBytes)));
  const selectedBytes = $derived(apps.filter((a) => selected.has(a.id)).reduce((s, a) => s + a.sizeBytes, 0));

  /** The apps being uninstalled, in the order shown; null when the dialog is closed. */
  let uninstalling = $state<InstalledApp[] | null>(null);

  function startUninstall(list: InstalledApp[]) {
    if (list.length) uninstalling = list;
  }

  function finished(removed: string[]) {
    uninstalling = null;
    if (!removed.length) return;
    selected = new Set([...selected].filter((id) => !removed.includes(id)));
    // Show the change at once, then read the real list again.
    apps = apps.filter((a) => !removed.includes(a.id));
    listInstalledApps(true).then((a) => (apps = a), () => {});
  }

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }
</script>

<h1>{t("apps.title")}</h1>
<p class="muted sub">{status === "ready" ? t("apps.summary", { count: apps.length, size: formatBytes(total) }) : "\u00a0"}</p>
{#if usesSampleData()}<SampleBanner />{/if}

<div class="tabs" role="tablist">
  <button role="tab" class="tab" class:on={tab === "installed"} aria-selected={tab === "installed"} onclick={() => (tab = "installed")}>
    <Icon name="apps" filled={tab === "installed"} /> {t("apps.tabInstalled")}
    {#if status === "ready"}<span class="count">{apps.length}</span>{/if}
  </button>
  <button role="tab" class="tab" class:on={tab === "startup"} aria-selected={tab === "startup"} onclick={() => (tab = "startup")}>
    <Icon name="rocket" filled={tab === "startup"} /> {t("apps.tabStartup")}
    {#if startupCount !== null}<span class="count">{startupCount}</span>{/if}
  </button>
</div>

{#if tab === "startup"}
  <StartupApps bind:count={startupCount} />
{:else}

<input class="search" type="search" placeholder={t("apps.search")} bind:value={query} />

<div class="chips">
  {#each shownFilters as f (f.id)}
    <button class="chip" class:on={filter === f.id} onclick={() => (filter = f.id)}>{t(f.label)}</button>
  {/each}
</div>

{#if status === "loading"}
  <p class="muted empty" role="status"><span class="spinner"></span>{t("apps.loading")}</p>
{:else if status === "error"}
  <p class="muted empty" role="alert">{t("apps.error")} <button class="btn btn-ghost" onclick={() => load(true)}>{t("apps.retry")}</button></p>
{:else}
{#each visible as app (app.id)}
  <div class="card row" class:sel={selected.has(app.id)}>
    <input type="checkbox" checked={selected.has(app.id)} onchange={() => toggle(app.id)} aria-label={app.name} />
    {#if app.icon}
      <img class="ico" src={app.icon} alt="" width="32" height="32" />
    {:else}
      <div class="ico" style="background:{app.iconColor}">{app.name[0]}</div>
    {/if}
    <div class="info">
      <div class="name">{app.name}</div>
      <div class="muted meta">{[app.publisher, detail(app)].filter(Boolean).join(" · ")}</div>
    </div>
    {#if app.bloatware}<span class="tag tag-keep">{t("apps.bloatware")}</span>
    {:else if app.neverUsed}<span class="tag tag-check">{t("apps.neverUsed")}</span>{/if}
    <div class="size">
      {#if app.sizeBytes > 0}
        {formatBytes(app.sizeBytes)}
        <div class="meter"><i style="width:{(app.sizeBytes / largest) * 100}%"></i></div>
      {:else}
        <span class="muted">–</span>
      {/if}
    </div>
    <button class="btn btn-ghost" onclick={() => startUninstall([app])}>{t("common.uninstall")}</button>
  </div>
{:else}
  <p class="muted empty">{t("apps.empty")}</p>
{/each}
{/if}

{#if selected.size > 0}
  <div class="card footer">
    <b>{t("apps.selected", { count: selected.size })}</b>
    <span class="muted">· {t("apps.willFree", { size: formatBytes(selectedBytes) })}</span>
    <span class="grow"></span>
    <button class="btn btn-ghost" onclick={() => (selected = new Set())}>{t("common.cancel")}</button>
    <button class="btn btn-danger" onclick={() => startUninstall(apps.filter((a) => selected.has(a.id)).toSorted((a, b) => b.sizeBytes - a.sizeBytes))}>{t("apps.uninstallSelected")}</button>
  </div>
{/if}

{/if}

{#if uninstalling}
  <UninstallDialog apps={uninstalling} onClose={finished} />
{/if}

<style>
  .sub { margin: 3px 0 10px; }
  .tabs { display: flex; gap: 4px; border-bottom: 1px solid var(--border); margin: 6px 0 14px; }
  .tab { display: inline-flex; align-items: center; gap: 7px; padding: 8px 14px; background: none; border: none; border-bottom: 2px solid transparent; margin-bottom: -1px; font-size: 13.5px; color: var(--text-muted); cursor: pointer; border-radius: var(--radius-sm) var(--radius-sm) 0 0; }
  .tab:hover { background: var(--hover); }
  .tab.on { color: var(--accent); border-bottom-color: var(--accent); font-weight: 600; }
  .count { font-size: 11px; font-weight: 600; padding: 1px 7px; border-radius: 9px; background: var(--track); color: var(--text-muted); }
  .tab.on .count { background: var(--accent-soft); color: var(--accent); }
  .search { width: 100%; font-size: 14px; padding: 11px 16px; border-radius: 8px; border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); }
  .chips { display: flex; gap: 8px; margin: 10px 0; flex-wrap: wrap; }
  .chip { padding: 5px 13px; border-radius: 16px; background: var(--surface); border: 1px solid var(--border-strong); font-size: 12.5px; }
  .chip.on { background: var(--accent); color: var(--on-accent); border-color: var(--accent); }
  .row { display: flex; align-items: center; gap: 13px; padding: 10px 14px; margin-bottom: 6px; border-width: 2px; border-color: transparent; }
  .row.sel { border-color: var(--accent); }
  .row input { width: 18px; height: 18px; accent-color: var(--accent); }
  img.ico { object-fit: contain; padding: 3px; background: none; }
  .ico { width: 38px; height: 38px; border-radius: 9px; display: flex; align-items: center; justify-content: center; color: #fff; font-weight: 700; flex: none; }
  .info { min-width: 0; }
  .name, .meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name { font-size: 14.5px; font-weight: 600; }
  .meta { font-size: 12px; }
  .size { margin-left: auto; text-align: right; font-size: 13.5px; width: 100px; flex: none; }
  .meter { height: 4px; background: var(--track); border-radius: 2px; margin-top: 4px; }
  .meter i { display: block; height: 4px; border-radius: 2px; background: var(--accent); }
  .footer { position: sticky; bottom: 0; display: flex; align-items: center; gap: 12px; padding: 12px 16px; margin-top: 8px; box-shadow: var(--shadow-up); }
  .grow { flex: 1; }
  .empty { padding: 24px 0; text-align: center; }
  .spinner { display: inline-block; width: 14px; height: 14px; margin-right: 8px; vertical-align: -2px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
