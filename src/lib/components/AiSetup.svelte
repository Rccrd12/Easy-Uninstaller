<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { IconName } from "../icons";
  import { ai, aiReady, serviceInfo, turnOff } from "../ai.svelte";
  import { t } from "../i18n/index.svelte";
  import { saveSettings, settings } from "../settings.svelte";
  import type { AiMode } from "../types";
  import AiServicePicker from "./AiServicePicker.svelte";

  /**
   * Turning the AI help on, in two steps: where it runs (online, on this PC, or not at
   * all), then which service and model. Shown the first time "Ask AI" is pressed.
   */
  const current = serviceInfo(settings.ai.service);
  let step = $state<1 | 2>(1);
  let choice = $state<AiMode>(settings.ai.mode !== "off" ? settings.ai.mode : current?.kind === "local" ? "local" : "online");
  let tab = $state<"cli" | "api">(current?.kind === "api" ? "api" : "cli");

  const then = ai.setup?.then;

  const CHOICES: { mode: AiMode; icon: IconName | ""; lines: string[] }[] = [
    { mode: "online", icon: "globe", lines: ["a", "b", "c"] },
    { mode: "local", icon: "desktop", lines: ["a", "b", "c"] },
    { mode: "off", icon: "", lines: ["a"] },
  ];

  function close() {
    ai.setup = null;
  }

  function next() {
    if (choice === "off") {
      turnOff();
      close();
      return;
    }
    step = 2;
  }

  const ready = $derived(aiReady() || (!!settings.ai.service && !!settings.ai.model && pickedKind() === (choice === "local" ? "local" : tab)));

  function pickedKind() {
    return serviceInfo(settings.ai.service)?.kind;
  }

  function done() {
    settings.ai.mode = choice;
    saveSettings();
    close();
    then?.();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="ai-setup-title">
    {#if step === 1}
      <div class="top">
        <h2 id="ai-setup-title"><Icon name="sparkle" /> {t("ai.setup.title")}</h2>
        <span class="muted step">{t("ai.setup.step", { step: 1 })}</span>
      </div>
      <p class="lead">{t("ai.setup.lead")} <b>{t("ai.setup.leadBold")}</b>{t("ai.setup.leadEnd")}</p>
      <div class="choices" role="radiogroup" aria-labelledby="ai-setup-title">
        {#each CHOICES as c (c.mode)}
          <button class="choice" class:on={choice === c.mode} role="radio" aria-checked={choice === c.mode} onclick={() => (choice = c.mode)}>
            <b>{#if c.icon}<Icon name={c.icon} /> {/if}{t(`ai.setup.${c.mode}.title`)}</b>
            <ul>
              {#each c.lines as line (line)}<li>{t(`ai.setup.${c.mode}.${line}`)}</li>{/each}
            </ul>
          </button>
        {/each}
      </div>
      <div class="foot">
        <span class="muted">{t("ai.setup.changeLater")}</span>
        <button class="btn btn-ai" onclick={next}>{choice === "off" ? t("common.close") : t("ai.setup.continue")}</button>
      </div>
    {:else}
      <div class="top">
        <h2 id="ai-setup-title"><Icon name={choice === "local" ? "desktop" : "globe"} /> {choice === "local" ? t("ai.setup.localTitle") : t("ai.setup.onlineTitle")}</h2>
        <span class="muted step">{t("ai.setup.step", { step: 2 })}</span>
      </div>
      {#if choice === "online"}
        <div class="segmented" role="tablist">
          <button role="tab" aria-selected={tab === "cli"} class:on={tab === "cli"} onclick={() => (tab = "cli")}><Icon name="key" /> {t("ai.setup.tabCli")}</button>
          <button role="tab" aria-selected={tab === "api"} class:on={tab === "api"} onclick={() => (tab = "api")}><Icon name="key" /> {t("ai.setup.tabApi")}</button>
        </div>
      {/if}
      <div class="body">
        {#key choice === "local" ? "local" : tab}
          <AiServicePicker kind={choice === "local" ? "local" : tab} />
        {/key}
      </div>
      <div class="foot line">
        <button class="link" onclick={() => (step = 1)}><Icon name="arrowLeft" /> {t("ai.setup.back")}</button>
        <button class="btn btn-ai" disabled={!ready} onclick={done}>{then ? t("ai.setup.doneAsk") : t("ai.setup.done")}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.32); display: flex; align-items: center; justify-content: center; z-index: 20; padding: 24px; }
  .dialog { width: 900px; max-width: 100%; max-height: calc(100vh - 48px); overflow-y: auto; padding: 24px 28px 22px; box-shadow: 0 16px 48px rgba(0, 0, 0, 0.22); }
  .top { display: flex; justify-content: space-between; align-items: baseline; gap: 12px; }
  h2 { font-size: 20px; font-weight: 600; }
  .step { font-size: 12.5px; flex: none; }
  .lead { margin: 8px 0 16px; font-size: 14px; line-height: 1.45; color: var(--text-muted); }
  .lead b { color: var(--text); }
  .choices { display: grid; grid-template-columns: 1.2fr 1fr 0.8fr; gap: 12px; }
  .choice { text-align: left; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface); padding: 14px 16px; display: flex; flex-direction: column; gap: 8px; }
  .choice.on { border: 2px solid var(--ai); padding: 13px 15px; background: var(--tip-bg); }
  .choice b { font-size: 15px; }
  .choice ul { list-style: none; display: flex; flex-direction: column; gap: 6px; font-size: 13px; line-height: 1.4; }
  .foot { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 20px; }
  .foot .muted { font-size: 12.5px; }
  .foot.line { border-top: 1px solid var(--border); padding-top: 16px; }
  .btn-ai { padding: 9px 20px; font-size: 14px; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .link { border: none; background: none; color: var(--accent); padding: 0; font-size: 13px; }
  .segmented { display: inline-flex; background: var(--track); border-radius: 8px; padding: 3px; margin: 12px 0 4px; }
  .segmented button { border: none; background: none; padding: 6px 14px; font-size: 13px; border-radius: 6px; }
  .segmented .on { background: var(--raised); font-weight: 600; box-shadow: var(--shadow); }
  .body { margin-top: 12px; }
  @media (max-width: 760px) {
    .choices { grid-template-columns: 1fr; }
  }
</style>
