<script lang="ts">
  import Icon from "./Icon.svelte";
  import { aiReady, openSetup } from "../ai.svelte";
  import { t } from "../i18n/index.svelte";
  import { indexReady, watchIndex } from "../localIndex.svelte";
  import { settings } from "../settings.svelte";

  /**
   * The search box of the Files screen. With the AI on, a whole sentence ("videos over
   * 500 MB I haven't opened in a year") becomes a filter; with it off, words of the name.
   * `onTidy` adds the "Tidy up Downloads" button beside the note.
   */
  let { value = "", busy = false, onSearch, onTidy }: { value?: string; busy?: boolean; onSearch: (sentence: string) => void; onTidy?: () => void } = $props();

  let text = $state("");
  $effect.pre(() => {
    text = value;
  });

  watchIndex();
  const ready = $derived(aiReady());
  /** No AI, but the local index: a few words of what the files show. */
  const content = $derived(!ready && indexReady());

  function submit(e: SubmitEvent) {
    e.preventDefault();
    const sentence = text.trim();
    if (sentence && !busy) onSearch(sentence);
  }
</script>

<form class="bar" class:ai={ready || content} onsubmit={submit} role="search">
  <span class="lead" aria-hidden="true"><Icon name={ready || content ? "sparkle" : "search"} size={18} /></span>
  <input bind:value={text} maxlength="300" placeholder={t(ready ? "search.placeholderAi" : content ? "search.placeholderContent" : "search.placeholderName")} aria-label={t("search.title")} />
  <button class="btn btn-primary" type="submit" disabled={busy || !text.trim()}>{t("search.button")}</button>
</form>
<div class="under">
  {#if !ready && !content}
    <span class="muted note">{t("search.aiOff")} <button type="button" class="link" onclick={() => openSetup()}>{t("search.turnOn")}</button></span>
  {/if}
  {#if onTidy}
    <span class="grow"></span>
    <button type="button" class="btn btn-ghost tidy" onclick={onTidy}><Icon name="folderOpen" />{t("search.tidy")}</button>
  {/if}
</div>

<style>
  .bar { display: flex; align-items: center; gap: 10px; background: var(--surface); border: 1px solid var(--border-strong); border-radius: var(--radius); padding: 6px 6px 6px 14px; }
  .bar.ai { border: 2px solid var(--ai); box-shadow: 0 0 0 4px var(--tip-bg); padding: 5px 5px 5px 13px; }
  .bar:focus-within { border-color: var(--accent); }
  .bar.ai:focus-within { border-color: var(--ai); }
  .lead { display: flex; color: var(--text-muted); }
  .bar.ai .lead { color: var(--ai); }
  input { flex: 1; min-width: 0; border: none; background: none; font-size: 14px; padding: 6px 0; outline: none; user-select: text; }
  input::placeholder { color: var(--text-muted); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .under { display: flex; align-items: center; gap: 10px; margin: 8px 0 14px; min-height: 34px; }
  .note { font-size: 12.5px; line-height: 1.4; }
  .note :global(.icon) { flex: none; }
  .link { border: none; background: none; color: var(--accent); padding: 0; font-size: inherit; }
  .grow { flex: 1; }
  .tidy { flex: none; }
</style>
