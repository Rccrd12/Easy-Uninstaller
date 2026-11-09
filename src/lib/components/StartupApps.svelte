<script lang="ts">
  import Icon from "./Icon.svelte";
  import { listStartupApps, setStartupApps } from "../api";
  import { t } from "../i18n/index.svelte";
  import type { StartupApp, StartupChange, StartupReport } from "../types";

  /** How many programs start with Windows now, for the tab. */
  let { count = $bindable(null) }: { count?: number | null } = $props();

  type Notice = { kind: "done" | "error" | "info"; lines: string[]; undo?: StartupChange[] };

  let list = $state<StartupApp[]>([]);
  let status = $state<"loading" | "ready" | "error">("loading");
  /** Ids being changed right now. */
  let busy = $state<Set<string>>(new Set());
  let notice = $state<Notice | null>(null);

  function load() {
    status = "loading";
    listStartupApps().then(
      (l) => {
        list = l;
        status = "ready";
      },
      () => (status = "error"),
    );
  }

  $effect(() => load());

  const on = $derived(list.filter((a) => a.enabled));
  const off = $derived(list.filter((a) => !a.enabled));
  const suggested = $derived(on.filter((a) => a.advice === "turnOff"));
  $effect(() => {
    count = status === "ready" ? on.length : null;
  });

  const nameOf = (id: string) => list.find((a) => a.id === id)?.name ?? id;

  /** Applies the changes, updates the rows, and says what happened. */
  async function change(changes: StartupChange[], undoing = false) {
    if (!changes.length) return;
    busy = new Set([...busy, ...changes.map((c) => c.id)]);
    notice = null;
    let report: StartupReport;
    try {
      report = await setStartupApps(changes);
    } catch {
      report = { changed: [], failed: changes.map((c) => c.id), declined: [] };
    }
    const wanted = new Map(changes.map((c) => [c.id, c.enabled]));
    list = list.map((a) => (report.changed.includes(a.id) ? { ...a, enabled: wanted.get(a.id)! } : a));
    busy = new Set([...busy].filter((id) => !wanted.has(id)));
    notice = describe(report, changes, undoing);
  }

  function describe(report: StartupReport, changes: StartupChange[], undoing: boolean): Notice | null {
    const lines: string[] = [];
    const done = changes.filter((c) => report.changed.includes(c.id));
    if (undoing) {
      if (done.length) lines.push(t("startup.undone"));
    } else if (done.length) {
      const turnedOff = done.filter((c) => !c.enabled);
      if (turnedOff.length > 1) lines.push(t("startup.turnedOff", { count: turnedOff.length }));
      else if (turnedOff.length === 1) lines.push(t("startup.turnedOffOne", { name: nameOf(turnedOff[0].id) }));
      for (const c of done.filter((c) => c.enabled)) lines.push(t("startup.turnedOn", { name: nameOf(c.id) }));
    }
    if (report.failed.length === 1) lines.push(t("startup.failed", { name: nameOf(report.failed[0]) }));
    else if (report.failed.length > 1) lines.push(t("startup.failedMany", { count: report.failed.length }));
    if (report.declined.length) lines.push(t("startup.declined"));
    if (!lines.length) return null;
    const problem = report.failed.length > 0 || report.declined.length > 0;
    return {
      kind: problem ? (done.length ? "info" : "error") : "done",
      lines,
      undo: undoing || !done.length ? undefined : done.map((c) => ({ id: c.id, enabled: !c.enabled })),
    };
  }

  function toggle(app: StartupApp) {
    change([{ id: app.id, enabled: !app.enabled }]);
  }

  function adviceTag(app: StartupApp): { cls: string; key: string } | null {
    if (app.advice === "system") return { cls: "tag-gray", key: "startup.advice.system" };
    if (!app.enabled) return { cls: "tag-gray", key: "startup.advice.off" };
    if (app.advice === "turnOff") return { cls: "tag-check", key: "startup.advice.turnOff" };
    if (app.advice === "keep") return { cls: "tag-safe", key: "startup.advice.keep" };
    return null;
  }

  function detail(app: StartupApp): string {
    if (app.note) return t(`startup.notes.${app.note}`);
    return [app.publisher, app.fileName].filter(Boolean).join(" · ");
  }
</script>

{#if status === "loading"}
  <p class="muted empty" role="status"><span class="spinner"></span>{t("startup.loading")}</p>
{:else if status === "error"}
  <p class="muted empty" role="alert">{t("startup.error")} <button class="btn btn-ghost" onclick={load}>{t("startup.retry")}</button></p>
{:else if !list.length}
  <div class="card blank">
    <span class="hero-icon"><Icon name="rocket" size={22} /></span>
    <p class="muted">{t("startup.empty")}</p>
  </div>
{:else}
  <div class="card hero">
    <span class="hero-icon"><Icon name="rocket" size={22} /></span>
    <div class="grow">
      <h2>{on.length === 1 ? t("startup.heroTitleOne") : t("startup.heroTitle", { count: on.length })}</h2>
      <p class="muted">
        {#if suggested.length > 1}{t("startup.heroText", { count: suggested.length })}
        {:else if suggested.length === 1}{t("startup.heroTextOne")}
        {:else}{t("startup.heroTextNone")}{/if}
      </p>
    </div>
    {#if suggested.length}
      <button class="btn btn-primary" disabled={busy.size > 0}
              onclick={() => change(suggested.map((a) => ({ id: a.id, enabled: false })))}>
        {busy.size > 0 ? t("startup.working") : suggested.length === 1 ? t("startup.turnOffSuggestedOne") : t("startup.turnOffSuggested", { count: suggested.length })}
      </button>
    {/if}
  </div>

  {#if notice}
    <div class="card notice {notice.kind}" role="status">
      <Icon name={notice.kind === "done" ? "checkCircle" : notice.kind === "error" ? "warning" : "info"} size={18} />
      <div class="grow">{#each notice.lines as line}<div>{line}</div>{/each}</div>
      {#if notice.undo}
        <button class="btn btn-ghost" disabled={busy.size > 0} onclick={() => change(notice!.undo!, true)}>
          <Icon name="undo" /> {t("startup.undo")}
        </button>
      {/if}
      <button class="icon-btn" aria-label={t("common.close")} onclick={() => (notice = null)}><Icon name="dismiss" /></button>
    </div>
  {/if}

  {#snippet row(app: StartupApp)}
    {@const tag = adviceTag(app)}
    <div class="item" class:off={!app.enabled}>
      {#if app.icon}
        <img class="ico" src={app.icon} alt="" width="32" height="32" />
      {:else}
        <div class="ico" style="background:{app.iconColor}">{app.name[0]}</div>
      {/if}
      <div class="info">
        <div class="name">
          {app.name}
          {#if app.allUsers}<span class="tag tag-info" title={t("startup.allUsersHint")}>{t("startup.allUsers")}</span>{/if}
        </div>
        <div class="muted meta" title={app.command}>{detail(app) || " "}</div>
      </div>
      <span class="impact impact-{app.impact}" title={t("startup.estimate")}>
        <span class="bars" aria-hidden="true"><i></i><i></i><i></i></span>
        {t(`startup.impact.${app.impact}`)}
      </span>
      <span class="advice">{#if tag}<span class="tag {tag.cls}">{t(tag.key)}</span>{/if}</span>
      <label class="switch" title={app.advice === "system" ? t("startup.systemHint") : app.allUsers ? t("startup.allUsersHint") : undefined}>
        <input type="checkbox" role="switch" checked={app.enabled}
               disabled={app.advice === "system" || busy.has(app.id)}
               aria-label={t("startup.switchLabel", { name: app.name })}
               onchange={(e) => { (e.currentTarget as HTMLInputElement).checked = app.enabled; toggle(app); }} />
        <span class="track" class:busy={busy.has(app.id)}></span>
      </label>
    </div>
  {/snippet}

  <div class="card list">
    {#each on as app (app.id)}{@render row(app)}{/each}
  </div>

  {#if off.length}
    <h3 class="section">{t("startup.offTitle", { count: off.length })}</h3>
    <div class="card list">
      {#each off as app (app.id)}{@render row(app)}{/each}
    </div>
  {/if}

  <p class="muted foot"><Icon name="info" /> {t("startup.applyNote")} {t("startup.estimate")}</p>
{/if}

<style>
  .empty { padding: 24px 0; text-align: center; }
  .spinner { display: inline-block; width: 14px; height: 14px; margin-right: 8px; vertical-align: -2px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .grow { flex: 1; min-width: 0; }

  .hero, .blank { display: flex; align-items: center; gap: 16px; padding: 16px 20px; margin-bottom: 12px; }
  .hero h2 { font-size: 17px; margin: 0 0 3px; }
  .hero p { margin: 0; font-size: 13px; line-height: 1.45; }
  .hero .btn { flex: none; }
  .hero-icon { width: 44px; height: 44px; border-radius: 12px; flex: none; display: flex; align-items: center; justify-content: center; background: var(--icon-tile-bg); color: var(--icon-tile-fg); }
  .blank p { margin: 0; }

  .notice { display: flex; align-items: center; gap: 12px; padding: 10px 14px; margin-bottom: 12px; font-size: 13px; }
  .notice.done { color: var(--safe-fg); background: var(--safe-bg); border-color: transparent; }
  .notice.done .grow { color: var(--text); }
  .notice.info { border-color: var(--check-border); }
  .notice.error { color: var(--keep-fg); background: var(--keep-bg); border-color: transparent; }
  .notice.error .grow { color: var(--text); }
  .icon-btn { background: none; border: none; color: var(--text-muted); padding: 4px; cursor: pointer; border-radius: var(--radius-sm); }
  .icon-btn:hover { background: var(--hover); }

  .list { padding: 2px 16px; }
  .item { display: flex; align-items: center; gap: 13px; padding: 11px 0; border-bottom: 1px solid var(--border); }
  .item:last-child { border-bottom: none; }
  .item.off .ico, .item.off .info, .item.off .impact { opacity: 0.6; }
  img.ico { object-fit: contain; padding: 3px; background: none; }
  .ico { width: 36px; height: 36px; border-radius: 9px; display: flex; align-items: center; justify-content: center; color: #fff; font-weight: 700; flex: none; }
  .info { flex: 1; min-width: 0; }
  .name { font-size: 14.5px; font-weight: 600; display: flex; align-items: center; gap: 8px; min-width: 0; }
  .name, .meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { font-size: 12px; margin-top: 1px; }

  .impact { width: 128px; flex: none; font-size: 12px; display: flex; align-items: center; gap: 7px; color: var(--text-muted); }
  .bars { display: inline-flex; align-items: flex-end; gap: 2px; height: 12px; }
  .bars i { width: 4px; border-radius: 1px; background: var(--track); }
  .bars i:nth-child(1) { height: 5px; } .bars i:nth-child(2) { height: 8px; } .bars i:nth-child(3) { height: 12px; }
  .impact-low .bars i:nth-child(1) { background: var(--text-muted); }
  .impact-medium .bars i:nth-child(-n+2) { background: var(--check-fg); }
  .impact-medium { color: var(--check-fg); }
  .impact-high .bars i { background: var(--danger); }
  .impact-high { color: var(--danger); font-weight: 600; }
  .advice { width: 170px; flex: none; text-align: center; }

  .switch { position: relative; flex: none; width: 40px; height: 22px; cursor: pointer; }
  .switch input { position: absolute; inset: 0; width: 100%; height: 100%; margin: 0; opacity: 0; cursor: inherit; z-index: 1; }
  .track { position: absolute; inset: 0; border-radius: 11px; background: var(--surface); border: 1px solid var(--border-strong); transition: background 0.15s; }
  .track::after { content: ""; position: absolute; top: 4px; left: 4px; width: 12px; height: 12px; border-radius: 50%; background: var(--text-muted); transition: transform 0.15s, background 0.15s; }
  .switch input:checked + .track { background: var(--accent); border-color: var(--accent); }
  .switch input:checked + .track::after { transform: translateX(18px); background: var(--on-accent); }
  .switch input:focus-visible + .track { outline: 2px solid var(--accent); outline-offset: 2px; }
  .switch input:disabled { cursor: not-allowed; }
  .switch input:disabled + .track { opacity: 0.45; }
  .track.busy::after { animation: pulse 0.9s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.35; } }

  .section { font-size: 13px; font-weight: 600; color: var(--text-muted); margin: 18px 0 8px; }
  .foot { font-size: 12px; margin: 14px 2px 4px; line-height: 1.5; }
</style>
