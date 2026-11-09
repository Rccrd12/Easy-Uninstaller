<script lang="ts">
  import AppIcon from "./AppIcon.svelte";
  import Icon from "./Icon.svelte";
  import { reinstallApp } from "../api";
  import { formatBytes, formatDate } from "../format";
  import { t } from "../i18n/index.svelte";
  import type { IconName } from "../icons";
  import type { ReinstallWay, UninstalledApp } from "../types";

  /**
   * The apps uninstalled with Easy Uninstaller, each with the way to get it back.
   * `installedNames` (lower case) marks the ones that are installed again.
   */
  let {
    list,
    installedNames,
    onForget,
  }: { list: UninstalledApp[]; installedNames: Set<string>; onForget: (id: string) => void } = $props();

  let failed = $state<string | null>(null);

  const ICON: Record<ReinstallWay, IconName> = { store: "bag", steam: "games", website: "globe", search: "search" };

  const isoDate = (secs: number) => new Date(secs * 1000).toISOString().slice(0, 10);

  async function open(app: UninstalledApp) {
    failed = null;
    await reinstallApp(app.id).catch(() => (failed = app.id));
  }
</script>

{#if list.length}
  <div class="card list">
    {#each list as app (app.id)}
      {@const again = installedNames.has(app.name.toLowerCase())}
      <div class="row">
        <AppIcon {app} size={34} />
        <div class="info">
          <div class="name">{app.name}</div>
          <div class="muted meta">
            {[app.publisher, t("uninstalled.when", { date: formatDate(isoDate(app.uninstalledAt)) }), app.sizeBytes ? formatBytes(app.sizeBytes) : ""].filter(Boolean).join(" · ")}
          </div>
          {#if failed === app.id}<div class="err" role="alert">{t("uninstalled.openError")}</div>{/if}
        </div>
        {#if again}
          <span class="tag tag-safe"><Icon name="check" size={13} />{t("uninstalled.again")}</span>
        {:else}
          <button class="btn btn-ghost" onclick={() => open(app)}>
            <Icon name={ICON[app.way]} />{t(`uninstalled.${app.way}`, { site: app.site })}<Icon name="openExternal" size={13} />
          </button>
        {/if}
        <button class="forget" title={t("uninstalled.forget")} aria-label={t("uninstalled.forget")} onclick={() => onForget(app.id)}><Icon name="dismiss" /></button>
      </div>
    {/each}
  </div>
{:else}
  <div class="empty">
    <span class="tile"><Icon name="undo" size={26} /></span>
    <p class="muted">{t("uninstalled.empty")}</p>
  </div>
{/if}

<style>
  .list { overflow: hidden; }
  .row { display: flex; align-items: center; gap: 12px; padding: 10px 10px 10px 14px; border-top: 1px solid var(--border); }
  .row:first-child { border-top: none; }
  .info { flex: 1; min-width: 0; }
  .name, .meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name { font-size: 14px; font-weight: 600; }
  .meta { font-size: 12px; }
  .err { font-size: 12px; color: var(--keep-fg); }
  .btn { white-space: nowrap; }
  .tag { display: inline-flex; align-items: center; gap: 5px; }
  .forget { background: none; border: none; color: var(--text-muted); padding: 6px; border-radius: var(--radius-sm); display: flex; }
  .forget:hover { background: var(--hover); color: var(--text); }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 48px 24px; text-align: center; }
  .empty p { max-width: 420px; line-height: 1.45; }
  .tile { width: 52px; height: 52px; border-radius: 14px; display: flex; align-items: center; justify-content: center; background: var(--icon-tile-bg); color: var(--icon-tile-fg); }
</style>
