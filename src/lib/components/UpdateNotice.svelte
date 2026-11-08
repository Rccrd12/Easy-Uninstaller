<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import { install, update } from "../updates.svelte";

  /** Shows Settings, where the download progress is. */
  let { onOpenSettings }: { onOpenSettings: () => void } = $props();

  function start() {
    update.noticeDismissed = true;
    onOpenSettings();
    install();
  }
</script>

{#if update.phase.kind === "available" && !update.noticeDismissed}
  <div class="notice card" role="status">
    <strong>{t("updateNotice.title")}</strong>
    <p class="muted">{t("updateNotice.text", { version: update.phase.info.version })}</p>
    <div class="actions">
      <button class="btn btn-ghost" onclick={() => (update.noticeDismissed = true)}>{t("updateNotice.later")}</button>
      <button class="btn btn-primary" onclick={start}>{t("updateNotice.update")}</button>
    </div>
  </div>
{/if}

<style>
  .notice { position: fixed; right: 24px; bottom: 24px; width: 320px; padding: 16px 18px; box-shadow: var(--shadow-pop); z-index: 20; }
  p { margin: 4px 0 14px; font-size: 13px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
