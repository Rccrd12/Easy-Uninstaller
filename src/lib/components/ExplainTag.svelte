<script lang="ts">
  import { knownFolder } from "../explain";
  import { t } from "../i18n/index.svelte";
  import type { AiAnswer } from "../types";

  /**
   * Short verdict for a folder: its kind when it can go or needs a look, a warning when it should stay.
   * A folder the app does not know shows the AI's verdict when there is one, in the colours of the
   * app's own tags with an "AI" mark in front. "Can likely go" is yellow, never the green "safe":
   * the AI's word is advice, not a check.
   */
  let { explainId, answer }: { explainId: string | null; answer?: AiAnswer } = $props();
  const folder = $derived(knownFolder(explainId));
  const aiClass = { removable: "tag-check", keep: "tag-keep", unknown: "tag-gray" } as const;
</script>

{#if !folder && answer}
  <span class="tag {aiClass[answer.verdict]} ai" title={t("ai.tag.title", { verdict: t(`ai.verdict.${answer.verdict}`), what: answer.what })}>
    <span class="mark">{t("ai.tag.mark")}</span>{t(`ai.tag.${answer.verdict}`)}
  </span>
{:else if !folder}
  <span class="tag tag-gray">{t("explain.tag.unknown")}</span>
{:else if folder.verdict === "keep"}
  <span class="tag tag-keep">{t("explain.tag.keep")}</span>
{:else}
  <span class="tag {folder.verdict === 'safe' ? 'tag-safe' : 'tag-check'}">{t(`explain.folders.${folder.id}.label`)}</span>
{/if}

<style>
  .ai { display: inline-flex; align-items: center; gap: 5px; padding-left: 3px; }
  .mark { font-size: 9.5px; font-weight: 700; letter-spacing: 0.3px; padding: 1px 5px; border-radius: 8px; background: var(--ai); color: var(--on-ai); }
</style>
