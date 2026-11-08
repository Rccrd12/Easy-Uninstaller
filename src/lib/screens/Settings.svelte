<script lang="ts">
  import { QUARANTINE_DAYS } from "../api";
  import { availableLanguages, t } from "../i18n";

  const version = "0.1.0";
</script>

<h1>{t("settings.title")}</h1>

<div class="card list">
  <label class="row">
    <span>{t("settings.language")}</span>
    <select>
      {#each availableLanguages as lang (lang)}<option>{lang}</option>{/each}
    </select>
  </label>
  <div class="row">
    <span>{t("settings.quarantineDays")}</span>
    <span class="muted">{t("settings.days", { days: QUARANTINE_DAYS })}</span>
  </div>
  <label class="row">
    <span>{t("settings.restorePoint")}</span>
    <input type="checkbox" checked />
  </label>
</div>

<p class="muted about">{t("settings.about", { version })}</p>

<style>
  .list { margin-top: 16px; }
  .row { display: flex; align-items: center; justify-content: space-between; padding: 14px 18px; border-bottom: 1px solid var(--border); }
  .row:last-child { border-bottom: none; }
  .row input { width: 18px; height: 18px; accent-color: var(--accent); }
  select { font-family: inherit; padding: 4px 8px; border-radius: var(--radius-sm); border: 1px solid var(--border-strong); }
  .about { margin-top: 14px; font-size: 12px; }
</style>
