<script lang="ts">
  import { createRestorePoint, QUARANTINE_DAYS } from "../api";
  import { getLanguage, languages, setLanguage, t, type Language } from "../i18n/index.svelte";
  import { saveSettings, settings } from "../settings.svelte";
  import type { RestorePointOutcome } from "../types";
  import { getTheme, setTheme, themes } from "../theme.svelte";

  const version = "0.1.0";

  let restorePoint = $state<RestorePointOutcome | "creating" | null>(null);

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
</style>
