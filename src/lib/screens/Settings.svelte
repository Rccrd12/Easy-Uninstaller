<script lang="ts">
  import { aiStats, createRestorePoint, QUARANTINE_DAYS } from "../api";
  import { forgetAnswers, serviceInfo } from "../ai.svelte";
  import AiServicePicker from "../components/AiServicePicker.svelte";
  import { getLanguage, languages, setLanguage, t, type Language } from "../i18n/index.svelte";
  import { saveSettings, settings } from "../settings.svelte";
  import type { AiMode, AiStats, RestorePointOutcome } from "../types";
  import { getTheme, setTheme, themes } from "../theme.svelte";

  const version = "0.1.0";

  let restorePoint = $state<RestorePointOutcome | "creating" | null>(null);

  // ── AI help ──
  const modes: AiMode[] = ["off", "online", "local"];
  let aiTab = $state<"cli" | "api">(serviceInfo(settings.ai.service)?.kind === "api" ? "api" : "cli");
  let stats = $state<AiStats | null>(null);
  const loadStats = () => aiStats().then((s) => (stats = s)).catch(() => {});
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
  <div class="row">
    <span>{t("settings.quarantineDays")}</span>
    <span class="muted">{t("settings.days", { days: QUARANTINE_DAYS })}</span>
  </div>
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
          <button role="tab" aria-selected={aiTab === "cli"} class:on={aiTab === "cli"} onclick={() => (aiTab = "cli")}>🎟️ {t("ai.setup.tabCli")}</button>
          <button role="tab" aria-selected={aiTab === "api"} class:on={aiTab === "api"} onclick={() => (aiTab = "api")}>🔑 {t("ai.setup.tabApi")}</button>
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
      {#each ["contents", "username", "apps", "personal"] as item (item)}<span>✅ {t(`ai.settings.neverItems.${item}`)}</span>{/each}
    </div>
  </div>
</div>

<p class="muted about">{t("settings.about", { version })}</p>

<style>
  .list { margin-top: 16px; }
  .row { display: flex; align-items: center; justify-content: space-between; padding: 14px 18px; border-bottom: 1px solid var(--border); }
  .row:last-child { border-bottom: none; }
  .row input { width: 18px; height: 18px; accent-color: var(--accent); flex: none; }
  .restore { flex-direction: column; align-items: stretch; gap: 10px; }
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
  .about { margin-top: 14px; font-size: 12px; }
  .section { display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 600; color: var(--text-muted); letter-spacing: 0.3px; text-transform: uppercase; margin: 22px 0 0; }
  .new { font-size: 10px; font-weight: 700; padding: 2px 7px; border-radius: 8px; background: var(--ai); color: var(--on-ai); }
  .ai { border-color: var(--tip-border); }
  .block { flex-direction: column; align-items: stretch; gap: 10px; }
  .tabs { align-self: flex-start; }
  .never { display: flex; flex-wrap: wrap; gap: 6px 18px; font-size: 13px; }
  .row .btn:disabled { opacity: 0.5; cursor: default; }
</style>
