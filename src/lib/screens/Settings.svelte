<script lang="ts">
  import EyeOffIcon from "../components/EyeOffIcon.svelte";
  import Icon from "../components/Icon.svelte";
  import { createRestorePoint, KEEP_DAYS_CHOICES, listExclusions } from "../api";
  import { formatBytes } from "../format";
  import { getLanguage, languages, setLanguage, t, type Language } from "../i18n/index.svelte";
  import { quarantine, saveQuarantine, saveSettings, settings } from "../settings.svelte";
  import type { Screen } from "../screens";
  import type { RestorePointOutcome } from "../types";
  import { getTheme, setTheme, themes } from "../theme.svelte";
  // Only this field ends up in the bundle; the release workflow keeps it in sync with the tag.
  import { version } from "../../../package.json";

  let { onNavigate }: { onNavigate: (s: Screen) => void } = $props();

  let hiddenCount = $state<number | null>(null);
  $effect(() => {
    listExclusions()
      .then((l) => (hiddenCount = l.length))
      .catch(() => (hiddenCount = null));
  });

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

<p class="muted about">{t("settings.about", { version })}</p>

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
  .about { margin-top: 14px; font-size: 12px; }
</style>
