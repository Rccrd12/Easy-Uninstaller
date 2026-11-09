<script lang="ts">
  import EyeOffIcon from "../components/EyeOffIcon.svelte";
  import Icon from "../components/Icon.svelte";
  import AiServicePicker from "../components/AiServicePicker.svelte";
  import { AI_PARALLEL_CHOICES, aiStats, createRestorePoint, INDEX_DOWNLOAD_BYTES, indexCheck, indexFindModels, KEEP_DAYS_CHOICES, listExclusions, REMINDER_THRESHOLDS } from "../api";
  import { cancelIndexDownload, changeIndex, indexErrorText, installIndex, localIndex, removeIndex, watchIndex } from "../localIndex.svelte";
  import { forgetAnswers, serviceInfo } from "../ai.svelte";
  import { formatBytes } from "../format";
  import { getLanguage, languages, setLanguage, t, type Language } from "../i18n/index.svelte";
  import { applyTextSize, quarantine, saveQuarantine, saveSettings, settings } from "../settings.svelte";
  import { reminders, saveReminders, showSampleNotice } from "../reminders.svelte";
  import type { Screen } from "../screens";
  import type { AiMode, AiStats, IndexCheck, IndexDepth, IndexQuality, IndexSource, OwnModel, ReminderSettings, RestorePointOutcome } from "../types";
  import { getTheme, setTheme, themes } from "../theme.svelte";
  import { check, install, update } from "../updates.svelte";
  // Only this field ends up in the bundle; the release workflow keeps it in sync with the tag.
  import { version } from "../../../package.json";

  let { onNavigate }: { onNavigate: (s: Screen) => void } = $props();

  let hiddenCount = $state<number | null>(null);
  $effect(() => {
    listExclusions()
      .then((l) => (hiddenCount = l.length))
      .catch(() => (hiddenCount = null));
  });

  const phase = $derived(update.phase);

  let restorePoint = $state<RestorePointOutcome | "creating" | null>(null);

  let saving = $state(false);
  let saveError = $state<string | null>(null);

  /** The chosen drive, also when it is not connected right now (it is still the choice). */
  const driveMissing = $derived(quarantine.drive !== null && !quarantine.drives.some((d) => d.mountPoint === quarantine.drive));

  async function changeQuarantine(change: { keepDays?: number; drive?: string | null }) {
    saving = true;
    saveError = null;
    try {
      await saveQuarantine({ keepDays: quarantine.keepDays, drive: quarantine.drive, ...change });
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }

  // ── Reminders ──
  let remindSaving = $state(false);
  let remindError = $state<string | null>(null);
  let sample = $state<"sending" | "sent" | { error: string } | null>(null);

  async function changeReminders(change: Partial<ReminderSettings>) {
    remindSaving = true;
    remindError = null;
    try {
      await saveReminders({ ...reminders, ...change });
    } catch (e) {
      remindError = String(e);
    } finally {
      remindSaving = false;
    }
  }

  async function sendSample() {
    sample = "sending";
    sample = await showSampleNotice().then(
      () => "sent" as const,
      (e) => ({ error: String(e) }),
    );
  }

  // ── AI help ──
  const modes: AiMode[] = ["off", "online", "local"];
  let aiTab = $state<"cli" | "api">(serviceInfo(settings.ai.service)?.kind === "api" ? "api" : "cli");
  let stats = $state<AiStats | null>(null);
  const loadStats = () =>
    aiStats()
      .then((s) => (stats = s))
      .catch(() => {});
  $effect(() => {
    loadStats();
  });

  function setMode(mode: AiMode) {
    settings.ai.mode = mode;
    saveSettings();
  }

  async function clearAnswers() {
    await forgetAnswers();
    loadStats();
  }

  const aiKind = $derived(serviceInfo(settings.ai.service)?.kind);

  // ── Search by content (the local index) ──
  watchIndex();
  const qualities: IndexQuality[] = ["light", "better", "best"];
  const depths: IndexDepth[] = ["quick", "partial", "full"];
  const ix = $derived(localIndex.status);
  const count = (n: number) => n.toLocaleString(getLanguage());
  const percent = (done: number, total: number) => (total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0);

  // One's own model: chosen here, tried, and only then used.
  const sources: IndexSource[] = ["download", "files", "server"];
  let tab = $state<IndexSource | null>(null);
  let draft = $state({ ownModel: "", ownVision: "", serverUrl: "" });
  let found = $state<OwnModel[] | null>(null);
  let checking = $state(false);
  let checked = $state<{ key: string; result: IndexCheck } | null>(null);
  const shownSource = $derived(tab ?? ix?.settings.source ?? "download");
  const draftKey = $derived(`${shownSource}|${draft.ownModel.trim()}|${draft.ownVision.trim()}|${draft.serverUrl.trim()}`);
  const savedKey = $derived(ix ? `${ix.settings.source}|${ix.settings.ownModel}|${ix.settings.ownVision}|${ix.settings.serverUrl}` : "");
  const result = $derived(checked?.key === draftKey ? checked.result : null);
  const usable = $derived(!!result && (result.pictures || result.error === "index.error.needEngine"));
  const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

  function pickSource(source: IndexSource) {
    if (!ix) return;
    tab = source;
    draft = { ownModel: ix.settings.ownModel, ownVision: ix.settings.ownVision, serverUrl: ix.settings.serverUrl || "http://127.0.0.1:8080" };
    if (source === "download") {
      if (ix.settings.source !== "download") changeIndex({ source });
    } else if (source === "files" && !found) {
      indexFindModels()
        .then((m) => {
          found = m;
          if (!draft.ownModel && m[0]) draft = { ...draft, ownModel: m[0].model, ownVision: m[0].vision ?? "" };
        })
        .catch(() => (found = []));
    }
  }

  async function tryModel() {
    if (!ix || checking) return;
    checking = true;
    const key = draftKey;
    try {
      const result = await indexCheck({ ...ix.settings, source: shownSource, ...trimmed() });
      checked = { key, result };
    } catch (e) {
      checked = { key, result: { text: false, pictures: false, dim: 0, error: String(e) } };
    } finally {
      checking = false;
    }
  }

  const trimmed = () => ({ ownModel: draft.ownModel.trim(), ownVision: draft.ownVision.trim(), serverUrl: draft.serverUrl.trim() });

  async function useModel() {
    await changeIndex({ source: shownSource, ...trimmed() });
    if (localIndex.status && !localIndex.status.ready && localIndex.status.downloadBytes > 0) installIndex();
  }

  async function makeRestorePoint() {
    restorePoint = "creating";
    restorePoint = await createRestorePoint().catch(() => "failed" as const);
  }
</script>

<h1>{t("settings.title")}</h1>

<div class="card list">
  <label class="row">
    <span>{t("settings.language")}</span>
    <select value={getLanguage()} onchange={(e) => setLanguage(e.currentTarget.value as Language)}>
      {#each languages as lang (lang.code)}<option value={lang.code}>{lang.name}</option>{/each}
    </select>
  </label>
  <div class="row theme">
    <span id="theme-label">
      {t("settings.theme")}
    </span>
    <div class="segmented" role="radiogroup" aria-labelledby="theme-label">
      {#each themes as theme (theme)}
        <button role="radio" aria-checked={getTheme() === theme} class:on={getTheme() === theme} onclick={() => setTheme(theme)}>
          {t(`settings.themes.${theme}`)}
        </button>
      {/each}
    </div>
  </div>
  <label class="row">
    <span>
      {t("settings.largeText")}
    </span>
    <input type="checkbox" bind:checked={settings.largeText} onchange={() => { saveSettings(); applyTextSize(); }} />
  </label>
  <label class="row">
    <span>
      {t("settings.technical")}
    </span>
    <input type="checkbox" bind:checked={settings.technical} onchange={saveSettings} />
  </label>
  <label class="row">
    <span>
      {t("settings.quarantineDays")}
    </span>
    <select value={quarantine.keepDays} disabled={saving} onchange={(e) => changeQuarantine({ keepDays: Number(e.currentTarget.value) })}>
      {#each KEEP_DAYS_CHOICES as days (days)}<option value={days}>{t("settings.days", { days })}</option>{/each}
    </select>
  </label>
  <div class="row column">
    <label class="inner">
      <span>
        {t("settings.quarantineDrive")}
        <span class="muted hint">{t("settings.quarantineDriveHint", { drive: quarantine.systemDrive })}</span>
      </span>
      <select value={quarantine.drive ?? ""} disabled={saving || (!quarantine.drives.length && !quarantine.drive)}
              onchange={(e) => changeQuarantine({ drive: e.currentTarget.value || null })}>
        <option value="">{t("settings.driveDefault", { drive: quarantine.systemDrive })}</option>
        {#each quarantine.drives as d (d.mountPoint)}
          <option value={d.mountPoint}>{t(d.label ? "settings.driveOption" : "settings.driveOptionNoLabel", { drive: d.mountPoint, label: d.label, free: formatBytes(d.freeBytes) })}</option>
        {/each}
        {#if driveMissing}<option value={quarantine.drive}>{t("settings.driveMissing", { drive: quarantine.drive ?? "" })}</option>{/if}
      </select>
    </label>
    {#if !quarantine.drives.length && !quarantine.drive}
      <span class="muted note">{t("settings.noOtherDrive")}</span>
    {/if}
    {#if saveError}<span class="note bad" role="alert">{t("settings.saveFailed", { error: saveError })}</span>{/if}
  </div>
  <button class="row hidden-row" onclick={() => onNavigate("hidden")}>
    <span class="hidden-icon"><EyeOffIcon size={18} /></span>
    <span class="grow">
      {t("settings.hidden")}
    </span>
    {#if hiddenCount}<span class="count">{hiddenCount}</span>{/if}
    <span class="manage">{t("settings.manage")}<Icon name="chevronRight" size={14} /></span>
  </button>
  <div class="row restore">
    <label class="restore-toggle">
      <span>
        {t("settings.restorePoint")}
      </span>
      <input type="checkbox" bind:checked={settings.restorePoint} onchange={saveSettings} />
    </label>
    <div class="restore-now">
      <button class="btn btn-ghost" disabled={restorePoint === "creating"} onclick={makeRestorePoint}>{t("settings.restorePointNow")}</button>
      {#if restorePoint}
        <span class="status" class:ok={restorePoint === "created"} class:bad={restorePoint !== "created" && restorePoint !== "creating"} role="status">
          {t(`settings.restorePointStatus.${restorePoint}`)}
        </span>
      {/if}
    </div>
  </div>
</div>

<h2 class="section">{t("reminders.title")}</h2>
<div class="card list">
  <label class="row">
    <span class="grow">
      {t("reminders.lowSpace")}
      <span class="muted hint">{t("reminders.lowSpaceHint", { drive: quarantine.systemDrive, percent: reminders.threshold })}</span>
    </span>
    <input type="checkbox" checked={reminders.lowSpace} disabled={remindSaving}
           onchange={(e) => changeReminders({ lowSpace: e.currentTarget.checked })} />
  </label>
  {#if reminders.lowSpace}
    <label class="row sub">
      <span>{t("reminders.threshold")}</span>
      <select value={reminders.threshold} disabled={remindSaving} onchange={(e) => changeReminders({ threshold: Number(e.currentTarget.value) })}>
        {#each REMINDER_THRESHOLDS as percent (percent)}<option value={percent}>{t("reminders.thresholdOption", { percent })}</option>{/each}
      </select>
    </label>
  {/if}
  <label class="row">
    <span class="grow">
      {t("reminders.monthly")}
      <span class="muted hint">{t("reminders.monthlyHint")}</span>
    </span>
    <input type="checkbox" checked={reminders.monthly} disabled={remindSaving}
           onchange={(e) => changeReminders({ monthly: e.currentTarget.checked })} />
  </label>
  <div class="row column">
    <div class="how">
      <span class="grow"></span>
      <button class="btn btn-ghost" disabled={sample === "sending"} onclick={sendSample}>{t("reminders.sample")}</button>
    </div>
    {#if sample === "sent"}<span class="note ok" role="status">{t("reminders.sampleSent")}</span>
    {:else if sample && sample !== "sending"}<span class="note bad" role="alert">{t("reminders.sampleFailed", { error: sample.error })}</span>{/if}
    {#if remindError}<span class="note bad" role="alert">{t("reminders.saveFailed", { error: remindError })}</span>{/if}
  </div>
</div>

<h2 class="section">{t("ai.settings.title")} <span class="new">{t("ai.settings.new")}</span></h2>
<div class="card list ai">
  <div class="row theme">
    <span id="ai-mode-label">
      {t("ai.settings.mode")}
    </span>
    <div class="segmented" role="radiogroup" aria-labelledby="ai-mode-label">
      {#each modes as mode (mode)}
        <button role="radio" aria-checked={settings.ai.mode === mode} class:on={settings.ai.mode === mode} onclick={() => setMode(mode)}>
          {t(`ai.settings.modes.${mode}`)}
        </button>
      {/each}
    </div>
  </div>
  {#if settings.ai.mode !== "off"}
    <div class="row block">
      {#if settings.ai.mode === "online"}
        <div class="segmented tabs" role="tablist">
          <button role="tab" aria-selected={aiTab === "cli"} class:on={aiTab === "cli"} onclick={() => (aiTab = "cli")}><Icon name="ticket" />{t("ai.setup.tabCli")}</button>
          <button role="tab" aria-selected={aiTab === "api"} class:on={aiTab === "api"} onclick={() => (aiTab = "api")}><Icon name="key" />{t("ai.setup.tabApi")}</button>
        </div>
      {/if}
      {#key settings.ai.mode === "local" ? "local" : aiTab}
        <AiServicePicker kind={settings.ai.mode === "local" ? "local" : aiTab} />
      {/key}
    </div>
    <label class="row">
      <span>
        {t("ai.settings.confirm")}
        <span class="muted hint">{t("ai.settings.confirmHint")}</span>
      </span>
      <input type="checkbox" bind:checked={settings.ai.confirm} onchange={saveSettings} />
    </label>
    <label class="row">
      <span>
        {t("ai.settings.parallel")}
        <span class="muted hint">{t("ai.settings.parallelHint")}</span>
      </span>
      <select value={settings.ai.parallel} onchange={(e) => { settings.ai.parallel = Number(e.currentTarget.value); saveSettings(); }}>
        {#each AI_PARALLEL_CHOICES as n (n)}<option value={n}>{n}</option>{/each}
      </select>
    </label>
  {/if}
  <div class="row">
    <span>
      {t("ai.settings.month")}
      <span class="muted hint">
        {[
          t((stats?.askedThisMonth ?? 0) === 1 ? "ai.settings.monthOne" : "ai.settings.monthCount", { count: stats?.askedThisMonth ?? 0 }),
          ...(aiKind === "api" ? [t("ai.settings.costApi")] : []),
        ].join(" · ")}
      </span>
    </span>
  </div>
  <div class="row">
    <span>
      {t("ai.settings.saved")}
      <span class="muted hint">{t((stats?.answers ?? 0) === 1 ? "ai.settings.savedOne" : "ai.settings.savedCount", { count: stats?.answers ?? 0 })}</span>
    </span>
    <button class="btn btn-ghost" disabled={!stats?.answers} onclick={clearAnswers}>{t("ai.settings.clear")}</button>
  </div>
</div>


<h2 class="section">{t("index.title")} <span class="new">{t("ai.settings.new")}</span></h2>
{#if ix}
  {@const st = ix.settings}
  <div class="card list ai">
    <div class="row update" aria-live="polite">
      <span class="grow">
        {#if ix.download}
          <span class="version">{ix.download.verifying ? t("index.status.verifying") : t("index.status.downloading", { done: formatBytes(ix.download.doneBytes), total: formatBytes(ix.download.totalBytes) })}</span>
          <span class="bar" class:indeterminate={ix.download.verifying}><span style:width="{ix.download.verifying ? 30 : percent(ix.download.doneBytes, ix.download.totalBytes)}%"></span></span>
        {:else if !ix.ready && st.source !== "download"}
          <span class="version">{t("index.status.ownNone")}</span>
        {:else if !ix.ready}
          <span class="version">{t("index.status.none")}</span>
        {:else if ix.indexing}
          <span class="version">{t("index.status.indexing", { done: count(ix.indexed), left: count(ix.pending) })}</span>
          <span class="bar"><span style:width="{percent(ix.indexed, ix.indexed + ix.pending)}%"></span></span>
          {#if ix.current}<span class="muted hint current">{t("index.status.current", { name: ix.current })}</span>{/if}
        {:else}
          <span class="version">{t(st.paused ? "index.status.paused" : "index.status.ready", { count: count(ix.indexed) })}</span>
        {/if}
        {#if ix.error}<span class="hint bad" role="alert">{indexErrorText(ix.error)}</span>{/if}
      </span>
      {#if ix.download}
        <button class="btn btn-ghost" onclick={cancelIndexDownload}>{t("index.cancel")}</button>
      {:else if !ix.ready && ix.downloadBytes > 0}
        <button class="btn btn-primary" disabled={localIndex.busy} onclick={installIndex}><Icon name="download" />{t("index.download", { size: formatBytes(ix.downloadBytes) })}</button>
      {:else if st.paused}
        <button class="btn btn-ghost" onclick={() => changeIndex({ paused: false })}>{t("index.resume")}</button>
      {:else if ix.indexing}
        <button class="btn btn-ghost" onclick={() => changeIndex({ paused: true })}>{t("index.pause")}</button>
      {/if}
    </div>
    <div class="row theme">
      <span id="index-source">
        {t("index.source.label")}
      </span>
      <div class="segmented" role="radiogroup" aria-labelledby="index-source">
        {#each sources as src (src)}
          <button role="radio" aria-checked={shownSource === src} class:on={shownSource === src} disabled={!!ix.download} onclick={() => pickSource(src)}>{t(`index.source.${src}`)}</button>
        {/each}
      </div>
    </div>
    {#if shownSource === "download"}
      <div class="row theme">
        <span id="index-quality">
          {t("index.quality.label")}
          <span class="muted hint">{formatBytes(INDEX_DOWNLOAD_BYTES[st.quality])}</span>
        </span>
        <div class="segmented" role="radiogroup" aria-labelledby="index-quality">
          {#each qualities as q (q)}
            <button role="radio" aria-checked={st.quality === q} class:on={st.quality === q} disabled={!!ix.download} onclick={() => changeIndex({ quality: q })}>{t(`index.quality.${q}`)}</button>
          {/each}
        </div>
      </div>
    {:else}
      <div class="row own">
        {#if shownSource === "files"}
          {#if found === null && tab === "files"}
            <span class="muted hint">{t("index.own.searching")}</span>
          {:else if found && found.length === 0}
            <span class="muted hint">{t("index.own.noneFound")}</span>
          {:else if found}
            <span class="hint">{t("index.own.found")}</span>
            <div class="found" role="radiogroup" aria-label={t("index.own.found")}>
              {#each found as m (m.model)}
                <button role="radio" aria-checked={draft.ownModel === m.model} class:on={draft.ownModel === m.model} onclick={() => (draft = { ...draft, ownModel: m.model, ownVision: m.vision ?? "" })}>
                  <b>{fileName(m.model)}</b>
                  <span class="muted">{formatBytes(m.bytes)}{m.vision ? "" : ` · ${t("index.own.textOnly")}`}</span>
                </button>
              {/each}
            </div>
          {/if}
          <label class="path"><span>{t("index.own.model")}</span><input bind:value={draft.ownModel} placeholder="C:\…\model.gguf" spellcheck="false" /></label>
          <label class="path"><span>{t("index.own.vision")}</span><input bind:value={draft.ownVision} placeholder="C:\…\mmproj.gguf" spellcheck="false" /></label>
        {:else}
          <label class="path"><span>{t("index.own.url")}</span><input bind:value={draft.serverUrl} placeholder="http://127.0.0.1:8080" spellcheck="false" /></label>
        {/if}
        <div class="own-actions">
          <span class="grow">
            {#if checking}
              <span class="muted hint">{t("index.own.trying")}</span>
            {:else if result?.pictures}
              <span class="hint ok">{t("index.own.ok", { dim: result.dim })}</span>
            {:else if result?.error}
              <span class="hint bad" role="alert">{indexErrorText(result.error)}</span>
            {/if}
          </span>
          <button class="btn btn-ghost" disabled={checking} onclick={tryModel}>{t("index.own.try")}</button>
          {#if draftKey === savedKey}
            <span class="hint ok">{t("index.own.inUse")}</span>
          {:else}
            <button class="btn btn-primary" disabled={!usable || localIndex.busy} onclick={useModel}>{t("index.own.use")}</button>
          {/if}
        </div>
      </div>
    {/if}
    <div class="row theme">
      <span id="index-depth">
        {t("index.depth.label")}
      </span>
      <div class="segmented" role="radiogroup" aria-labelledby="index-depth">
        {#each depths as d (d)}
          <button role="radio" aria-checked={st.depth === d} class:on={st.depth === d} onclick={() => changeIndex({ depth: d })}>{t(`index.depth.${d}`)}</button>
        {/each}
      </div>
    </div>
    <div class="row theme">
      <span>
        {t("index.what.label")}
      </span>
      <div class="checks">
        <label><input type="checkbox" checked={st.photos} onchange={(e) => changeIndex({ photos: e.currentTarget.checked })} />{t("index.what.photos")}</label>
        <label><input type="checkbox" checked={st.videos} onchange={(e) => changeIndex({ videos: e.currentTarget.checked })} />{t("index.what.videos")}</label>
        <label><input type="checkbox" checked={st.documents} onchange={(e) => changeIndex({ documents: e.currentTarget.checked })} />{t("index.what.documents")}</label>
      </div>
    </div>
    <div class="row theme">
      <span id="index-where">
        {t("index.where.label")}
      </span>
      <div class="segmented" role="radiogroup" aria-labelledby="index-where">
        <button role="radio" aria-checked={!st.wholePc} class:on={!st.wholePc} onclick={() => changeIndex({ wholePc: false })}>{t("index.where.mine")}</button>
        <button role="radio" aria-checked={st.wholePc} class:on={st.wholePc} onclick={() => changeIndex({ wholePc: true })}>{t("index.where.all")}</button>
      </div>
    </div>
    {#if ix.diskBytes > 0}
      <div class="row">
        <span>
          {t("index.space")}
          <span class="muted hint">{formatBytes(ix.diskBytes)}</span>
        </span>
        <button class="btn btn-ghost" disabled={localIndex.busy} onclick={removeIndex}>{t("index.remove")}</button>
      </div>
    {/if}
  </div>
{/if}

<h2 class="section">{t("settings.updates")}</h2>
<div class="card list">
  <div class="row update" aria-live="polite">
    <span class="grow">
      <span class="version">{t("settings.version", { version })}</span>
      {#if phase.kind === "checking"}
        <span class="muted hint">{t("settings.updateChecking")}</span>
      {:else if phase.kind === "upToDate"}
        <span class="hint ok">{t("settings.updateUpToDate")}</span>
      {:else if phase.kind === "available"}
        <span class="hint available">{t("settings.updateAvailable", { version: phase.info.version })}</span>
      {:else if phase.kind === "downloading"}
        <span class="muted hint">
          {phase.percent === null ? t("settings.updateDownloadingNoSize") : t("settings.updateDownloading", { percent: phase.percent })}
        </span>
        <span class="bar" class:indeterminate={phase.percent === null}><span style:width="{phase.percent ?? 30}%"></span></span>
      {:else if phase.kind === "installing"}
        <span class="muted hint">{t("settings.updateInstalling", { version: phase.info.version })}</span>
        <span class="bar indeterminate"><span style:width="30%"></span></span>
      {:else if phase.kind === "failed"}
        <span class="hint bad" role="alert">
          {#if phase.offline}{t("settings.updateOffline")}
          {:else if phase.info}{t("settings.updateInstallFailed", { error: phase.error })}
          {:else}{t("settings.updateFailed", { error: phase.error })}{/if}
        </span>
      {/if}
    </span>
    {#if phase.kind === "available"}
      <button class="btn btn-primary" onclick={install}><Icon name="download" />{t("settings.updateNow")}</button>
    {:else if phase.kind === "failed" && phase.info}
      <button class="btn btn-primary" onclick={install}><Icon name="refresh" />{t("settings.updateRetry")}</button>
    {:else if phase.kind !== "downloading" && phase.kind !== "installing"}
      <button class="btn btn-ghost" disabled={phase.kind === "checking"} onclick={check}>
        <Icon name="refresh" />
        {phase.kind === "failed" ? t("settings.updateRetry") : t("settings.updateCheck")}
      </button>
    {/if}
  </div>
  <label class="row">
    <span>
      {t("settings.updateAuto")}
    </span>
    <input type="checkbox" bind:checked={settings.checkUpdates} onchange={saveSettings} />
  </label>
</div>

<style>
  .list { margin-top: 16px; }
  .row { display: flex; align-items: center; justify-content: space-between; padding: 14px 18px; border-bottom: 1px solid var(--border); }
  .row:last-child { border-bottom: none; }
  .row input { width: 18px; height: 18px; accent-color: var(--accent); flex: none; }
  .restore, .column { flex-direction: column; align-items: stretch; gap: 8px; }
  .inner { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .note { font-size: 12.5px; }
  .note.ok { color: var(--safe-fg); font-weight: 600; }
  .note.bad { color: var(--check-fg); font-weight: 600; }
  label.row { gap: 16px; }
  select { flex: none; max-width: 300px; }
  .restore-toggle { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .hint { display: block; font-size: 12px; margin-top: 2px; }
  .restore-now { display: flex; align-items: center; gap: 12px; }
  .restore-now .btn:disabled { opacity: 0.5; cursor: default; }
  .status { font-size: 12.5px; }
  .status.ok { color: var(--safe-fg); font-weight: 600; }
  .status.bad { color: var(--check-fg); font-weight: 600; }
  .theme { gap: 16px; }
  .segmented { display: inline-flex; flex: none; background: var(--track); border-radius: 8px; padding: 3px; }
  .segmented button { border: none; background: none; padding: 5px 14px; font-size: 12.5px; border-radius: 6px; }
  .segmented .on { background: var(--raised); font-weight: 600; box-shadow: var(--shadow); }
  select { font-family: inherit; padding: 4px 8px; border-radius: var(--radius-sm); border: 1px solid var(--border-strong); }
  .hidden-row { width: 100%; gap: 12px; border: none; border-bottom: 1px solid var(--border); background: none; text-align: left; font-size: inherit; color: inherit; }
  .hidden-row:hover { background: var(--hover); }
  .hidden-icon { display: flex; color: var(--text-muted); }
  .grow { flex: 1; min-width: 0; }
  .count { flex: none; font-size: 12px; font-weight: 600; background: var(--track); border-radius: 10px; padding: 1px 8px; }
  .manage { display: inline-flex; align-items: center; gap: 2px; flex: none; color: var(--accent); font-weight: 600; font-size: 13px; }
  .section { display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 600; color: var(--text-muted); letter-spacing: 0.3px; text-transform: uppercase; margin: 22px 0 0; }
  .new { font-size: 10px; font-weight: 700; padding: 2px 7px; border-radius: 8px; background: var(--ai); color: var(--on-ai); }
  .ai { border-color: var(--tip-border); }
  .block { flex-direction: column; align-items: stretch; gap: 10px; }
  .tabs { align-self: flex-start; }
  .row .btn:disabled { opacity: 0.5; cursor: default; }
  .sub { padding-left: 34px; background: var(--hover); }
  .how { display: flex; align-items: center; gap: 10px; font-size: 13px; }
  .how .btn { flex: none; }
  .how .btn:disabled { opacity: 0.5; cursor: default; }
  .update { gap: 14px; }
  .update .btn { flex: none; }
  .update .btn:disabled { opacity: 0.5; cursor: default; }
  .version { font-weight: 600; }
  .checks { display: flex; gap: 16px; flex: none; font-size: 13px; }
  .checks label { display: inline-flex; align-items: center; gap: 6px; }
  .checks input { width: 16px; height: 16px; accent-color: var(--accent); }
  .current { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row.own { flex-direction: column; align-items: stretch; gap: 8px; }
  .found { display: flex; flex-direction: column; gap: 4px; }
  .found button { display: flex; justify-content: space-between; gap: 12px; text-align: left; border: 1px solid var(--border); background: var(--surface); border-radius: var(--radius-sm); padding: 7px 10px; font-size: 12.5px; }
  .found button.on { border-color: var(--accent); background: var(--accent-soft); }
  .found b { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .path { display: flex; align-items: center; gap: 10px; font-size: 12.5px; }
  .path span { width: 160px; flex: none; }
  .row .path input { flex: 1; width: auto; height: auto; min-width: 0; padding: 6px 8px; border: 1px solid var(--border-strong); border-radius: var(--radius-sm); background: var(--surface); font-size: 12.5px; user-select: text; }
  .own-actions { display: flex; align-items: center; gap: 10px; }
  .segmented button:disabled { opacity: 0.5; cursor: default; }
  .hint.ok { color: var(--safe-fg); font-weight: 600; }
  .hint.bad { color: var(--check-fg); font-weight: 600; }
  .hint.available { color: var(--accent); font-weight: 600; }
  .bar { display: block; height: 6px; margin-top: 8px; border-radius: 3px; background: var(--track); overflow: hidden; max-width: 360px; }
  .bar > span { display: block; height: 100%; border-radius: 3px; background: var(--accent); transition: width 0.15s; }
  .bar.indeterminate > span { animation: slide 1.2s ease-in-out infinite; }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(340%); } }
</style>
