<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import NewPcGuide from "../components/NewPcGuide.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import StartupApps from "../components/StartupApps.svelte";
  import UninstallDialog from "../components/UninstallDialog.svelte";
  import UninstalledApps from "../components/UninstalledApps.svelte";
  import AppCaches from "../components/AppCaches.svelte";
  import AppDetail, { appSpace } from "../components/AppDetail.svelte";
  import { forgetUninstalled, listAppCaches, listInstalledApps, listUninstalledApps, usesSampleData } from "../api";
  import { formatBytes, formatDate } from "../format";
  import { t } from "../i18n/index.svelte";
  import { saveSettings, settings } from "../settings.svelte";
  import type { AppCache, InstalledApp, UninstalledApp } from "../types";

  type Filter = "all" | "unused" | "preinstalled" | "recent" | "store";
  const filters: { id: Filter; label: string }[] = [
    { id: "all", label: "apps.filterAll" },
    { id: "unused", label: "apps.filterUnused" },
    { id: "preinstalled", label: "apps.filterPreinstalled" },
    { id: "recent", label: "apps.filterRecent" },
    { id: "store", label: "apps.filterStore" },
  ];

  type Tab = "installed" | "cache" | "startup" | "uninstalled";
  let tab = $state<Tab>("installed");
  /** Programs that start with Windows, once the tab has counted them. */
  let startupCount = $state<number | null>(null);

  let apps = $state<InstalledApp[]>([]);
  let status = $state<"loading" | "ready" | "error">("loading");
  let query = $state("");
  let filter = $state<Filter>("all");
  let selected = $state<Set<string>>(new Set());
  /** App caches by id; measured after the app list, in the background. */
  let caches = $state<Map<string, AppCache>>(new Map());
  let measuring = $state(true);
  /** The app shown in the side panel. */
  let openId = $state<string | null>(null);
  const opened = $derived(apps.find((a) => a.id === openId) ?? null);

  /** A cache this small is not worth a hint in the list. */
  const CACHE_HINT_MIN = 20 * 1024 * 1024;

  function loadCaches(refresh = false) {
    measuring = true;
    listAppCaches(refresh).then(
      (list) => {
        caches = new Map(list.map((c) => [c.id, c]));
        measuring = false;
        rerank();
      },
      () => (measuring = false),
    );
  }

  /**
   * Sizes the list is sorted by, taken when apps or caches are read, so a row does
   * not jump away right after its cache is cleared.
   */
  let rank = $state<Map<string, number>>(new Map());
  const rerank = () => (rank = new Map(apps.map((a) => [a.id, spaceOf(a)])));

  const cacheOf = (a: InstalledApp) => (a.cacheId ? caches.get(a.cacheId) : undefined);
  const spaceOf = (a: InstalledApp) => appSpace(a, cacheOf(a)).total;
  const cacheToClear = (a: InstalledApp) => {
    const c = cacheOf(a);
    return c && c.cacheBytes >= CACHE_HINT_MIN ? c.cacheBytes : 0;
  };

  /** Cache that can be emptied, over the installed apps we know a cache for. */
  const cacheTotal = $derived(apps.reduce((s, a) => s + (cacheOf(a)?.cacheBytes ?? 0), 0));

  function cleared(cache: AppCache, freed: number) {
    caches = new Map(caches).set(cache.id, cache);
    // A Store app's size counts its data folder, cache included: take off what went
    // now, then read the real sizes again.
    apps = apps.map((a) => (a.source === "store" && a.cacheId === cache.id ? { ...a, sizeBytes: Math.max(0, a.sizeBytes - freed) } : a));
    listInstalledApps(true).then((a) => (apps = a), () => {});
  }

  function load(refresh = false) {
    status = "loading";
    listInstalledApps(refresh).then(
      (a) => {
        apps = a;
        rerank();
        status = "ready";
      },
      () => (status = "error"),
    );
  }

  $effect(() => {
    load();
    loadCaches();
  });

  /** The "New PC" guided cleanup is open. */
  let guide = $state(false);

  let uninstalled = $state<UninstalledApp[]>([]);
  const loadUninstalled = () => listUninstalledApps().then((l) => (uninstalled = l), () => {});
  $effect(() => {
    loadUninstalled();
  });
  const installedNames = $derived(new Set(apps.map((a) => a.name.toLowerCase())));

  function forget(id: string) {
    uninstalled = uninstalled.filter((u) => u.id !== id);
    forgetUninstalled([id]).catch(loadUninstalled);
  }

  /** Apps that came with Windows or the PC and that the New PC cleanup suggests. */
  const newPcApps = $derived(apps.filter((a) => a.newPc));
  const newPcBytes = $derived(newPcApps.reduce((s, a) => s + a.sizeBytes, 0));
  const showNewPc = $derived(status === "ready" && newPcApps.length > 0 && (!settings.newPcDismissed || filter === "preinstalled"));

  function notNow() {
    settings.newPcDismissed = true;
    saveSettings();
  }

  /** Apps removed by the New PC cleanup: off the list at once, then read again. */
  function removedByGuide(removed: string[]) {
    apps = apps.filter((a) => !removed.includes(a.id));
    selected = new Set([...selected].filter((id) => !removed.includes(id)));
    listInstalledApps(true).then((a) => (apps = a), () => {});
    loadUninstalled();
  }

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
      .toSorted((a, b) => (rank.get(b.id) ?? spaceOf(b)) - (rank.get(a.id) ?? spaceOf(a))),
  );

  function detail(a: InstalledApp): string {
    if (a.lastUsed) return t("apps.lastUsed", { when: a.lastUsed });
    if (a.neverUsed) return t("apps.neverUsed");
    if (a.installedOn) return t("apps.installedOn", { date: formatDate(a.installedOn) });
    return "";
  }

  // Mirrors SCOPES in src-tauri/src/apps/registry.rs: app ids are `reg:<scope>:<key name>`.
  const UNINSTALL_KEYS: Record<string, string> = {
    hklm: "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    hklm32: "HKLM\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    hkcu: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
  };

  /** Where Windows lists the app: its Uninstall registry key, or its Store package name. */
  function listedAt(a: InstalledApp): string {
    const m = /^reg:(\w+):(.+)$/.exec(a.id);
    if (m && UNINSTALL_KEYS[m[1]]) return `${UNINSTALL_KEYS[m[1]]}\\${m[2]}`;
    return a.id.replace(/^store:/, "");
  }

  /** The technical details line: kind of installer, version, folder and registry key. */
  function technical(a: InstalledApp): string {
    return [
      t(`technical.sources.${a.source}`),
      a.version && t("technical.version", { version: a.version }),
      a.location,
      listedAt(a),
    ].filter(Boolean).join(" · ");
  }
  const total = $derived(apps.reduce((s, a) => s + spaceOf(a), 0));
  const largest = $derived(Math.max(1, ...apps.map(spaceOf)));
  const selectedBytes = $derived(apps.filter((a) => selected.has(a.id)).reduce((s, a) => s + spaceOf(a), 0));

  /** The apps being uninstalled, in the order shown; null when the dialog is closed. */
  let uninstalling = $state<InstalledApp[] | null>(null);

  function startUninstall(list: InstalledApp[]) {
    if (list.length) uninstalling = list;
  }

  function finished(removed: string[]) {
    uninstalling = null;
    if (!removed.length) return;
    if (openId && removed.includes(openId)) openId = null;
    selected = new Set([...selected].filter((id) => !removed.includes(id)));
    // Show the change at once, then read the real list again.
    apps = apps.filter((a) => !removed.includes(a.id));
    listInstalledApps(true).then((a) => (apps = a), () => {});
    loadCaches(true);
    loadUninstalled();
  }

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }
</script>

{#if guide}
  <NewPcGuide {apps} onBack={() => (guide = false)} onShowUninstalled={() => { guide = false; tab = "uninstalled"; }} onRemoved={removedByGuide} />
{:else}
<h1>{t("apps.title")}</h1>
<p class="muted sub">{status === "ready" ? t("apps.summary", { count: apps.length, size: formatBytes(total) }) : "\u00a0"}</p>
{#if usesSampleData()}<SampleBanner />{/if}

<div class="tabs" role="tablist">
  <button role="tab" class="tab" class:on={tab === "installed"} aria-selected={tab === "installed"} onclick={() => (tab = "installed")}>
    <Icon name="apps" filled={tab === "installed"} /> {t("apps.tabInstalled")}
    {#if status === "ready"}<span class="count">{apps.length}</span>{/if}
  </button>
  <button role="tab" class="tab" class:on={tab === "cache"} aria-selected={tab === "cache"} onclick={() => (tab = "cache")}>
    <Icon name="broom" filled={tab === "cache"} /> {t("apps.tabCache")}
    {#if cacheTotal > 0}<span class="count">{formatBytes(cacheTotal)}</span>{/if}
  </button>
  <button role="tab" class="tab" class:on={tab === "startup"} aria-selected={tab === "startup"} onclick={() => (tab = "startup")}>
    <Icon name="rocket" filled={tab === "startup"} /> {t("apps.tabStartup")}
    {#if startupCount !== null}<span class="count">{startupCount}</span>{/if}
  </button>
  <button role="tab" class="tab" class:on={tab === "uninstalled"} aria-selected={tab === "uninstalled"} onclick={() => (tab = "uninstalled")}>
    <Icon name="history" filled={tab === "uninstalled"} /> {t("apps.tabUninstalled")}
    {#if uninstalled.length}<span class="count">{uninstalled.length}</span>{/if}
  </button>
</div>

{#if tab === "cache"}
  <AppCaches {apps} {caches} {measuring} onCleared={cleared} />
{:else if tab === "startup"}
  <StartupApps bind:count={startupCount} />
{:else if tab === "uninstalled"}
  <UninstalledApps list={uninstalled} {installedNames} onForget={forget} />
{:else}

<div class="layout">
<div class="list">
{#if showNewPc}
  <div class="card newpc">
    <span class="tile"><Icon name="laptop" size={24} /></span>
    <div class="grow">
      <b>{t("newPc.bannerTitle")}</b>
      <div class="muted">{t("newPc.bannerText", { count: newPcApps.length, size: formatBytes(newPcBytes) })}</div>
    </div>
    {#if !settings.newPcDismissed}<button class="btn btn-ghost" onclick={notNow}>{t("newPc.later")}</button>{/if}
    <button class="btn btn-primary" onclick={() => (guide = true)}>{t("newPc.start")}</button>
  </div>
{/if}

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
  {@const hint = cacheToClear(app)}
  {@const size = spaceOf(app)}
  <div class="card row" class:sel={selected.has(app.id)} class:open={openId === app.id}>
    <input type="checkbox" checked={selected.has(app.id)} onchange={() => toggle(app.id)} aria-label={app.name} />
    <button class="open-btn" onclick={() => (openId = openId === app.id ? null : app.id)} aria-label={t("apps.detail.show", { app: app.name })} aria-expanded={openId === app.id}></button>
    {#if app.icon}
      <img class="ico" src={app.icon} alt="" width="32" height="32" />
    {:else}
      <div class="ico" style="background:{app.iconColor}">{app.name[0]}</div>
    {/if}
    <div class="info">
      <div class="name">{app.name}</div>
      {#if hint}
        <div class="meta cache"><Icon name="broom" size={13} />{t("apps.cacheHint", { size: formatBytes(hint) })}</div>
      {:else}
        <div class="muted meta">{[app.publisher, detail(app)].filter(Boolean).join(" · ")}</div>
      {/if}
      {#if settings.technical}{@const tech = technical(app)}<div class="muted meta tech" title={tech.replaceAll(" · ", "\n")}>{tech}</div>{/if}
    </div>
    {#if opened}<!-- No room for tags next to the panel. -->
    {:else if app.bloatware}<span class="tag tag-check">{t("apps.bloatware")}</span>
    {:else if app.neverUsed}<span class="tag tag-check">{t("apps.neverUsed")}</span>{/if}
    <div class="size">
      {#if size > 0}
        {formatBytes(size)}
        <div class="meter"><i style="width:{(size / largest) * 100}%"></i></div>
      {:else}
        <span class="muted">–</span>
      {/if}
    </div>
    {#if !opened}<button class="btn btn-ghost" onclick={() => startUninstall([app])}>{t("common.uninstall")}</button>{/if}
  </div>
{:else}
  <p class="muted empty">{t("apps.empty")}</p>
{/each}
{/if}

{#if selected.size > 0 && tab === "installed"}
  <div class="card footer">
    <b>{t("apps.selected", { count: selected.size })}</b>
    <span class="muted">· {t("apps.willFree", { size: formatBytes(selectedBytes) })}</span>
    <span class="grow"></span>
    <button class="btn btn-ghost" onclick={() => (selected = new Set())}>{t("common.cancel")}</button>
    <button class="btn btn-danger" onclick={() => startUninstall(apps.filter((a) => selected.has(a.id)).toSorted((a, b) => spaceOf(b) - spaceOf(a)))}>{t("apps.uninstallSelected")}</button>
  </div>
{/if}
</div>

{#if opened}
  <AppDetail app={opened} cache={cacheOf(opened)} {measuring} onCleared={cleared} onUninstall={() => startUninstall([opened])} onClose={() => (openId = null)} />
{/if}
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
  .newpc { display: flex; align-items: center; gap: 14px; padding: 14px 16px; margin-bottom: 12px; border-color: var(--accent); background: var(--accent-soft); }
  .newpc .muted { font-size: 13px; margin-top: 2px; }
  .tile { width: 42px; height: 42px; border-radius: 10px; display: flex; align-items: center; justify-content: center; background: var(--surface); color: var(--accent); flex: none; }
  .search { width: 100%; font-size: 14px; padding: 11px 16px; border-radius: 8px; border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); }
  .chips { display: flex; gap: 8px; margin: 10px 0; flex-wrap: wrap; }
  .chip { padding: 5px 13px; border-radius: 16px; background: var(--surface); border: 1px solid var(--border-strong); font-size: 12.5px; }
  .chip.on { background: var(--accent); color: var(--on-accent); border-color: var(--accent); }
  .layout { display: flex; gap: 18px; align-items: flex-start; }
  .list { flex: 1; min-width: 0; }
  .row { position: relative; display: flex; align-items: center; gap: 13px; padding: 10px 14px; margin-bottom: 6px; border-width: 2px; border-color: transparent; }
  .row:hover { border-color: var(--border-strong); }
  .row.sel, .row.open { border-color: var(--accent); }
  .row.open { background: var(--accent-soft); }
  /* The whole row opens the side panel; the checkbox and buttons sit above it. */
  .open-btn { position: absolute; inset: 0; background: none; border: none; border-radius: var(--radius); cursor: pointer; }
  .open-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .row > :not(.open-btn) { position: relative; }
  .row > .info, .row > .ico, .row > .size, .row > .tag { pointer-events: none; }
  .row input { width: 18px; height: 18px; accent-color: var(--accent); }
  img.ico { object-fit: contain; padding: 3px; background: none; }
  .ico { width: 38px; height: 38px; border-radius: 9px; display: flex; align-items: center; justify-content: center; color: #fff; font-weight: 700; flex: none; }
  .info { min-width: 0; }
  .name, .meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name { font-size: 14.5px; font-weight: 600; }
  .meta { font-size: 12px; }
  .tech { font-size: 11.5px; margin-top: 1px; user-select: text; }
  .meta.cache { color: var(--cat-reclaimable); display: flex; align-items: center; gap: 5px; font-weight: 500; }
  .size { margin-left: auto; text-align: right; font-size: 13.5px; width: 100px; flex: none; }
  .meter { height: 4px; background: var(--track); border-radius: 2px; margin-top: 4px; }
  .meter i { display: block; height: 4px; border-radius: 2px; background: var(--accent); }
  .footer { position: sticky; bottom: 0; display: flex; align-items: center; gap: 12px; padding: 12px 16px; margin-top: 8px; box-shadow: var(--shadow-up); }
  .grow { flex: 1; }
  .empty { padding: 24px 0; text-align: center; }
  .spinner { display: inline-block; width: 14px; height: 14px; margin-right: 8px; vertical-align: -2px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
