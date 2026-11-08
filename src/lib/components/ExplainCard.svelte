<script lang="ts">
  import { knownFolder } from "../explain";
  import { formatBytes } from "../format";
  import { t } from "../i18n/index.svelte";
  import type { FileItem } from "../types";

  /** The "What is this?" panel for one folder or file, from the list built into the app. */
  let { item, groupTitle, onBack }: { item: FileItem; groupTitle: string; onBack: () => void } = $props();

  const folder = $derived(knownFolder(item.explainId));
  const shown = $derived(item.originalPath ?? item.path);
  const name = $derived(shown.split(/[\\/]/).pop() || shown);
  const parent = $derived(shown.slice(0, Math.max(shown.length - name.length - 1, 0)));
  const verdictIcon = { safe: "✅", check: "⚠️", keep: "🛑" } as const;
</script>

<button class="back" onclick={onBack}>← {t("explain.back", { group: groupTitle })}</button>
<div class="label">{t("explain.title")}</div>
<div class="head">
  <span class="icon" aria-hidden="true">{folder?.icon ?? "📁"}</span>
  <div class="head-text">
    <h3 title={shown}>{name}</h3>
    <div class="muted where" title={parent}>{parent} · {formatBytes(item.sizeBytes)}</div>
  </div>
</div>

{#if folder}
  <div class="verdict {folder.verdict}">{verdictIcon[folder.verdict]} <b>{t(`explain.verdict.${folder.verdict}`)}</b></div>
  <h4>{t("explain.what")}</h4>
  <p>{t(`explain.folders.${folder.id}.what`)}</p>
  <h4>{t("explain.ifRemoved")}</h4>
  <p>{t(`explain.folders.${folder.id}.ifRemoved`)}</p>
  <h4>{t("explain.madeBy")}</h4>
  <p>{folder.madeBy}</p>
  <div class="muted source">📚 {t("explain.source")}</div>
{:else}
  <div class="unknown">
    <b>{t("explain.unknownTitle", { name })}</b>
    <p>{t("explain.unknownHint")}</p>
    <div class="ai">
      <p class="muted">{t("explain.aiSoon")}</p>
      <button class="btn btn-ghost" disabled>✨ {t("explain.askAi")}</button>
    </div>
  </div>
{/if}

<style>
  .back { border: none; background: none; color: var(--accent); font-size: 12.5px; padding: 0; margin-bottom: 12px; text-align: left; cursor: pointer; }
  .label { font-size: 11px; font-weight: 600; letter-spacing: 0.3px; color: var(--text-muted); text-transform: uppercase; }
  .head { display: flex; gap: 10px; align-items: center; margin: 8px 0 12px; }
  .icon { font-size: 26px; flex: none; }
  .head-text { min-width: 0; }
  h3 { font-size: 17px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .where { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .verdict { padding: 8px 12px; border-radius: var(--radius-sm); font-size: 13.5px; margin-bottom: 4px; }
  .verdict.safe { background: var(--safe-bg); color: var(--safe-fg); }
  .verdict.check { background: var(--check-bg); color: var(--check-fg); }
  .verdict.keep { background: var(--keep-bg); color: var(--keep-fg); }
  h4 { font-size: 12.5px; margin-top: 12px; }
  p { font-size: 13px; line-height: 1.45; margin-top: 2px; }
  .source { font-size: 11.5px; margin-top: 14px; background: var(--bg); padding: 7px 10px; border-radius: var(--radius-sm); }
  .unknown { margin-top: 4px; }
  .unknown b { font-size: 13.5px; }
  .ai { margin-top: 14px; border: 1px dashed var(--tip-border); background: var(--tip-bg); border-radius: var(--radius-sm); padding: 10px 12px; }
  .ai p { font-size: 12.5px; margin: 0 0 8px; }
  .ai .btn:disabled { opacity: 0.6; cursor: default; }
</style>
