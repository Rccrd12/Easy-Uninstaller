<script lang="ts">
  import Icon from "./Icon.svelte";
  import { formatBytes } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import { cancelIndexDownload, changeIndex, indexErrorText, installIndex, localIndex, retryIndex, watchIndex } from "../localIndex.svelte";

  /**
   * The search box of the Files screen. It opens once the files are indexed: a sentence
   * ("videos of the holidays over 500 MB") is read on this PC and looked for in the index.
   * Until then it is locked, and below it is what makes the index: the model's download
   * (or `onOptions`, to use one's own) and how far indexing is.
   * `onTidy` adds the "Tidy up Downloads" button beside it.
   */
  let {
    value = "",
    busy = false,
    onSearch,
    onTidy,
    onOptions,
  }: { value?: string; busy?: boolean; onSearch: (sentence: string) => void; onTidy?: () => void; onOptions?: () => void } = $props();

  let text = $state("");
  $effect.pre(() => {
    text = value;
  });

  watchIndex();
  const ix = $derived(localIndex.status);
  /** Something to look in: the model works and files are indexed (or only documents are wanted). */
  const open = $derived(!!ix?.ready && (ix.indexed > 0 || (!ix.settings.photos && !ix.settings.videos)));
  const count = (n: number) => n.toLocaleString(locale());
  const percent = (done: number, total: number) => (total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0);

  function submit(e: SubmitEvent) {
    e.preventDefault();
    const sentence = text.trim();
    if (open && sentence && !busy) onSearch(sentence);
  }
</script>

<form class="bar" class:ai={open} class:locked={!open} onsubmit={submit} role="search">
  <span class="lead" aria-hidden="true"><Icon name={open ? "sparkle" : "lock"} size={18} /></span>
  <input bind:value={text} maxlength="300" disabled={!open} placeholder={t(open ? "search.placeholderContent" : "search.placeholderLocked")} aria-label={t("search.title")} />
  <button class="btn btn-primary" type="submit" disabled={!open || busy || !text.trim()}>{t("search.button")}</button>
</form>
{#if !open || ix?.download || (ix?.indexing && ix.pending > 0) || onTidy}
<div class="under">
  {#if !ix}
    <span></span>
  {:else if ix.download}
    <div class="progress">
      <span class="muted note">{ix.download.verifying ? t("index.status.verifying") : t("index.status.downloading", { done: formatBytes(ix.download.doneBytes), total: formatBytes(ix.download.totalBytes) })}</span>
      <span class="meter" class:indeterminate={ix.download.verifying}><span style:width="{ix.download.verifying ? 30 : percent(ix.download.doneBytes, ix.download.totalBytes)}%"></span></span>
    </div>
    <button type="button" class="btn btn-ghost" onclick={cancelIndexDownload}>{t("index.cancel")}</button>
  {:else if !ix.ready}
    {#if ix.error}<span class="note bad" role="alert">{indexErrorText(ix.error)}</span>{:else if ix.settings.source === "server"}<span class="muted note">{t("search.needServer")}</span>{/if}
    {#if ix.downloadBytes > 0}
      <button type="button" class="btn btn-ai" disabled={localIndex.busy} onclick={installIndex}>
        <Icon name="download" />{t(ix.settings.source === "download" ? "search.getModel" : "index.download", { size: formatBytes(ix.downloadBytes) })}
      </button>
    {/if}
    {#if onOptions}<button type="button" class="btn btn-ghost" onclick={onOptions}>{t(ix.settings.source === "download" ? "search.useOwn" : "searchTopic.options")}</button>{/if}
  {:else if ix.indexing && ix.pending > 0}
    <div class="progress">
      <span class="muted note">{t("index.status.indexing", { done: count(ix.indexed), left: count(ix.pending) })}</span>
      <span class="meter"><span style:width="{percent(ix.indexed, ix.indexed + ix.pending)}%"></span></span>
    </div>
  {:else if !open && ix.settings.paused}
    <span class="muted note">{t("index.status.paused", { count: count(ix.indexed) })}</span>
    <button type="button" class="btn btn-ghost" onclick={() => changeIndex({ paused: false })}>{t("index.resume")}</button>
  {:else if !open && ix.error}
    <span class="note bad" role="alert">{indexErrorText(ix.error)}</span>
    <button type="button" class="btn btn-primary" disabled={localIndex.busy} onclick={retryIndex}>{t("index.retry")}</button>
    {#if onOptions}<button type="button" class="btn btn-ghost" onclick={onOptions}>{t("searchTopic.options")}</button>{/if}
  {:else if !open}
    <span class="muted note">{t("search.nothingIndexed")}</span>
    {#if onOptions}<button type="button" class="btn btn-ghost" onclick={onOptions}>{t("searchTopic.options")}</button>{/if}
  {:else}
    <span></span>
  {/if}
  {#if onTidy}
    <span class="grow"></span>
    <button type="button" class="btn btn-ghost tidy" onclick={onTidy}><Icon name="folderOpen" />{t("search.tidy")}</button>
  {/if}
</div>
{/if}

<style>
  .bar { display: flex; align-items: center; gap: 10px; background: var(--surface); border: 1px solid var(--border-strong); border-radius: var(--radius); padding: 6px 6px 6px 14px; }
  .bar.ai { border: 2px solid var(--ai); box-shadow: 0 0 0 4px var(--tip-bg); padding: 5px 5px 5px 13px; }
  .bar.locked { background: var(--bg); }
  .bar:focus-within { border-color: var(--accent); }
  .bar.ai:focus-within { border-color: var(--ai); }
  .lead { display: flex; color: var(--text-muted); }
  .bar.ai .lead { color: var(--ai); }
  input { flex: 1; min-width: 0; border: none; background: none; font-size: 14px; padding: 6px 0; outline: none; user-select: text; }
  input:disabled { cursor: not-allowed; }
  input::placeholder { color: var(--text-muted); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .under { display: flex; align-items: center; gap: 10px; margin: 8px 0 14px; min-height: 34px; }
  .note { font-size: 12.5px; line-height: 1.4; }
  .progress { display: flex; flex-direction: column; gap: 5px; flex: 0 1 380px; min-width: 0; }
  .progress .note { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meter { display: block; height: 6px; border-radius: 3px; background: var(--track); overflow: hidden; }
  .meter > span { display: block; height: 100%; background: var(--ai); border-radius: 3px; transition: width 0.25s; }
  .meter.indeterminate > span { animation: slide 1.2s ease-in-out infinite; }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(340%); } }
  .under .btn { white-space: nowrap; flex: none; }
  .bad { color: var(--check-fg); font-weight: 600; }
  .grow { flex: 1; }
  .tidy { flex: none; }
</style>
