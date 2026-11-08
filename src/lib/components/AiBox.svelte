<script lang="ts">
  import { aiPreview } from "../api";
  import { ai, aiErrorText, aiReady, answerFor, askAbout, forgetAnswers, openSetup, serviceInfo, serviceLabel } from "../ai.svelte";
  import { fileCount, formatBytes } from "../format";
  import { getLanguage, locale, t } from "../i18n/index.svelte";
  import { quarantine, saveSettings, settings } from "../settings.svelte";
  import type { AiPreview, FileItem } from "../types";

  /**
   * The AI part of "What is this?" for a folder the app does not know: the button,
   * what will be sent (when the user wants to see it first), and the answer, with
   * what the app itself checked on this PC shown apart.
   */
  let { item }: { item: FileItem } = $props();

  const answer = $derived(answerFor(item.path));
  const asking = $derived(!!ai.asking[item.path.toLowerCase()]);
  let preview = $state<AiPreview | null>(null);
  let showText = $state(false);
  let dontAsk = $state(false);
  let error = $state<string | null>(null);

  const isCli = $derived(serviceInfo(settings.ai.service)?.kind === "cli");
  const isLocal = $derived(serviceInfo(settings.ai.service)?.kind === "local");
  const service = $derived(t(`ai.services.${settings.ai.service}.name`));

  /** The button: set the AI up first if needed, show what is sent if asked to, then ask. */
  function start() {
    error = null;
    if (!aiReady()) {
      openSetup(start);
      return;
    }
    if (settings.ai.confirm) {
      aiPreview(item, getLanguage())
        .then((p) => (preview = p))
        .catch((e) => (error = aiErrorText(e)));
    } else {
      ask();
    }
  }

  async function ask() {
    if (dontAsk) {
      settings.ai.confirm = false;
      saveSettings();
    }
    preview = null;
    error = null;
    try {
      await askAbout(item);
    } catch (e) {
      error = aiErrorText(e);
    }
  }

  async function askAgain() {
    await forgetAnswers([item.path]);
    start();
  }

  const monthYear = (secs: number) => new Date(secs * 1000).toLocaleDateString(locale(), { month: "long", year: "numeric" });
  const verdictIcon = { removable: "🟡", keep: "🔴", unknown: "⚪" } as const;
</script>

{#if answer}
  <div class="box">
    <div class="box-head">
      <span class="label">✨ {t("ai.answer.title")}</span>
      <span class="who">{serviceLabel(answer.service, answer.model)}</span>
    </div>
    <p class="what">{answer.what}</p>
    {#if answer.madeBy}<p class="made-by muted">{t("ai.answer.madeBy", { name: answer.madeBy })}</p>{/if}
    <div class="verdict {answer.verdict}">
      <b>{verdictIcon[answer.verdict]} {t(`ai.verdict.${answer.verdict}`)}</b>
      {#if answer.verdict !== "unknown"}· {t(`ai.confidence.${answer.confidence}`)}{/if}
      {#if answer.reason}<div class="reason">{answer.reason}</div>{/if}
    </div>
    {#if answer.overridden}
      <p class="override">🛡️ {t("ai.answer.overridden", { app: answer.facts.installedApp ?? "" })}</p>
    {/if}
    <div class="label facts-label">{t("ai.answer.checked")}</div>
    <ul class="facts">
      {#if answer.facts.installedApp}
        <li class="warn">⚠ {t("ai.answer.appInstalled", { app: answer.facts.installedApp })}</li>
      {:else}
        <li>✓ {t("ai.answer.noApp")}</li>
      {/if}
      {#if answer.facts.modified}<li>✓ {t("ai.answer.modified", { date: monthYear(answer.facts.modified) })}</li>{/if}
      <li>✓ {t("ai.answer.notWindows")}</li>
    </ul>
  </div>
  <p class="muted foot">{t("ai.answer.mayBeWrong", { days: quarantine.keepDays })}</p>
  <p class="muted foot">🔒 {t("ai.answer.sentOnly")} <button class="link" onclick={askAgain}>{t("ai.answer.askAgain")}</button></p>
{:else if asking}
  <div class="box">
    <span class="label">✨ {t("ai.box.title")}</span>
    <p class="asking"><span class="spinner" aria-hidden="true"></span>{t("ai.box.asking", { service })}</p>
    <p class="muted small">{t(isLocal ? "ai.box.askingLocal" : "ai.box.askingHint")}</p>
  </div>
{:else if preview}
  {@const s = preview.summary}
  <div class="box">
    <span class="label">✨ {t("ai.confirm.title")}</span>
    <p class="intro">{t("ai.confirm.intro", { service: serviceLabel(settings.ai.service, settings.ai.model) })}</p>
    <pre class="sent">{t("ai.confirm.folder")}: {s.path}
{t("ai.confirm.size")}: {formatBytes(s.sizeBytes)}, {fileCount(s.fileCount)}{#if s.modified}
{t("ai.confirm.modified")}: {monthYear(s.modified)}{/if}{#if s.subfolders.length}
{t("ai.confirm.subfolders")}: {s.subfolders.join(", ")}{/if}{#if s.names.length}
{t("ai.confirm.names", { shown: s.names.length, total: s.fileCount.toLocaleString(locale()) })}: {s.names.join(", ")}{/if}</pre>
    <button class="link small" onclick={() => (showText = !showText)}>{showText ? t("ai.confirm.hideText") : t("ai.confirm.showText")}</button>
    {#if showText}<pre class="sent exact">{preview.question}</pre>{/if}
    <ul class="promises">
      <li>🚫 {t("ai.confirm.never")}</li>
      {#if isCli}<li>🔒 {t("ai.confirm.noTools", { service })}</li>{/if}
      <li>💶 {t(isCli ? "ai.confirm.costCli" : isLocal ? "ai.confirm.costLocal" : "ai.confirm.costApi")}</li>
    </ul>
    <div class="buttons">
      <button class="btn btn-ai" onclick={ask}>✨ {t("ai.box.ask")}</button>
      <button class="btn btn-ghost" onclick={() => (preview = null)}>{t("common.cancel")}</button>
    </div>
    <label class="dont-ask"><input type="checkbox" bind:checked={dontAsk} /> {t("ai.confirm.dontAsk")}</label>
  </div>
{:else}
  <div class="box idle">
    <span class="label">✨ {t("ai.box.title")}</span>
    <p class="small">{t(aiReady() ? "ai.box.hint" : "ai.box.hintOff")}</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <button class="btn btn-ai" onclick={start}>✨ {error ? t("ai.box.retry") : t("ai.box.ask")}</button>
    {#if aiReady()}<p class="muted small via">{t("ai.box.via", { service: serviceLabel(settings.ai.service, settings.ai.model) })}</p>{/if}
  </div>
{/if}

<style>
  .box { margin-top: 14px; border: 1px solid var(--tip-border); background: var(--tip-bg); border-radius: var(--radius); padding: 12px 14px; }
  .box-head { display: flex; justify-content: space-between; align-items: baseline; gap: 8px; }
  .label { font-size: 11px; font-weight: 700; letter-spacing: 0.4px; text-transform: uppercase; color: var(--ai-fg); }
  .who { font-size: 11.5px; color: var(--ai-fg); text-align: right; }
  .what { font-size: 13px; line-height: 1.5; margin-top: 6px; user-select: text; }
  .made-by { font-size: 12px; margin-top: 2px; }
  .verdict { margin-top: 10px; padding: 8px 10px; border-radius: var(--radius-sm); font-size: 13px; }
  .verdict.removable { background: var(--check-bg); color: var(--check-fg); }
  .verdict.keep { background: var(--keep-bg); color: var(--keep-fg); }
  .verdict.unknown { background: var(--track); color: var(--text-muted); }
  .reason { font-size: 12.5px; margin-top: 3px; }
  .override { font-size: 12.5px; margin-top: 8px; }
  .facts-label { display: block; margin-top: 12px; color: var(--text-muted); }
  .facts { list-style: none; font-size: 12.5px; margin-top: 4px; display: flex; flex-direction: column; gap: 3px; }
  .facts li { color: var(--safe-fg); }
  .facts li.warn { color: var(--keep-fg); font-weight: 600; }
  .foot { font-size: 11.5px; margin-top: 8px; line-height: 1.4; }
  .link { border: none; background: none; color: var(--accent); padding: 0; font-size: inherit; }
  .small { font-size: 12px; }
  .asking { display: flex; align-items: center; gap: 8px; font-size: 13px; margin: 8px 0 4px; }
  .spinner { width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--tip-border); border-top-color: var(--ai); animation: spin 0.9s linear infinite; flex: none; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .intro { font-size: 12.5px; margin: 6px 0; }
  .sent { font-family: Consolas, "Cascadia Mono", monospace; font-size: 11px; line-height: 1.5; background: var(--surface); border: 1px solid var(--tip-border); border-radius: var(--radius-sm); padding: 8px 10px; white-space: pre-wrap; word-break: break-word; user-select: text; }
  .exact { margin-top: 6px; max-height: 180px; overflow-y: auto; }
  .promises { list-style: none; font-size: 12px; margin-top: 8px; display: flex; flex-direction: column; gap: 4px; line-height: 1.4; }
  .buttons { display: flex; gap: 8px; margin-top: 10px; }
  .dont-ask { display: flex; gap: 6px; align-items: center; font-size: 12px; margin-top: 8px; }
  .dont-ask input { accent-color: var(--ai); }
  .idle p.small { margin: 6px 0 10px; line-height: 1.4; }
  .error { font-size: 12.5px; color: var(--keep-fg); margin-bottom: 8px; }
  .via { margin-top: 6px; }
</style>
