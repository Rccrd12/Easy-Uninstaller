<script lang="ts">
  import Icon from "./Icon.svelte";
  import { driveName, offload } from "../offload.svelte";
  import { fileCount, formatBytes } from "../format";
  import { t } from "../i18n/index.svelte";

  /** The two cards of "Free up space without deleting anything"; their details open in the side panel. */
  let { focusedId, onFocus }: { focusedId: string | null; onFocus: (id: string) => void } = $props();

  const od = $derived(offload.scan?.oneDrive);
  const mv = $derived(offload.scan?.moveFiles);
  /** OneDrive is set up and has something to free, or is freeing it now. */
  const showOneDrive = $derived(!!od && od.folders.length > 0 && (od.fileCount > 0 || od.freeingCount > 0));
  /** There is another drive, and something to move or to move back. */
  const showMove = $derived(!!mv && mv.drives.length > 0 && (mv.fileCount > 0 || mv.moved.length > 0));
  const target = $derived(mv?.drives[0]);
  const movedBytes = $derived(mv?.moved.reduce((n, m) => n + m.sizeBytes, 0) ?? 0);
</script>

{#if offload.scanning && !offload.scan}
  <h2>{t("offload.section")}</h2>
  <div class="card group waiting" role="status"><span class="spinner" aria-hidden="true"></span><span class="muted">{t("offload.scanning")}</span></div>
{:else if showOneDrive || showMove}
  <h2>{t("offload.section")}</h2>
  {#if showOneDrive && od}
    <div class="card group" class:focused={focusedId === "offload-onedrive"} role="button" tabindex="0"
         onclick={() => onFocus("offload-onedrive")} onkeydown={(e) => e.key === "Enter" && onFocus("offload-onedrive")}>
      <span class="kind"><Icon name="cloud" size={18} /></span>
      <div class="info">
        <div class="title">{t("offload.onedrive.title")}</div>
        <div class="muted desc">
          {#if od.fileCount > 0}{t("offload.onedrive.description")} · {fileCount(od.fileCount)}{:else}{t("offload.onedrive.freeingNote", { size: formatBytes(od.freeingBytes) })}{/if}
        </div>
      </div>
      {#if od.fileCount > 0}
        <span class="tag tag-info">{t("offload.onedrive.tag")}</span>
        <span class="size">{formatBytes(od.sizeBytes)}</span>
      {:else}
        <span class="tag tag-gray">{t("offload.onedrive.freeingTag")}</span>
        <span class="size">{formatBytes(od.freeingBytes)}</span>
      {/if}
    </div>
  {/if}
  {#if showMove && mv && target}
    <div class="card group" class:focused={focusedId === "offload-move"} role="button" tabindex="0"
         onclick={() => onFocus("offload-move")} onkeydown={(e) => e.key === "Enter" && onFocus("offload-move")}>
      <span class="kind"><Icon name="drive" size={18} /></span>
      <div class="info">
        <div class="title">{t("offload.move.title")}</div>
        <div class="muted desc">
          {#if mv.fileCount > 0}
            {t("offload.move.description", { drive: driveName(target) })} · {t("offload.move.freeOn", { size: formatBytes(target.freeBytes) })}
          {:else}
            {t("offload.move.movedNote", { count: fileCount(mv.moved.length), size: formatBytes(movedBytes) })}
          {/if}
        </div>
      </div>
      {#if mv.fileCount > 0}
        <span class="tag tag-info">{t("offload.move.tag")}</span>
        <span class="size">{formatBytes(mv.sizeBytes)}</span>
      {:else}
        <span class="tag tag-gray">{t("offload.move.movedTag")}</span>
        <span class="size">{formatBytes(movedBytes)}</span>
      {/if}
    </div>
  {/if}
{/if}

<style>
  h2 { font-size: 12px; font-weight: 600; color: var(--text-muted); letter-spacing: 0.3px; text-transform: uppercase; margin: 14px 0 6px; }
  .group { display: flex; align-items: center; gap: 12px; padding: 11px 14px; margin-bottom: 7px; border-width: 2px; border-color: transparent; cursor: pointer; }
  .group.focused { border-color: var(--accent); }
  .group.waiting { cursor: default; font-size: 13px; gap: 10px; }
  .kind { width: 18px; display: flex; justify-content: center; color: var(--accent); }
  .info { min-width: 0; flex: 1; }
  .title { font-size: 14px; font-weight: 600; }
  .desc { font-size: 12px; }
  /* The right margin is the room of the ⋯ button of the file groups, so sizes line up. */
  .size { font-size: 14px; font-weight: 600; width: 76px; text-align: right; margin-right: 40px; }
  .spinner { width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--accent-soft); border-top-color: var(--accent); animation: spin 0.9s linear infinite; flex: none; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
