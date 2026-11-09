<script lang="ts">
  import { untrack } from "svelte";
  import AppIcon from "./AppIcon.svelte";
  import Icon from "./Icon.svelte";
  import UninstallDialog from "./UninstallDialog.svelte";
  import { formatBytes } from "../format";
  import { t } from "../i18n/index.svelte";
  import type { InstalledApp } from "../types";

  /**
   * The "New PC" guided cleanup: the apps that came with Windows or the PC, the ones
   * usually not needed already selected, the others left to the user. Uninstalling
   * goes through the usual dialog. `apps` is the whole app list; `onRemoved` gets the
   * ids of the apps removed so the list can be updated.
   */
  let {
    apps,
    onBack,
    onShowUninstalled,
    onRemoved,
  }: {
    apps: InstalledApp[];
    onBack: () => void;
    onShowUninstalled: () => void;
    onRemoved: (ids: string[]) => void;
  } = $props();

  const bySize = (a: InstalledApp, b: InstalledApp) => b.sizeBytes - a.sizeBytes;
  const usuallyNot = $derived(apps.filter((a) => a.newPc?.advice === "remove").toSorted(bySize));
  const yourChoice = $derived(apps.filter((a) => a.newPc?.advice === "choose").toSorted(bySize));

  // The suggested apps start selected (once: later changes to the list do not reselect).
  let selected = $state<Set<string>>(new Set(untrack(() => apps.filter((a) => a.newPc?.advice === "remove").map((a) => a.id))));
  let uninstalling = $state<InstalledApp[] | null>(null);
  let removedCount = $state(0);

  const chosen = $derived([...usuallyNot, ...yourChoice].filter((a) => selected.has(a.id)));
  const chosenBytes = $derived(chosen.reduce((s, a) => s + a.sizeBytes, 0));
  const step = $derived(uninstalling ? 2 : removedCount ? 3 : 1);

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }

  function finished(removed: string[]) {
    const gone = (uninstalling ?? []).filter((a) => removed.includes(a.id));
    uninstalling = null;
    if (!gone.length) return;
    removedCount += gone.length;
    selected = new Set([...selected].filter((id) => !removed.includes(id)));
    onRemoved(removed);
  }

  const steps = ["newPc.stepChoose", "newPc.stepUninstall", "newPc.stepDone"];
</script>

{#snippet group(title: string, note: string, list: InstalledApp[])}
  {#if list.length}
    <div class="sec"><b>{title}</b>{#if note} <span class="muted">· {note}</span>{/if}</div>
    <div class="card list">
      {#each list as app (app.id)}
        <label class="row" class:sel={selected.has(app.id)}>
          <input type="checkbox" checked={selected.has(app.id)} onchange={() => toggle(app.id)} />
          <AppIcon {app} size={34} />
          <span class="info">
            <span class="name">{app.name}</span>
            <span class="muted why">{app.newPc ? t(`newPc.reasons.${app.newPc.reason}`) : ""}</span>
          </span>
          <span class="size">{app.sizeBytes ? formatBytes(app.sizeBytes) : "–"}</span>
        </label>
      {/each}
    </div>
  {/if}
{/snippet}

<button class="back" onclick={onBack}><Icon name="chevronLeft" />{t("apps.title")}</button>
<h1>{t("newPc.title")}</h1>

<ol class="steps" aria-label={t("newPc.title")}>
  {#each steps as key, i (key)}
    <li class:on={step === i + 1} class:past={step > i + 1} aria-current={step === i + 1 ? "step" : undefined}>
      <span class="dot">{#if step > i + 1}<Icon name="check" size={12} />{:else}{i + 1}{/if}</span>{t(key)}
    </li>
  {/each}
</ol>

{#if removedCount && !usuallyNot.length && !yourChoice.length}
  <div class="card done">
    <span class="tile"><Icon name="checkCircle" size={30} /></span>
    <div class="grow">
      <h2>{t("newPc.doneTitle")}</h2>
      <p class="muted">{t("uninstall.allRemoved", { count: removedCount })}. {t("newPc.doneText")}</p>
    </div>
  </div>
  <div class="actions">
    <button class="btn btn-ghost" onclick={onShowUninstalled}>{t("newPc.seeUninstalled")}</button>
    <button class="btn btn-primary" onclick={onBack}>{t("newPc.backToApps")}</button>
  </div>
{:else}
  {#if removedCount}
    <div class="card done small">
      <span class="tile"><Icon name="checkCircle" size={22} /></span>
      <div class="grow">
        <b>{t("uninstall.allRemoved", { count: removedCount })}</b>
        <div class="muted">{t("newPc.doneText")}</div>
      </div>
      <button class="btn btn-ghost" onclick={onShowUninstalled}>{t("newPc.seeUninstalled")}</button>
    </div>
  {:else}
    <p class="muted intro">{t("newPc.intro")}</p>
  {/if}


  {@render group(t("newPc.groupRemove"), t("newPc.groupRemoveNote"), usuallyNot)}
  {@render group(t("newPc.groupChoose"), "", yourChoice)}

  <div class="card footer">
    <b>{t("apps.selected", { count: chosen.length })}</b>
    {#if chosenBytes}<span class="muted">· {t("apps.willFree", { size: formatBytes(chosenBytes) })}</span>{/if}
    <span class="grow"></span>
    <button class="btn btn-danger" disabled={!chosen.length} onclick={() => (uninstalling = chosen)}>{t("newPc.uninstall", { count: chosen.length })}</button>
  </div>
{/if}

{#if uninstalling}
  <UninstallDialog apps={uninstalling} onClose={finished} />
{/if}

<style>
  .back { display: inline-flex; align-items: center; gap: 4px; background: none; border: none; color: var(--accent); font-size: 13px; padding: 2px 0; margin-bottom: 6px; }
  .steps { display: flex; gap: 22px; list-style: none; margin: 10px 0 14px; font-size: 13px; color: var(--text-muted); }
  .steps li { display: flex; align-items: center; gap: 7px; }
  .steps li.on { color: var(--text); font-weight: 600; }
  .dot { width: 20px; height: 20px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; font-size: 11.5px; font-weight: 600; border: 1px solid var(--border-strong); background: var(--surface); }
  .on .dot { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
  .past .dot { background: var(--accent-soft); border-color: var(--accent-soft); color: var(--accent); }
  .intro { max-width: 720px; line-height: 1.45; margin-bottom: 4px; }
  .sec { margin: 16px 0 6px; font-size: 13.5px; }
  .list { overflow: hidden; }
  .row { display: flex; align-items: center; gap: 12px; padding: 9px 14px; border-top: 1px solid var(--border); cursor: pointer; }
  .row:first-child { border-top: none; }
  .row:hover { background: var(--hover); }
  .row input { width: 18px; height: 18px; accent-color: var(--accent); flex: none; }
  .info { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .name, .why { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name { font-size: 14px; font-weight: 600; }
  .why { font-size: 12.5px; }
  .size { font-size: 13.5px; width: 80px; text-align: right; flex: none; }
  .footer { position: sticky; bottom: 0; display: flex; align-items: center; gap: 12px; padding: 12px 16px; margin-top: 12px; box-shadow: var(--shadow-up); }
  .grow { flex: 1; min-width: 0; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .done { display: flex; align-items: center; gap: 16px; padding: 22px 24px; margin-top: 6px; }
  .done.small { padding: 12px 16px; gap: 12px; margin: 4px 0 2px; }
  .done h2 { font-size: 18px; font-weight: 600; margin-bottom: 4px; }
  .done p { line-height: 1.45; }
  .tile { display: flex; color: var(--safe-fg); }
  .actions { display: flex; gap: 10px; justify-content: flex-end; margin-top: 14px; }
</style>
