<script lang="ts">
  import { knownFolder } from "../explain";
  import { t } from "../i18n/index.svelte";
  import type { AiAnswer } from "../types";

  /**
   * Short verdict for a folder: its kind when it can go or needs a look, a warning when it should stay.
   * A folder the app does not know shows the AI's answer when there is one, always in the AI's own
   * purple tag: the AI never gets the green "safe" one.
   */
  let { explainId, answer }: { explainId: string | null; answer?: AiAnswer } = $props();
  const folder = $derived(knownFolder(explainId));
</script>

{#if !folder && answer}
  <span class="tag tag-ai" title={answer.what}><i class="dot {answer.verdict}" aria-hidden="true"></i>✨ {answer.label || t(`ai.verdict.${answer.verdict}`)}</span>
{:else if !folder}
  <span class="tag tag-gray">{t("explain.tag.unknown")}</span>
{:else if folder.verdict === "keep"}
  <span class="tag tag-keep">{t("explain.tag.keep")}</span>
{:else}
  <span class="tag {folder.verdict === 'safe' ? 'tag-safe' : 'tag-check'}">{t(`explain.folders.${folder.id}.label`)}</span>
{/if}

<style>
  .tag-ai { display: inline-flex; align-items: center; gap: 4px; max-width: 170px; overflow: hidden; text-overflow: ellipsis; }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; }
  .dot.removable { background: var(--check-fg); }
  .dot.keep { background: var(--keep-fg); }
  .dot.unknown { background: var(--text-muted); }
</style>
