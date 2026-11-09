<script lang="ts">
  import FileThumb from "./FileThumb.svelte";
  import Icon from "./Icon.svelte";
  import StatusIcon from "./StatusIcon.svelte";
  import { tidyApply, tidyAsk, tidyByType, tidyOverview, tidyPreview, tidyUndo } from "../api";
  import { aiConfig, aiErrorText, aiReady, openSetup, serviceInfo, serviceLabel } from "../ai.svelte";
  import { fileCount, formatBytes } from "../format";
  import { getLanguage, locale, t } from "../i18n/index.svelte";
  import type { IconName } from "../icons";
  import { displayPath, nameOf } from "../paths";
  import { saveSettings, settings } from "../settings.svelte";
  import type { TidyDest, TidyOverview, TidyPlan, TidyPreview, TidyReport, TidyUndoReport } from "../types";

  /**
   * Tidying the Downloads folder (AI help, phase 3). The AI proposes folders from the
   * file names; without it, files are grouped by type. Every file of the proposal can
   * be unticked, nothing moves until "Tidy up", and a tidy can be undone.
   */
  let { onBack }: { onBack: () => void } = $props();

  type Phase = "loading" | "intro" | "confirm" | "asking" | "plan" | "applying" | "done";
  let phase = $state<Phase>("loading");
  let overview = $state<TidyOverview | null>(null);
  let preview = $state<TidyPreview | null>(null);
  let plan = $state<TidyPlan | null>(null);
  /** Ticked files, by folder id. */
  let picked = $state<Record<string, Set<string>>>({});
  let pickedInstallers = $state<Set<string>>(new Set());
  let open = $state<Set<string>>(new Set());
  let report = $state<TidyReport | null>(null);
  let undone = $state<TidyUndoReport | null>(null);
  let undoing = $state(false);
  let error = $state<string | null>(null);
  let dontAsk = $state(false);
  /** The exact list sent to the AI, shown on request. */
  let showSent = $state(false);
  /** Waiting for the AI (not for the quick grouping by type). */
  let askingAi = $state(false);

  const service = $derived(t(`ai.services.${settings.ai.service}.name`));
  const kind = $derived(serviceInfo(settings.ai.service)?.kind);

  $effect(() => {
    load();
  });

  function errorText(e: unknown): string {
    const key = String(e);
    if (key.startsWith("tidy.error.")) return t(key);
    if (key.startsWith("ai.error.")) return aiErrorText(e);
    return t("tidy.failed", { error: key });
  }

  async function load() {
    phase = "loading";
    error = null;
    try {
      overview = await tidyOverview();
    } catch (e) {
      error = errorText(e);
    }
    phase = "intro";
  }

  function startAi() {
    error = null;
    if (!aiReady()) {
      openSetup(startAi);
      return;
    }
    if (settings.ai.confirm) {
      tidyPreview(getLanguage())
        .then((p) => {
          preview = p;
          phase = "confirm";
        })
        .catch((e) => (error = errorText(e)));
    } else {
      ask();
    }
  }

  async function ask() {
    if (dontAsk) {
      settings.ai.confirm = false;
      saveSettings();
    }
    phase = "asking";
    askingAi = true;
    error = null;
    try {
      show(await tidyAsk(aiConfig(), getLanguage()));
    } catch (e) {
      error = errorText(e);
      phase = "intro";
    }
  }

  async function byType() {
    error = null;
    phase = "asking";
    askingAi = false;
    try {
      show(await tidyByType(getLanguage()));
    } catch (e) {
      error = errorText(e);
      phase = "intro";
    }
  }

  /** Every file of every folder ticked, the installers not: they would leave the PC's files. */
  function show(p: TidyPlan) {
    plan = p;
    picked = Object.fromEntries(p.folders.map((f) => [f.id, new Set(f.files.map((x) => x.path))]));
    pickedInstallers = new Set();
    open = new Set(p.folders.slice(0, 1).map((f) => f.id));
    phase = "plan";
  }

  function toggleFolder(id: string) {
    const folder = plan?.folders.find((f) => f.id === id);
    if (!folder) return;
    const all = picked[id]?.size === folder.files.length;
    picked = { ...picked, [id]: all ? new Set() : new Set(folder.files.map((f) => f.path)) };
  }
  function toggleFile(id: string, path: string) {
    const next = new Set(picked[id]);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    picked = { ...picked, [id]: next };
  }
  function toggleInstaller(path: string) {
    const next = new Set(pickedInstallers);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    pickedInstallers = next;
  }
  function toggleOpen(id: string) {
    const next = new Set(open);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    open = next;
  }
  const allInstallers = $derived(!!plan && plan.installers.length > 0 && plan.installers.every((f) => pickedInstallers.has(f.path)));

  const moveCount = $derived(Object.values(picked).reduce((s, set) => s + set.size, 0));
  const folderTotal = $derived(Object.values(picked).filter((set) => set.size > 0).length);
  const folders = (n: number) => (n === 1 ? t("tidy.folderOne") : t("tidy.folders", { count: n.toLocaleString(locale()) }));

  async function apply() {
    if (!plan || (!moveCount && !pickedInstallers.size)) return;
    phase = "applying";
    error = null;
    try {
      const choices = plan.folders.map((f) => ({ folderId: f.id, paths: [...(picked[f.id] ?? [])] })).filter((c) => c.paths.length);
      report = await tidyApply(choices, [...pickedInstallers], plan);
      undone = null;
      phase = "done";
    } catch (e) {
      error = errorText(e);
      phase = "plan";
    }
  }

  async function undo(runId: string) {
    undoing = true;
    error = null;
    try {
      undone = await tidyUndo(runId);
      if (phase !== "done") await load();
    } catch (e) {
      error = errorText(e);
    } finally {
      undoing = false;
    }
  }

  const destIcon: Record<TidyDest, IconName> = { documents: "document", pictures: "image", videos: "video", music: "music", downloads: "download" };
  const placeName = (d: TidyDest) => t(`files.review.folders.${d}`);
  const dateOf = (secs: number) => new Date(secs * 1000).toLocaleDateString(locale(), { day: "numeric", month: "long" });
</script>

<div class="tidy">
  <button class="back" onclick={onBack}><Icon name="chevronLeft" size={14} />{t("files.title")}</button>
  <h1>{t("tidy.title")}</h1>
  <p class="muted sub">{t("tidy.subtitle")}</p>

  {#if error}
    <div class="card status error-card" role="alert"><StatusIcon kind="warn" /><span>{error}</span></div>
  {/if}

  {#if phase === "loading"}
    <div class="card status" role="status"><span class="spinner" aria-hidden="true"></span>{t("files.review.loading")}</div>
  {:else if phase === "intro" && overview}
    {#if !overview.downloads}
      <div class="card status"><Icon name="info" />{t("tidy.noDownloads")}</div>
    {:else}
      <div class="card intro">
        <span class="tile" aria-hidden="true"><Icon name="download" size={24} /></span>
        <div class="intro-text">
          {#if overview.fileCount}
            <b>{t("tidy.loose", { files: fileCount(overview.fileCount), size: formatBytes(overview.sizeBytes) })}</b>
          {:else}
            <b>{t("tidy.none")}</b>
          {/if}
          <span class="muted small">{overview.downloads}</span>
          {#if overview.fileCount}
            <div class="buttons">
              <button class="btn btn-ai" onclick={startAi}><Icon name="sparkle" />{t("tidy.askAi")}</button>
              <button class="btn btn-ghost" onclick={byType}><Icon name="list" />{t("tidy.byType")}</button>
            </div>
            {#if !aiReady()}<p class="muted small ai-off">{t("tidy.aiOff")}</p>{/if}
          {/if}
        </div>
      </div>
    {/if}
    {#if overview.lastRun}
      {@const run = overview.lastRun}
      <div class="card last">
        <Icon name="undo" />
        <span class="grow">{t("tidy.lastRun", { date: dateOf(run.at), files: fileCount(run.movedCount), folders: folders(run.folderCount) })}</span>
        <button class="btn btn-ghost" disabled={undoing} onclick={() => undo(run.id)}>{undoing ? t("tidy.done.undoing") : t("tidy.undoLast")}</button>
      </div>
    {/if}
    {#if undone}
      <p class="muted small undone">{t("tidy.done.undone", { files: fileCount(undone.restoredCount) })}{undone.failedCount ? ` ${t("tidy.done.undoFailed", { files: fileCount(undone.failedCount) })}` : ""}</p>
    {/if}
  {:else if phase === "confirm" && preview}
    <div class="card box">
      <span class="label"><Icon name="sparkle" size={14} />{t("tidy.confirm.title", { service: serviceLabel(settings.ai.service, settings.ai.model) })}</span>
      <p class="intro-line">{t("tidy.confirm.intro", { files: fileCount(preview.lines.length) })}</p>
      <button class="link small" onclick={() => (showSent = !showSent)}>{showSent ? t("tidy.confirm.hideList") : t("tidy.confirm.showList")}</button>
      {#if showSent}<pre class="sent">{preview.lines.join("\n")}</pre>{/if}
      {#if preview.notSentCount}<p class="muted small">{t("tidy.confirm.notSent", { files: fileCount(preview.notSentCount) })}</p>{/if}
      <ul class="promises">
        <li><Icon name="money" />{t(kind === "cli" ? "ai.confirm.costCli" : kind === "local" ? "ai.confirm.costLocal" : "ai.confirm.costApi")}</li>
      </ul>
      <div class="buttons">
        <button class="btn btn-ai" onclick={ask}><Icon name="sparkle" />{t("tidy.confirm.send")}</button>
        <button class="btn btn-ghost" onclick={() => (phase = "intro")}>{t("common.cancel")}</button>
      </div>
      <label class="dont-ask"><input type="checkbox" bind:checked={dontAsk} /> {t("ai.confirm.dontAsk")}</label>
    </div>
  {:else if phase === "asking"}
    <div class="card status" role="status"><span class="spinner" aria-hidden="true"></span>{askingAi ? t("tidy.asking", { service }) : t("files.review.loading")}</div>
  {:else if (phase === "plan" || phase === "applying") && plan}
    {@const p = plan}
    <div class="plan-head">
      <h2>{t("tidy.plan.title", { files: fileCount(p.folders.reduce((s, f) => s + f.files.length, 0)), folders: folders(p.folders.length) })}</h2>
      <span class="who">{#if p.byAi}<Icon name="sparkle" size={14} />{t("tidy.plan.byAi", { service: serviceLabel(settings.ai.service, settings.ai.model) })}{:else}{t("tidy.plan.byType")}{/if}</span>
    </div>
    {#if p.folders.length === 0 && p.installers.length === 0}
      <div class="card status"><Icon name="info" />{t("tidy.plan.empty")}</div>
    {/if}
    {#each p.folders as folder (folder.id)}
      {@const set = picked[folder.id] ?? new Set()}
      {@const isOpen = open.has(folder.id)}
      <div class="card folder" class:off={set.size === 0}>
        <div class="folder-head">
          <input type="checkbox" checked={set.size === folder.files.length} indeterminate={set.size > 0 && set.size < folder.files.length}
                 onchange={() => toggleFolder(folder.id)} aria-label={folder.name} />
          <span class="tile small-tile" aria-hidden="true"><Icon name={destIcon[folder.dest]} size={18} /></span>
          <div class="folder-text">
            <div class="folder-name"><b>{folder.name}</b><span class="tag" class:tag-new={!folder.exists}>{folder.exists ? t("tidy.plan.exists") : t("tidy.plan.new")}</span></div>
            <div class="muted small">{folder.why ? `${folder.why} · ` : ""}{displayPath(folder.target)}</div>
          </div>
          <span class="muted count">{fileCount(set.size)}</span>
          <span class="size">{formatBytes(folder.files.filter((f) => set.has(f.path)).reduce((s, f) => s + f.sizeBytes, 0))}</span>
          <button class="expand" aria-expanded={isOpen} onclick={() => toggleOpen(folder.id)}>
            {isOpen ? t("tidy.plan.hideFiles") : t("tidy.plan.showFiles")}<span class="chev" class:up={isOpen}><Icon name="chevronDown" size={12} /></span>
          </button>
        </div>
        {#if isOpen}
          <ul class="files">
            {#each folder.files as file (file.path)}
              <li class:picked={set.has(file.path)}>
                <input type="checkbox" checked={set.has(file.path)} onchange={() => toggleFile(folder.id, file.path)} aria-label={t("tidy.plan.pick", { name: nameOf(file.path) })} />
                <FileThumb path={file.path} size={28} />
                <span class="file-name" title={file.path}>{nameOf(file.path)}</span>
                <span class="muted small">{formatBytes(file.sizeBytes)}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/each}

    {#if p.installers.length}
      <div class="card folder installers" class:off={pickedInstallers.size === 0}>
        <div class="folder-head">
          <input type="checkbox" checked={allInstallers} indeterminate={pickedInstallers.size > 0 && !allInstallers}
                 onchange={() => (pickedInstallers = allInstallers ? new Set() : new Set(p.installers.map((f) => f.path)))} aria-label={t("tidy.installers.title")} />
          <span class="tile small-tile quarantine-tile" aria-hidden="true"><Icon name="quarantine" size={18} /></span>
          <div class="folder-text">
            <div class="folder-name"><b>{t("tidy.installers.title")}</b></div>
            <div class="muted small">{t("tidy.installers.desc")}</div>
          </div>
          <span class="size">{formatBytes(p.installers.reduce((s, f) => s + f.sizeBytes, 0))}</span>
        </div>
        <ul class="files">
          {#each p.installers as file (file.path)}
            <li class:picked={pickedInstallers.has(file.path)}>
              <input type="checkbox" checked={pickedInstallers.has(file.path)} onchange={() => toggleInstaller(file.path)} aria-label={t("tidy.plan.pick", { name: nameOf(file.path) })} />
              <FileThumb path={file.path} size={28} />
              <span class="file-name" title={file.path}>{nameOf(file.path)}</span>
              <span class="muted small">{formatBytes(file.sizeBytes)}</span>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    {#if p.leftCount}<p class="muted small left">{t(p.byAi ? "tidy.plan.left" : "tidy.plan.leftByType", { files: fileCount(p.leftCount) })}</p>{/if}
    {#if p.notSentCount}<p class="muted small left">{t("tidy.plan.notSent", { files: fileCount(p.notSentCount) })}</p>{/if}

    <div class="card footer">
      <div class="footer-text">
        <b>{t("tidy.apply.summary", { files: fileCount(moveCount), folders: folders(folderTotal) })}{#if pickedInstallers.size} {t("tidy.apply.plusInstallers", { files: fileCount(pickedInstallers.size) })}{/if}</b>
        <span class="muted small">{moveCount || pickedInstallers.size ? t("tidy.apply.note") : t("tidy.apply.nothing")}</span>
      </div>
      <span class="grow"></span>
      <button class="btn btn-ghost" disabled={phase === "applying"} onclick={() => { plan = null; load(); }}>{t("common.cancel")}</button>
      <button class="btn btn-primary" disabled={phase === "applying" || (!moveCount && !pickedInstallers.size)} onclick={apply}>
        {#if phase !== "applying"}<Icon name="folderOpen" />{/if}{phase === "applying" ? t("tidy.apply.moving") : t("tidy.apply.button")}
      </button>
    </div>
  {:else if phase === "done" && report}
    {@const r = report}
    <div class="card done">
      {#if undone}
        <StatusIcon kind={undone.failedCount ? "warn" : "ok"} />
        <div class="done-text">
          <b>{t("tidy.done.undone", { files: fileCount(undone.restoredCount) })}</b>
          {#if undone.failedCount}<p class="muted small">{t("tidy.done.undoFailed", { files: fileCount(undone.failedCount) })}</p>{/if}
        </div>
      {:else}
        <StatusIcon kind="ok" />
        <div class="done-text">
          {#if r.movedCount}<b>{t("tidy.done.title", { files: fileCount(r.movedCount), folders: folders(r.folderCount) })}</b>{/if}
          {#if r.quarantine.movedCount}<p>{t("tidy.done.quarantine", { files: fileCount(r.quarantine.movedCount) })}</p>{/if}
          {#if r.skippedCount}<p class="muted small">{t("tidy.done.skipped", { files: fileCount(r.skippedCount) })}</p>{/if}
          {#if !r.movedCount && !r.quarantine.movedCount}<b>{t("files.nothingMoved")}</b>{/if}
          {#if r.movedCount && plan}
            <ul class="where">
              {#each plan.folders.filter((f) => picked[f.id]?.size) as f (f.id)}
                <li><Icon name={destIcon[f.dest]} size={16} /><span>{displayPath(f.target)}</span><span class="muted">{fileCount(picked[f.id].size)}</span></li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </div>
    <div class="buttons">
      {#if r.runId && !undone}
        {@const id = r.runId}
        <button class="btn btn-ghost" disabled={undoing} onclick={() => undo(id)}>{#if !undoing}<Icon name="undo" />{/if}{undoing ? t("tidy.done.undoing") : t("tidy.done.undo")}</button>
      {/if}
      <button class="btn btn-primary" onclick={onBack}>{t("tidy.done.back")}</button>
    </div>
  {/if}
</div>

<style>
  .tidy { padding-bottom: 8px; }
  .back { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 2px 0; margin-bottom: 4px; display: inline-flex; align-items: center; gap: 4px; }
  .back:hover { color: var(--accent); }
  .sub { margin: 3px 0 14px; max-width: 760px; line-height: 1.45; }
  h2 { font-size: 16px; font-weight: 600; }
  .small { font-size: 12px; }
  .grow { flex: 1; }
  .status { padding: 16px 18px; display: flex; align-items: center; gap: 10px; font-size: 13.5px; margin-bottom: 12px; }
  .error-card { border-left: 3px solid var(--danger); }
  .spinner { width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--tip-border); border-top-color: var(--ai); animation: spin 0.9s linear infinite; flex: none; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .btn:disabled { opacity: 0.5; cursor: default; }

  .intro { display: flex; gap: 16px; padding: 18px 20px; margin-bottom: 12px; }
  .tile { width: 48px; height: 48px; border-radius: var(--radius); background: var(--icon-tile-bg); color: var(--icon-tile-fg); display: flex; align-items: center; justify-content: center; flex: none; }
  .small-tile { width: 36px; height: 36px; border-radius: 8px; }
  .quarantine-tile { background: var(--check-bg); color: var(--check-fg); }
  .intro-text { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .intro-text b { font-size: 15px; }
  .buttons { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .ai-off { margin-top: 8px; }
  .last { display: flex; align-items: center; gap: 12px; padding: 10px 14px; margin-bottom: 12px; font-size: 13px; }
  .undone { margin: -4px 0 12px; }

  .box { border-left: 3px solid var(--ai); padding: 14px 16px; }
  .label { display: inline-flex; align-items: center; gap: 6px; font-size: 11px; font-weight: 700; letter-spacing: 0.4px; text-transform: uppercase; color: var(--ai-fg); }
  .intro-line { font-size: 13px; margin: 8px 0; }
  .sent { font-family: Consolas, "Cascadia Mono", monospace; font-size: 11.5px; line-height: 1.55; background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 8px 10px; white-space: pre-wrap; word-break: break-word; max-height: 220px; overflow-y: auto; user-select: text; }
  .promises { list-style: none; font-size: 12px; margin-top: 10px; display: flex; flex-direction: column; gap: 4px; line-height: 1.4; }
  .promises li { display: flex; gap: 7px; align-items: flex-start; }
  .promises li :global(.icon) { color: var(--text-muted); margin-top: 0.15em; }
  .dont-ask { display: flex; gap: 6px; align-items: center; font-size: 12px; margin-top: 10px; }
  .dont-ask input { accent-color: var(--ai); }

  .plan-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; margin-bottom: 10px; }
  .who { display: inline-flex; align-items: center; gap: 5px; font-size: 12.5px; color: var(--ai-fg); flex: none; }
  .folder { margin-bottom: 8px; overflow: hidden; }
  .folder.off .folder-head { opacity: 0.6; }
  .folder-head { display: flex; align-items: center; gap: 12px; padding: 10px 14px; }
  input[type="checkbox"] { width: 18px; height: 18px; accent-color: var(--accent); flex: none; }
  .folder-text { flex: 1; min-width: 0; }
  .folder-name { display: flex; align-items: center; gap: 8px; font-size: 14px; }
  .folder-text .small { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 2px; }
  .tag { font-size: 11px; padding: 1px 8px; border-radius: 10px; background: var(--track); color: var(--text-muted); font-weight: 600; white-space: nowrap; }
  .tag-new { background: var(--safe-bg); color: var(--safe-fg); }
  .count { font-size: 12.5px; flex: none; }
  .size { font-size: 13px; font-weight: 600; width: 72px; text-align: right; flex: none; }
  .expand { border: none; background: none; color: var(--accent); font-size: 12.5px; display: inline-flex; align-items: center; gap: 4px; flex: none; padding: 4px; }
  .chev { display: inline-flex; transition: transform 0.12s; }
  .chev.up { transform: rotate(180deg); }
  .files { list-style: none; border-top: 1px solid var(--border); padding: 4px 14px 6px 44px; display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); column-gap: 18px; }
  .files li { display: flex; align-items: center; gap: 10px; padding: 5px 0; min-width: 0; }
  .file-name { flex: 1; min-width: 0; font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .files li:not(.picked) .file-name { color: var(--text-muted); }
  .left { margin: 6px 2px; }

  .footer { position: sticky; bottom: 0; display: flex; align-items: center; gap: 10px; padding: 12px 16px; margin-top: 12px; box-shadow: var(--shadow-up); z-index: 2; }
  .footer-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }

  .done { display: flex; gap: 12px; align-items: flex-start; padding: 16px 18px; border-left: 4px solid var(--safe-fg); }
  .done-text { display: flex; flex-direction: column; gap: 4px; font-size: 13.5px; min-width: 0; }
  .where { list-style: none; margin-top: 8px; display: flex; flex-direction: column; gap: 5px; font-size: 13px; }
  .where li { display: flex; align-items: center; gap: 8px; }
  .where li :global(.icon) { color: var(--text-muted); }
</style>
