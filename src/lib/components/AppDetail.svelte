<script lang="ts" module>
  import type { AppCache, InstalledApp } from "../types";

  export interface AppSpace {
    program: number;
    data: number;
    cache: number;
    total: number;
  }

  /**
   * What an app takes in all: its program plus, when we know them, its data and cache.
   * A Store app's size already counts its data folder (cache included).
   */
  export function appSpace(app: InstalledApp, cache: AppCache | undefined): AppSpace {
    const c = cache?.cacheBytes ?? 0;
    const data = cache?.dataBytes ?? 0;
    if (app.source === "store") {
      const total = Math.max(app.sizeBytes, c + data);
      return { program: total - c - data, data, cache: c, total };
    }
    return { program: app.sizeBytes, data, cache: c, total: app.sizeBytes + data + c };
  }
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";
  import { clearAppCache } from "../api";
  import { fileCount, formatBytes, formatDate } from "../format";
  import { t } from "../i18n/index.svelte";

  let {
    app,
    cache,
    measuring,
    onCleared,
    onUninstall,
    onClose,
  }: {
    app: InstalledApp;
    cache: AppCache | undefined;
    /** The caches are still being measured. */
    measuring: boolean;
    /** The cache measured again, and the bytes freed. */
    onCleared: (cache: AppCache, freed: number) => void;
    onUninstall: () => void;
    onClose: () => void;
  } = $props();

  type State =
    | { kind: "idle" }
    | { kind: "clearing" }
    | { kind: "running" }
    | { kind: "done"; freed: number; skipped: number }
    | { kind: "error" };
  let phase = $state<State>({ kind: "idle" });

  // A different app: start over. (Through a derived id: the same app comes back as a
  // new object when the list is read again, and that must not hide the result.)
  const appId = $derived(app.id);
  $effect(() => {
    void appId;
    phase = { kind: "idle" };
  });

  const space = $derived(appSpace(app, cache));
  const known = $derived(app.cacheId !== null);
  const parts = $derived(
    [
      { key: "program", label: t("apps.detail.program"), bytes: space.program, color: "var(--cat-apps)" },
      { key: "data", label: t("apps.detail.data"), bytes: space.data, color: "var(--cat-games)" },
      { key: "cache", label: t("apps.detail.cache"), bytes: space.cache, color: "var(--cat-reclaimable)" },
    ].filter((p) => p.key === "program" || cache),
  );

  async function clear() {
    if (!app.cacheId) return;
    phase = { kind: "clearing" };
    try {
      const out = await clearAppCache(app.cacheId);
      if (out.running) {
        phase = { kind: "running" };
        return;
      }
      phase = { kind: "done", freed: out.report.freedBytes, skipped: out.report.skippedCount };
      if (out.cache) onCleared(out.cache, out.report.freedBytes);
    } catch {
      phase = { kind: "error" };
    }
  }
</script>

<aside class="card detail" aria-label={app.name}>
  <button class="close" onclick={onClose} aria-label={t("common.close")}><Icon name="dismiss" /></button>
  <div class="head">
    {#if app.icon}
      <img class="ico" src={app.icon} alt="" width="44" height="44" />
    {:else}
      <div class="ico" style="background:{app.iconColor}">{app.name[0]}</div>
    {/if}
    <div class="title">
      <h2>{app.name}</h2>
      {#if app.publisher}<div class="muted small">{app.publisher}</div>{/if}
    </div>
  </div>

  <div class="sec">{t("apps.detail.spaceUsed")}</div>
  <div class="big">{formatBytes(space.total)}</div>
  {#if space.total > 0}
    <div class="bar" aria-hidden="true">
      {#each parts as p (p.key)}
        {#if p.bytes > 0}<i style="flex:{p.bytes};background:{p.color}"></i>{/if}
      {/each}
    </div>
  {/if}
  {#each parts as p (p.key)}
    <div class="legend">
      <b class="dot" style="background:{p.color}"></b>
      <span class="grow">{p.label}</span>
      <span>{formatBytes(p.bytes)}</span>
    </div>
  {/each}
  {#if known && measuring && !cache}
    <div class="legend muted"><span class="spinner"></span><span class="grow">{t("apps.detail.cache")}</span><span>{t("apps.detail.measuring")}</span></div>
  {/if}

  {#if known && cache}
    {#if phase.kind === "done"}
      <div class="box ok" role="status">
        <div class="icon-line"><Icon name="checkCircle" size={18} /><b>{t("apps.detail.cleared", { size: formatBytes(phase.freed) })}</b></div>
        {#if phase.skipped > 0}<p>{t("apps.detail.skipped", { count: fileCount(phase.skipped) })}</p>{/if}
      </div>
    {:else if cache.cacheBytes > 0}
      {#if phase.kind === "running" || (phase.kind === "idle" && cache.running)}
        <div class="box warn" role="alert">
          <div class="icon-line"><Icon name="warning" size={18} /><b>{t("apps.detail.running", { app: app.name })}</b></div>
        </div>
      {:else if phase.kind === "error"}
        <div class="box warn" role="alert"><div class="icon-line"><Icon name="warning" size={18} /><b>{t("apps.detail.error")}</b></div></div>
      {/if}
      <button class="btn btn-primary wide" onclick={clear} disabled={phase.kind === "clearing"}>
        {#if phase.kind === "clearing"}
          <span class="spinner light"></span>{t("apps.detail.clearing")}
        {:else if phase.kind === "running" || phase.kind === "error" || cache.running}
          <Icon name="refresh" />{t("apps.detail.tryAgain")}
        {:else}
          <Icon name="broom" />{t("apps.detail.clearCache", { size: formatBytes(cache.cacheBytes) })}
        {/if}
      </button>
    {:else}
      <p class="muted small note">{t("apps.detail.emptyCache")}</p>
    {/if}
  {:else if !known}
    <p class="muted small note icon-line"><Icon name="info" />{t("apps.detail.noCache")}</p>
  {/if}

  <div class="sec">{t("apps.detail.info")}</div>
  <div class="info"><span class="muted">{t("apps.detail.type")}</span><span>{t(app.source === "store" ? "apps.detail.typeStore" : "apps.detail.typeWin32")}</span></div>
  {#if app.installedOn}
    <div class="info"><span class="muted">{t("apps.detail.installed")}</span><span>{formatDate(app.installedOn)}</span></div>
  {/if}

  <button class="btn btn-ghost wide" onclick={onUninstall}>{t("apps.detail.uninstall")}</button>
</aside>

<style>
  .detail { position: sticky; top: 0; width: 320px; flex: none; padding: 20px; align-self: flex-start; max-height: calc(100vh - 48px); overflow-y: auto; }
  .close { position: absolute; top: 10px; right: 10px; background: none; border: none; padding: 6px; border-radius: var(--radius-sm); color: var(--text-muted); display: flex; }
  .close:hover { background: var(--hover); }
  .head { display: flex; gap: 12px; align-items: center; padding-right: 24px; }
  .ico { width: 44px; height: 44px; border-radius: 10px; display: flex; align-items: center; justify-content: center; color: #fff; font-weight: 700; font-size: 19px; flex: none; }
  img.ico { object-fit: contain; padding: 3px; }
  .title { min-width: 0; }
  h2 { font-size: 17px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .small { font-size: 12.5px; }
  .sec { font-size: 11.5px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-muted); margin: 20px 0 6px; }
  .big { font-size: 26px; font-weight: 700; }
  .bar { display: flex; gap: 2px; height: 12px; border-radius: 6px; overflow: hidden; margin: 8px 0 10px; background: var(--track); }
  .bar i { display: block; min-width: 3px; }
  .legend { display: flex; align-items: center; gap: 9px; font-size: 13px; padding: 3px 0; }
  .dot { width: 10px; height: 10px; border-radius: 3px; flex: none; }
  .grow { flex: 1; }
  .wide { width: 100%; margin-top: 14px; padding: 10px 16px; font-size: 13.5px; }
  .wide:disabled { opacity: 0.75; cursor: default; }
  .note { margin-top: 12px; }
  .box { margin-top: 14px; padding: 10px 12px; border-radius: var(--radius-sm); font-size: 13px; }
  .box p { margin: 4px 0 0 25px; }
  .box.ok { background: var(--safe-bg); color: var(--safe-fg); }
  .box.warn { background: var(--check-bg); color: var(--check-fg); border: 1px solid var(--check-border); }
  .info { display: flex; justify-content: space-between; gap: 10px; font-size: 13px; padding: 3px 0; }
  .spinner { display: inline-block; width: 12px; height: 12px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; flex: none; }
  .spinner.light { border-color: color-mix(in srgb, var(--on-accent) 35%, transparent); border-top-color: var(--on-accent); }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
