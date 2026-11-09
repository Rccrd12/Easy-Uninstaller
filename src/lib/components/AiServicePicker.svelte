<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { IconName } from "../icons";

  const NOTE_ICONS: Record<string, IconName> = { apiCost: "money" };
  import { onMount } from "svelte";
  import { aiModels, aiOpenPage, aiRemoveKey, aiServices, aiSetKey, aiTest } from "../api";
  import { ai, AI_SERVICES, aiConfig, aiErrorText, recommendedModel, serviceInfo, type AiKind, type AiServiceInfo } from "../ai.svelte";
  import { locale, t } from "../i18n/index.svelte";
  import { saveSettings, settings } from "../settings.svelte";
  import type { AiModel, AiServiceStatus } from "../types";

  /**
   * Picks one service of a kind (subscription programs, API keys, servers on this PC),
   * its key or address when it needs one, and its model, then tests it.
   * Writes straight into `settings.ai`; Windows keeps the keys.
   */
  let { kind }: { kind: AiKind } = $props();

  const list = $derived(AI_SERVICES.filter((s) => s.kind === kind));
  const selected = $derived(serviceInfo(settings.ai.service)?.kind === kind ? settings.ai.service : "");
  const info = $derived(serviceInfo(selected));

  let statuses = $state<Record<string, AiServiceStatus>>({});
  let checking = $state(false);
  let models = $state<AiModel[]>([]);
  let loadingModels = $state(false);
  let modelsError = $state<string | null>(null);
  let keyInput = $state("");
  let keyError = $state<string | null>(null);
  let urlInput = $state("");
  type Test = { state: "running" } | { state: "ok"; ms: number } | { state: "fail"; error: string };
  let test = $state<Test | null>(null);

  const status = (id: string) => statuses[id];
  const hasKey = $derived(!!statuses[selected]?.hasKey);
  const needsKeyFirst = $derived(!!info?.needsKey && !hasKey);
  const needsUrlFirst = $derived(info?.url === "required" && !(settings.ai.urls[selected] ?? "").trim());

  async function refresh() {
    checking = true;
    const found = await aiServices({ ...settings.ai.urls }).catch(() => [] as AiServiceStatus[]);
    statuses = Object.fromEntries(found.map((s) => [s.id, s]));
    checking = false;
    if (!selected) {
      // The first one that can answer, else the first of the list.
      const first = list.find((s) => usable(s)) ?? list[0];
      if (first) select(first.id);
    } else {
      loadModels();
    }
  }

  onMount(refresh);

  /** Installed, or a server that is on. */
  function usable(s: AiServiceInfo): boolean {
    const st = statuses[s.id];
    if (!st) return false;
    if (s.kind === "api") return s.id !== "custom" && st.hasKey;
    return st.found || startsItself(s.id);
  }

  function select(id: string) {
    if (settings.ai.service === id && models.length) return;
    settings.ai.service = id;
    settings.ai.model = "";
    settings.ai.modelLabel = "";
    saveSettings();
    test = null;
    keyInput = "";
    keyError = null;
    urlInput = settings.ai.urls[id] ?? "";
    loadModels();
  }

  async function loadModels() {
    models = [];
    modelsError = null;
    test = null;
    if (!info || needsKeyFirst || needsUrlFirst) return;
    loadingModels = true;
    const asked = selected;
    try {
      const found = await aiModels(aiConfig());
      if (asked !== selected) return;
      models = found;
      if (found.length && !found.some((m) => m.id === settings.ai.model)) {
        const best = recommendedModel(asked);
        settings.ai.model = found.find((m) => m.id === best)?.id ?? found[0].id;
      }
      settings.ai.modelLabel = found.find((m) => m.id === settings.ai.model)?.label ?? "";
      saveSettings();
    } catch (e) {
      if (asked === selected) modelsError = aiErrorText(e);
    } finally {
      loadingModels = false;
    }
  }

  function pickModel(id: string) {
    settings.ai.model = id;
    settings.ai.modelLabel = models.find((m) => m.id === id)?.label ?? "";
    saveSettings();
    test = null;
  }

  async function saveKey() {
    keyError = null;
    try {
      await aiSetKey(selected, keyInput);
      keyInput = "";
      statuses[selected] = { ...(statuses[selected] ?? { id: selected, found: true, models: null }), hasKey: true };
      loadModels();
    } catch (e) {
      keyError = aiErrorText(e);
    }
  }

  async function removeKey() {
    await aiRemoveKey(selected).catch(() => {});
    if (statuses[selected]) statuses[selected] = { ...statuses[selected], hasKey: false };
    models = [];
    test = null;
  }

  function saveUrl() {
    const value = urlInput.trim();
    if ((settings.ai.urls[selected] ?? "") === value) return;
    settings.ai.urls = { ...settings.ai.urls, [selected]: value };
    saveSettings();
    loadModels();
  }

  async function runTest() {
    test = { state: "running" };
    try {
      test = { state: "ok", ms: await aiTest(aiConfig()) };
    } catch (e) {
      test = { state: "fail", error: aiErrorText(e) };
    }
  }

  /** Not running, but the app starts it when a question needs it (Unsloth Studio). */
  const startsItself = (id: string) => !!status(id)?.canStart && settings.ai.autoStart;

  function setAutoStart(on: boolean) {
    settings.ai.autoStart = on;
    saveSettings();
    loadModels();
  }

  function badge(s: AiServiceInfo): { text: string; tone: "ok" | "warn" | "off" } | null {
    const st = status(s.id);
    if (!st) return null;
    if (s.kind === "cli") return st.found ? { text: t("ai.picker.found"), tone: "ok" } : { text: t("ai.picker.notInstalled"), tone: "off" };
    if (s.kind === "api") return st.hasKey ? { text: t("ai.picker.keySaved"), tone: "ok" } : null;
    if (s.url === "required" && !(settings.ai.urls[s.id] ?? "").trim()) return null;
    if (!st.found) return startsItself(s.id) ? { text: t("ai.picker.autoStarts"), tone: "ok" } : { text: t("ai.picker.off"), tone: "off" };
    if (s.needsKey && !st.hasKey) return { text: t("ai.picker.needsKey"), tone: "warn" };
    return { text: t("ai.picker.on"), tone: "ok" };
  }

  function subtitle(s: AiServiceInfo): string {
    if (s.kind !== "local") return t(`ai.services.${s.id}.sub`);
    const where = (settings.ai.urls[s.id] ?? "").trim() || s.defaultUrl;
    const n = status(s.id)?.models;
    const parts = [where ?? t(`ai.services.${s.id}.sub`)];
    if (n != null) parts.push(n === 1 ? t("ai.picker.oneModel") : t("ai.picker.models", { count: n }));
    else if (s.needsKey) parts.push(t(`ai.services.${s.id}.sub`));
    return parts.join(" · ");
  }

  const seconds = (ms: number) => Math.max(1, Math.round(ms / 1000)).toLocaleString(locale());
  const missing = $derived(list.filter((s) => s.installPage && status(s.id) && !status(s.id).found));
</script>

<div class="picker">
  <div class="left">
    <p class="muted intro">{t(`ai.picker.intro.${kind}`)}</p>
    <div class="options" role="radiogroup" aria-label={t(`ai.picker.intro.${kind}`)}>
      {#each list as s (s.id)}
        {@const b = badge(s)}
        {@const off = s.kind === "cli" && status(s.id) && !status(s.id).found}
        <button class="option" class:on={selected === s.id} role="radio" aria-checked={selected === s.id} disabled={!!off} onclick={() => select(s.id)}>
          <span class="radio" aria-hidden="true"></span>
          <span class="icon"><Icon name={s.icon} size={18} /></span>
          <span class="text">
            <b>{t(`ai.services.${s.id}.name`)}</b>
            <span class="muted">{subtitle(s)}</span>
          </span>
          {#if b}<span class="badge {b.tone}">{#if b.tone === "ok"}<Icon name="check" size={12} />{/if}{b.text}</span>{/if}
        </button>
      {/each}
    </div>
    <div class="links">
      {#each missing as s (s.id)}
        <button class="link" onclick={() => aiOpenPage(s.installPage ?? "")}>{t("ai.picker.howToInstall", { name: t(`ai.services.${s.id}.name`) })}<Icon name="openExternal" size={12} /></button>
        <span class="muted">·</span>
      {/each}
      <button class="link" disabled={checking} onclick={refresh}>{checking ? t("ai.picker.checking") : t("ai.picker.checkAgain")}</button>
    </div>
  </div>

  <div class="right">
    {#if info}
      {#if info.url}
        <label class="field">
          <span class="field-label">{t("ai.picker.address")}</span>
          <input type="text" spellcheck="false" bind:value={urlInput} placeholder={info.defaultUrl ?? t(`ai.picker.addressHint.${kind}`)} onchange={saveUrl} />
          <span class="muted small">{t(`ai.picker.addressHint.${kind}`)}</span>
        </label>
      {/if}

      {#if info.needsKey || info.optionalKey}
        <div class="field">
          <span class="field-label">{t(info.id === "unsloth" ? "ai.picker.unslothKey" : "ai.picker.key")}{info.optionalKey ? ` (${t("ai.picker.optional")})` : ""}</span>
          {#if hasKey}
            <div class="saved">
              <span class="ok-text"><Icon name="check" size={14} />{t("ai.picker.keyStored")}</span>
              <button class="link" onclick={removeKey}>{t("ai.picker.removeKey")}</button>
            </div>
          {:else}
            <div class="key-row">
              <input type="password" spellcheck="false" autocomplete="off" bind:value={keyInput} placeholder={t(`ai.services.${info.id}.keyHint`)} onkeydown={(e) => e.key === "Enter" && keyInput.trim() && saveKey()} />
              <button class="btn btn-ghost" disabled={!keyInput.trim()} onclick={saveKey}>{t("ai.picker.saveKey")}</button>
            </div>
            {#if keyError}<span class="bad-text small">{keyError}</span>{/if}
          {/if}
          {#if info.keyPage}
            <button class="link small" onclick={() => aiOpenPage(info.keyPage ?? "")}>{t("ai.picker.getKey")}<Icon name="openExternal" size={12} /></button>
          {/if}
        </div>
      {/if}

      {#if status(info.id)?.canStart}
        <label class="auto">
          <input type="checkbox" checked={settings.ai.autoStart} onchange={(e) => setAutoStart(e.currentTarget.checked)} />
          <span>{t("ai.picker.autoStart")}</span>
        </label>
      {/if}

      <div class="field">
        <span class="field-label">{t("ai.picker.model")}</span>
        {#if loadingModels}
          <span class="muted small">{ai.starting !== null && info.id === "unsloth" ? t("ai.picker.starting", { seconds: ai.starting }) : t("ai.picker.loadingModels")}</span>
        {:else if needsKeyFirst}
          <span class="muted small">{t("ai.picker.keyFirst")}</span>
        {:else if needsUrlFirst}
          <span class="muted small">{t("ai.picker.addressFirst")}</span>
        {:else if models.length}
          <select value={settings.ai.model} onchange={(e) => pickModel(e.currentTarget.value)}>
            {#each models as m (m.id)}
              <option value={m.id}>{m.label}{m.id === recommendedModel(info.id) ? ` (${t("ai.picker.recommended")})` : ""}</option>
            {/each}
          </select>
        {:else}
          <input type="text" spellcheck="false" value={settings.ai.model} placeholder={t("ai.picker.modelName")}
            onchange={(e) => pickModel(e.currentTarget.value.trim())} />
          <span class="muted small">{modelsError ?? t("ai.picker.noList")}</span>
        {/if}
      </div>

      <div class="test">
        <button class="btn btn-ghost" disabled={!settings.ai.model || test?.state === "running"} onclick={runTest}>
          {test?.state === "running" ? (ai.starting !== null && info.id === "unsloth" ? t("ai.picker.starting", { seconds: ai.starting }) : t("ai.picker.testing")) : t("ai.picker.test")}
        </button>
        {#if test?.state === "ok"}
          <span class="ok-text" role="status"><Icon name="check" size={14} />{t("ai.picker.works", { seconds: seconds(test.ms) })}</span>
        {:else if test?.state === "fail"}
          <span class="bad-text" role="status">{test.error}</span>
        {/if}
      </div>

      <ul class="notes">
        {#each kind === "api" ? ["apiCost"] : [] as note (note)}
          <li><Icon name={NOTE_ICONS[note]} />{t(`ai.picker.notes.${note}`)}</li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .picker { display: grid; grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr); gap: 22px; }
  .left { min-width: 0; }
  .right { border-left: 1px solid var(--border); padding-left: 22px; min-width: 0; display: flex; flex-direction: column; gap: 12px; }
  .intro { font-size: 12.5px; margin-bottom: 8px; }
  .options { display: flex; flex-direction: column; gap: 7px; }
  .option { display: flex; align-items: center; gap: 10px; padding: 9px 12px; border: 1px solid var(--border-strong); border-radius: var(--radius-sm); background: var(--surface); text-align: left; width: 100%; }
  .option.on { border: 2px solid var(--ai); padding: 8px 11px; background: var(--tip-bg); }
  .option:disabled { cursor: default; }
  .option:disabled .text, .option:disabled .icon { opacity: 0.55; }
  .radio { width: 16px; height: 16px; border-radius: 50%; border: 2px solid var(--border-strong); flex: none; }
  .option.on .radio { border-color: var(--ai); box-shadow: inset 0 0 0 3px var(--surface); background: var(--ai); }
  .icon { display: flex; justify-content: center; flex: none; width: 22px; color: var(--text-muted); }
  .text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .text b { font-size: 13px; }
  .text .muted { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .badge { display: inline-flex; align-items: center; gap: 3px; font-size: 11px; font-weight: 600; padding: 2px 8px; border-radius: 10px; flex: none; }
  .badge.ok { background: var(--safe-bg); color: var(--safe-fg); }
  .badge.warn { background: var(--check-bg); color: var(--check-fg); }
  .badge.off { background: var(--track); color: var(--text-muted); }
  .links { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; font-size: 12.5px; }
  .link { border: none; background: none; color: var(--accent); padding: 0; font-size: 12.5px; text-align: left; display: inline-flex; align-items: center; gap: 3px; }
  .link:disabled { opacity: 0.6; cursor: default; }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field-label { font-size: 12.5px; font-weight: 600; }
  .field input, .field select { padding: 6px 8px; border-radius: var(--radius-sm); border: 1px solid var(--border-strong); background: var(--surface); font-size: 13px; width: 100%; user-select: text; }
  .key-row { display: flex; gap: 6px; }
  .key-row .btn { flex: none; padding: 6px 12px; }
  .saved { display: flex; gap: 10px; align-items: center; font-size: 12.5px; }
  .small { font-size: 11.5px; }
  .auto { display: flex; gap: 8px; align-items: flex-start; font-size: 12.5px; font-weight: 600; }
  .auto input { width: 16px; height: 16px; margin-top: 1px; accent-color: var(--ai); flex: none; }
  .test { display: flex; flex-direction: column; align-items: flex-start; gap: 6px; }
  .test .btn:disabled, .key-row .btn:disabled { opacity: 0.5; cursor: default; }
  .ok-text { display: inline-flex; align-items: center; gap: 4px; color: var(--safe-fg); font-size: 12.5px; }
  .bad-text { color: var(--keep-fg); font-size: 12.5px; }
  .notes li { display: flex; gap: 7px; align-items: flex-start; }
  .notes li :global(.icon) { color: var(--text-muted); margin-top: 0.15em; }
  .notes { list-style: none; display: flex; flex-direction: column; gap: 6px; font-size: 12.5px; line-height: 1.4; }
  @media (max-width: 760px) {
    .picker { grid-template-columns: 1fr; }
    .right { border-left: none; padding-left: 0; border-top: 1px solid var(--border); padding-top: 14px; }
  }
</style>
