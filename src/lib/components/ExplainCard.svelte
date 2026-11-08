<script lang="ts">
  import { quarantine } from "../settings.svelte";
  import { knownFolder } from "../explain";
  import { formatBytes } from "../format";
  import { t } from "../i18n/index.svelte";
  import { answerFor } from "../ai.svelte";
  import type { FileItem } from "../types";
  import AiBox from "./AiBox.svelte";

  /**
   * The "What is this?" panel for one folder or file, from the list built into the app.
   * With `onMove`, it also offers to move the folder into quarantine, after a confirmation
   * that warns more strongly when the folder is better kept or unknown.
   */
  let {
    item,
    groupTitle,
    onBack,
    onMove,
    moving = false,
  }: { item: FileItem; groupTitle: string; onBack: () => void; onMove?: () => void; moving?: boolean } = $props();

  const folder = $derived(knownFolder(item.explainId));
  const shown = $derived(item.originalPath ?? item.path);
  const name = $derived(shown.split(/[\\/]/).pop() || shown);
  const parent = $derived(shown.slice(0, Math.max(shown.length - name.length - 1, 0)));
  const verdictIcon = { safe: "✅", check: "⚠️", keep: "🛑" } as const;

  let confirming = $state(false);
  /** Outside AppData (ProgramData): Windows asks for permission. */
  const needsAdmin = $derived(!/[\\/]AppData[\\/]/i.test(item.path));
  /** The AI's answer, only for folders the built-in list does not know. */
  const aiAnswer = $derived(folder ? undefined : answerFor(item.path));
  const risk = $derived(!folder ? (aiAnswer?.verdict === "keep" ? "keep" : "unknown") : folder.verdict === "keep" ? "keep" : "normal");
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
    {#if !aiAnswer}<p>{t("explain.unknownHint")}</p>{/if}
    <AiBox {item} />
  </div>
{/if}

{#if onMove}
  <div class="grow"></div>
  {#if confirming}
    <div class="confirm {risk}" role="alertdialog" aria-label={t("explain.move.title", { name })}>
      <b>{t(`explain.move.${risk}Title`, { name })}</b>
      {#if risk === "keep" && folder}
        <p>{t("explain.move.keepText")} {t(`explain.folders.${folder.id}.ifRemoved`)}</p>
      {:else if risk === "keep" && aiAnswer}
        <p>{t("ai.move.keepText")} {aiAnswer.reason}</p>
      {:else if risk === "unknown"}
        <p>{t("explain.move.unknownText")}</p>
      {/if}
      <p>{t("explain.move.restore", { days: quarantine.keepDays })}{needsAdmin ? ` ${t("explain.move.admin")}` : ""}</p>
      <div class="buttons">
        <button class="btn {risk === 'normal' ? 'btn-primary' : 'btn-danger'}" disabled={moving} onclick={onMove}>
          {moving ? t("files.moving") : t(risk === "normal" ? "explain.move.confirm" : "explain.move.confirmAnyway")}
        </button>
        <button class="btn btn-ghost" disabled={moving} onclick={() => (confirming = false)}>{t("common.cancel")}</button>
      </div>
    </div>
  {:else}
    <button class="btn {risk === 'normal' ? 'btn-primary' : 'btn-ghost'} wide" onclick={() => (confirming = true)}>
      {t("common.moveToQuarantine")} ({formatBytes(item.sizeBytes)})
    </button>
  {/if}
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
  .grow { flex: 1; min-height: 16px; }
  .wide { width: 100%; padding: 10px; }
  .confirm { border-radius: var(--radius-sm); padding: 12px; font-size: 13px; background: var(--bg); border: 1px solid var(--border-strong); }
  .confirm.keep { background: var(--keep-bg); color: var(--keep-fg); border-color: transparent; }
  .confirm.unknown { background: var(--check-bg); color: var(--check-fg); border-color: var(--check-border); }
  .confirm p { font-size: 12.5px; margin-top: 6px; }
  .buttons { display: flex; gap: 8px; margin-top: 10px; flex-wrap: wrap; }
  .btn:disabled { opacity: 0.5; cursor: default; }
</style>
