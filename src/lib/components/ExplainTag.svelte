<script lang="ts">
  import { knownFolder } from "../explain";
  import { t } from "../i18n/index.svelte";

  /** Short verdict for a folder: its kind when it can go or needs a look, a warning when it should stay. */
  let { explainId }: { explainId: string | null } = $props();
  const folder = $derived(knownFolder(explainId));
</script>

{#if !folder}
  <span class="tag tag-gray">{t("explain.tag.unknown")}</span>
{:else if folder.verdict === "keep"}
  <span class="tag tag-keep">{t("explain.tag.keep")}</span>
{:else}
  <span class="tag {folder.verdict === 'safe' ? 'tag-safe' : 'tag-check'}">{t(`explain.folders.${folder.id}.label`)}</span>
{/if}
