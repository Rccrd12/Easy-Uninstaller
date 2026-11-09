<script lang="ts">
  import EyeOffIcon from "../components/EyeOffIcon.svelte";
  import Icon from "../components/Icon.svelte";
  import AiServicePicker from "../components/AiServicePicker.svelte";
  import { AI_PARALLEL_CHOICES, aiStats, createRestorePoint, KEEP_DAYS_CHOICES, listExclusions } from "../api";
  import { forgetAnswers, serviceInfo } from "../ai.svelte";
  import { formatBytes } from "../format";
  import { getLanguage, languages, setLanguage, t, type Language } from "../i18n/index.svelte";
  import { applyTextSize, quarantine, saveQuarantine, saveSettings, settings } from "../settings.svelte";
  import type { Screen } from "../screens";
  import type { AiMode, AiStats, RestorePointOutcome } from "../types";
  import { getTheme, setTheme, themes } from "../theme.svelte";
  import { check, install, update } from "../updates.svelte";
  // Only this field ends up in the bundle; the release workflow keeps it in sync with the tag.
  import { version } from "../../../package.json";

  let { onNavigate }: { onNavigate: (s: Screen) => void } = $props();

  let hiddenCount = $state<number | null>(null);
  $effect(() => {
    listExclusions()
      .then((l) => (hiddenCount = l.length))
      .catch(() => (hiddenCount = null));
  });

  const phase = $derived(update.phase);

  let restorePoint = $state<RestorePointOutcome | "creating" | null>(null);

  let saving = $state(false);
  let saveError = $state<string | null>(null);

  /** The chosen drive, also when it is not connected right now (it is still the choice). */
  const driveMissing = $derived(quarantine.drive !== null && !quarantine.drives.some((d) => d.mountPoint === quarantine.drive));

  async function changeQuarantine(change: { keepDays?: number; drive?: string | null }) {
    saving = true;
    saveError = null;
    try {
      await saveQuarantine({ keepDays: quarantine.keepDays, drive: quarantine.drive, ...change });
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }

  // ── AI help ──
  const modes: AiMode[] = ["off", "online", "local"];
  let aiTab = $state<"cli" | "api">(serviceInfo(settings.ai.service)?.kind === "api" ? "api" : "cli");
  let stats = $state<AiStats | null>(null);
  const loadStats = () =>
    aiStats()
      .then((s) => (stats = s))
      .catch(() => {});
  $effect(() => {
    loadStats();
  });

  function setMode(mode: AiMode) {
    settings.ai.mode = mode;
    saveSettings();
  }

  async function clearAnswers() {
    await forgetAnswers();
    loadStats();
  }

  const aiKind = $derived(serviceInfo(settings.ai.service)?.kind);

  async function makeRestorePoint() {
    restorePoint = "creating";
    restorePoint = await createRestorePoint().catch(() => "failed" as const);
  }
</script>

<h1>{t("settings.title")}</h1>

<div class="card list">
  <label class="row">
    <span>{t("settings.language")}</span>
    <select value={getLanguage()} onchange={(e) => setLanguage(e.currentTarget.value as Language)}>
      {#each languages as lang (lang.code)}<option value={lang.code}>{lang.name}</option>{/each}
    </select>
  </label>
  <div class="row theme">
    <span id="theme-label">
      {t("settings.theme")}
      <span class="muted hint">{t("settings.themeHint")}</span>
    </span>
    <div class="segmented" role="radiogroup" aria-labelledby="theme-label">
      {#each themes as theme (theme)}
        <button role="radio" aria-checked={getTheme() === theme} class:on={getTheme() === theme} onclick={() => setTheme(theme)}>
          {t(`settings.themes.${theme}`)}
        </button>
      {/each}
    </div>
  </div>
  <label class="row">
    <span>
      {t("settings.largeText")}
      <span class="muted hint">{t("settings.largeTextHint")}</span>
    </span>
    <input type="checkbox" bind:checked={settings.largeText} onchange={() => { saveSettings(); applyTextSize(); }} />
  </label>
  <label class="row">
    <span>
      {t("settings.technical")}
      <span class="muted hint">{t("settings.technicalHint")}</span>
    </span>
    <input type="checkbox" bind:checked={settings.technical} onchange={saveSettings} />
  </label>
  <label class="row">
    <span>
      {t("settings.quarantineDays")}
      <span class="muted hint">{t("settings.quarantineDaysHint")}</span>
    </span>
    <select value={quarantine.keepDays} disabled={saving} onchange={(e) => changeQuarantine({ keepDays: Number(e.currentTarget.value) })}>
      {#each KEEP_DAYS_CHOICES as days (days)}<option value={days}>{t("settings.days", { days })}</option>{/each}
    </select>
  </label>
  <div class="row column">
    <label class="inner">
      <span>
        {t("settings.quarantineDrive")}
        <span class="muted hint">{t("settings.quarantineDriveHint", { drive: quarantine.systemDrive })}</span>
      </span>
      <select value={quarantine.drive ?? ""} disabled={saving || (!quarantine.drives.length && !quarantine.drive)}
              onchange={(e) => changeQuarantine({ drive: e.currentTarget.value || null })}>
        <option value="">{t("settings.driveDefault", { drive: quarantine.systemDrive })}</option>
        {#each quarantine.drives as d (d.mountPoint)}
          <option value={d.mountPoint}>{t(d.label ? "settings.driveOption" : "settings.driveOptionNoLabel", { drive: d.mountPoint, label: d.label, free: formatBytes(d.freeBytes) })}</option>
        {/each}
        {#if driveMissing}<option value={quarantine.drive}>{t("settings.driveMissing", { drive: quarantine.drive ?? "" })}</option>{/if}
      </select>
    </label>
    {#if !quarantine.drives.length && !quarantine.drive}
      <span class="muted note">{t("settings.noOtherDrive")}</span>
    {:else if quarantine.drive}
      <span class="note ok">{t("settings.driveChosen", { drive: quarantine.drive, system: quarantine.systemDrive })}</span>
    {/if}
    {#if saveError}<span class="note bad" role="alert">{t("settings.saveFailed", { error: saveError })}</span>{/if}
  </div>
  <button class="row hidden-row" onclick={() => onNavigate("hidden")}>
    <span class="hidden-icon"><EyeOffIcon size={18} /></span>
    <span class="grow">
      {t("settings.hidden")}
      <span class="muted hint">{t("settings.hiddenHint")}</span>
    </span>
    {#if hiddenCount}<span class="count">{hiddenCount}</span>{/if}
    <span class="manage">{t("settings.manage")}<Icon name="chevronRight" size={14} /></span>
  </button>
  <div class="row restore">
    <label class="restore-toggle">
      <span>
        {t("settings.restorePoint")}
        <span class="muted hint">{t("settings.restorePointHint")}</span>
      </span>
      <input type="checkbox" bind:checked={settings.restorePoint} onchange={saveSettings} />
    </label>
    <div class="restore-now">
      <button class="btn btn-ghost" disabled={restorePoint === "creating"} onclick={makeRestorePoint}>{t("settings.restorePointNow")}</button>
      {#if restorePoint}
        <span class="status" class:ok={restorePoint === "created"} class:bad={restorePoint !== "created" && restorePoint !== "creating"} role="status">
          {t(`settings.restorePointStatus.${restorePoint}`)}
        </span>
      {/if}
    </div>
  </div>
</div>

<h2 class="section">{t("ai.settings.title")} <span class="new">{t("ai.settings.new")}</span></h2>
<div class="card list ai">
  <div class="row theme">
    <span id="ai-mode-label">
      {t("ai.settings.mode")}
      <span class="muted hint">{t(`ai.settings.modeHint.${settings.ai.mode}`)}</span>
    </span>
    <div class="segmented" role="radiogroup" aria-labelledby="ai-mode-label">
      {#each modes as mode (mode)}
        <button role="radio" aria-checked={settings.ai.mode === mode} class:on={settings.ai.mode === mode} onclick={() => setMode(mode)}>
          {t(`ai.settings.modes.${mode}`)}
        </button>
      {/each}
    </div>
  </div>
  {#if settings.ai.mode !== "off"}
    <div class="row block">
      {#if settings.ai.mode === "online"}
        <div class="segmented tabs" role="tablist">
          <button role="tab" aria-selected={aiTab === "cli"} class:on={aiTab === "cli"} onclick={() => (aiTab = "cli")}><Icon name="ticket" />{t("ai.setup.tabCli")}</button>
          <button role="tab" aria-selected={aiTab === "api"} class:on={aiTab === "api"} onclick={() => (aiTab = "api")}><Icon name="key" />{t("ai.setup.tabApi")}</button>
        </div>
      {/if}
      {#key settings.ai.mode === "local" ? "local" : aiTab}
        <AiServicePicker kind={settings.ai.mode === "local" ? "local" : aiTab} />
      {/key}
    </div>
    <label class="row">
      <span>
        {t("ai.settings.confirm")}
        <span class="muted hint">{t("ai.settings.confirmHint")}</span>
      </span>
      <input type="checkbox" bind:checked={settings.ai.confirm} onchange={saveSettings} />
    </label>
    <label class="row">
      <span>
        {t("ai.settings.parallel")}
        <span class="muted hint">{t("ai.settings.parallelHint")}</span>
      </span>
      <select value={settings.ai.parallel} onchange={(e) => { settings.ai.parallel = Number(e.currentTarget.value); saveSettings(); }}>
        {#each AI_PARALLEL_CHOICES as n (n)}<option value={n}>{n}</option>{/each}
      </select>
    </label>
  {/if}
  <div class="row">
    <span>
      {t("ai.settings.month")}
      <span class="muted hint">
        {[
          t((stats?.askedThisMonth ?? 0) === 1 ? "ai.settings.monthOne" : "ai.settings.monthCount", { count: stats?.askedThisMonth ?? 0 }),
          ...(aiKind ? [t(aiKind === "cli" ? "ai.settings.costCli" : aiKind === "local" ? "ai.settings.costLocal" : "ai.settings.costApi")] : []),
        ].join(" · ")}
      </span>
    </span>
  </div>
  <div class="row">
    <span>
      {t("ai.settings.saved")}
      <span class="muted hint">{t((stats?.answers ?? 0) === 1 ? "ai.settings.savedOne" : "ai.settings.savedCount", { count: stats?.answers ?? 0 })}</span>
    </span>
    <button class="btn btn-ghost" disabled={!stats?.answers} onclick={clearAnswers}>{t("ai.settings.clear")}</button>
  </div>
  <div class="row block">
    <span>{t("ai.settings.never")}</span>
    <div class="never">
      {#each ["contents", "username", "apps", "personal"] as item (item)}<span class="never-item"><Icon name="checkCircle" />{t(`ai.settings.neverItems.${item}`)}</span>{/each}
    </div>
  </div>
</div>


<h2 class="section">{t("settings.updates")}</h2>
<div class="card list">
  <div class="row update" aria-live="polite">
    <span class="grow">
      <span class="version">{t("settings.version", { version })}</span>
      {#if phase.kind === "idle"}
        <span class="muted hint">{t("settings.updateIdle")}</span>
      {:else if phase.kind === "checking"}
        <span class="muted hint">{t("settings.updateChecking")}</span>
      {:else if phase.kind === "upToDate"}
        <span class="hint ok">{t("settings.updateUpToDate")}</span>
      {:else if phase.kind === "available"}
        <span class="hint available">{t("settings.updateAvailable", { version: phase.info.version })}</span>
        <span class="muted hint">{t("settings.updateRestartNote")}</span>
      {:else if phase.kind === "downloading"}
        <span class="muted hint">
          {phase.percent === null ? t("settings.updateDownloadingNoSize") : t("settings.updateDownloading", { percent: phase.percent })}
        </span>
        <span class="bar" class:indeterminate={phase.percent === null}><span style:width="{phase.percent ?? 30}%"></span></span>
      {:else if phase.kind === "installing"}
        <span class="muted hint">{t("settings.updateInstalling", { version: phase.info.version })}</span>
        <span class="bar indeterminate"><span style:width="30%"></span></span>
      {:else if phase.kind === "failed"}
        <span class="hint bad" role="alert">
          {#if phase.offline}{t("settings.updateOffline")}
          {:else if phase.info}{t("settings.updateInstallFailed", { error: phase.error })}
          {:else}{t("settings.updateFailed", { error: phase.error })}{/if}
        </span>
      {/if}
    </span>
    {#if phase.kind === "available"}
      <button class="btn btn-primary" onclick={install}><Icon name="download" />{t("settings.updateNow")}</button>
    {:else if phase.kind === "failed" && phase.info}
      <button class="btn btn-primary" onclick={install}><Icon name="refresh" />{t("settings.updateRetry")}</button>
    {:else if phase.kind !== "downloading" && phase.kind !== "installing"}
      <button class="btn btn-ghost" disabled={phase.kind === "checking"} onclick={check}>
        <Icon name="refresh" />
        {phase.kind === "failed" ? t("settings.updateRetry") : t("settings.updateCheck")}
      </button>
    {/if}
  </div>
  <label class="row">
    <span>
      {t("settings.updateAuto")}
      <span class="muted hint">{t("settings.updateAutoHint")}</span>
    </span>
    <input type="checkbox" bind:checked={settings.checkUpdates} onchange={saveSettings} />
  </label>
</div>

<style>
  .list { margin-top: 16px; }
  .row { display: flex; align-items: center; justify-content: space-between; padding: 14px 18px; border-bottom: 1px solid var(--border); }
  .row:last-child { border-bottom: none; }
  .row input { width: 18px; height: 18px; accent-color: var(--accent); flex: none; }
  .restore, .column { flex-direction: column; align-items: stretch; gap: 8px; }
  .inner { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .note { font-size: 12.5px; }
  .note.ok { color: var(--safe-fg); font-weight: 600; }
  .note.bad { color: var(--check-fg); font-weight: 600; }
  label.row { gap: 16px; }
  select { flex: none; max-width: 300px; }
  .restore-toggle { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .hint { display: block; font-size: 12px; margin-top: 2px; }
  .restore-now { display: flex; align-items: center; gap: 12px; }
  .restore-now .btn:disabled { opacity: 0.5; cursor: default; }
  .status { font-size: 12.5px; }
  .status.ok { color: var(--safe-fg); font-weight: 600; }
  .status.bad { color: var(--check-fg); font-weight: 600; }
  .theme { gap: 16px; }
  .segmented { display: inline-flex; flex: none; background: var(--track); border-radius: 8px; padding: 3px; }
  .segmented button { border: none; background: none; padding: 5px 14px; font-size: 12.5px; border-radius: 6px; }
  .segmented .on { background: var(--raised); font-weight: 600; box-shadow: var(--shadow); }
  select { font-family: inherit; padding: 4px 8px; border-radius: var(--radius-sm); border: 1px solid var(--border-strong); }
  .hidden-row { width: 100%; gap: 12px; border: none; border-bottom: 1px solid var(--border); background: none; text-align: left; font-size: inherit; color: inherit; }
  .hidden-row:hover { background: var(--hover); }
  .hidden-icon { display: flex; color: var(--text-muted); }
  .grow { flex: 1; min-width: 0; }
  .count { flex: none; font-size: 12px; font-weight: 600; background: var(--track); border-radius: 10px; padding: 1px 8px; }
  .manage { display: inline-flex; align-items: center; gap: 2px; flex: none; color: var(--accent); font-weight: 600; font-size: 13px; }
  .section { display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 600; color: var(--text-muted); letter-spacing: 0.3px; text-transform: uppercase; margin: 22px 0 0; }
  .new { font-size: 10px; font-weight: 700; padding: 2px 7px; border-radius: 8px; background: var(--ai); color: var(--on-ai); }
  .ai { border-color: var(--tip-border); }
  .block { flex-direction: column; align-items: stretch; gap: 10px; }
  .tabs { align-self: flex-start; }
  .never { display: flex; flex-wrap: wrap; gap: 6px 18px; font-size: 13px; }
  .never-item { display: inline-flex; align-items: center; gap: 6px; }
  .never-item :global(.icon) { color: var(--safe-fg); }
  .row .btn:disabled { opacity: 0.5; cursor: default; }
  .update { gap: 14px; }
  .update .btn { flex: none; }
  .update .btn:disabled { opacity: 0.5; cursor: default; }
  .version { font-weight: 600; }
  .hint.ok { color: var(--safe-fg); font-weight: 600; }
  .hint.bad { color: var(--check-fg); font-weight: 600; }
  .hint.available { color: var(--accent); font-weight: 600; }
  .bar { display: block; height: 6px; margin-top: 8px; border-radius: 3px; background: var(--track); overflow: hidden; max-width: 360px; }
  .bar > span { display: block; height: 100%; border-radius: 3px; background: var(--accent); transition: width 0.15s; }
  .bar.indeterminate > span { animation: slide 1.2s ease-in-out infinite; }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(340%); } }
</style>
