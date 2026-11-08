<script lang="ts">
  import LeftoverRow from "./LeftoverRow.svelte";
  import {
    createRestorePoint,
    findAppLeftovers,
    isAppInstalled,
    QUARANTINE_DAYS,
    quarantineLeftovers,
    restoreQuarantine,
    uninstallApp,
  } from "../api";
  import { formatBytes, leftoverCount } from "../format";
  import { t } from "../i18n/index.svelte";
  import { settings } from "../settings.svelte";
  import type { InstalledApp, LeftoverItem, MoveReport, RestorePointOutcome } from "../types";

  /**
   * Uninstalls `apps` one after the other, following the three steps of the design:
   * confirm, leftovers found, done. `onClose` gets the ids of the apps removed.
   */
  let { apps, onClose }: { apps: InstalledApp[]; onClose: (removed: string[]) => void } = $props();

  type Step =
    | { kind: "confirm" }
    | { kind: "restorePoint" }
    | { kind: "restorePointFailed"; outcome: Exclude<RestorePointOutcome, "created"> }
    | { kind: "running" }
    | { kind: "waiting" }
    | { kind: "notRemoved"; reason: "cancelled" | "declined" | "stillThere" }
    | { kind: "searching" }
    | { kind: "leftovers"; items: LeftoverItem[] }
    | { kind: "cleaning"; items: LeftoverItem[] }
    | { kind: "done"; report: MoveReport | null; found: number; undone: boolean }
    | { kind: "error"; message: string };

  /** How often to look whether the app is gone while its uninstaller still runs. */
  const POLL_MS = 2000;

  let index = $state(0);
  let step = $state<Step>({ kind: "confirm" });
  let picked = $state<Set<string>>(new Set());
  let undoing = $state(false);
  const removed: string[] = [];
  let pollTimer: ReturnType<typeof setTimeout> | undefined;

  const app = $derived(apps[index]);
  const name = $derived(app.name);
  const isStore = $derived(app.source === "store");
  /** Installed for everyone (registry key in HKLM): Windows asks for permission. */
  const forEveryone = $derived(app.id.startsWith("reg:hklm"));
  const last = $derived(index === apps.length - 1);

  const pickedItems = $derived(step.kind === "leftovers" ? step.items.filter((i) => picked.has(i.id)) : []);
  const pickedBytes = $derived(pickedItems.reduce((s, i) => s + i.sizeBytes, 0));
  const pickedAdmin = $derived(pickedItems.filter((i) => i.needsAdmin).length);

  const stepLabel = $derived(
    step.kind === "leftovers" || step.kind === "cleaning" || step.kind === "searching"
      ? t("uninstall.stepLeftovers")
      : step.kind === "done"
        ? t("uninstall.stepDone")
        : t("uninstall.stepConfirm"),
  );

  function errorText(e: unknown): string {
    const code = String(e);
    const known = ["notInstalled", "noUninstaller", "itself", "stillInstalled", "declined"];
    return known.includes(code) ? t(`uninstall.errors.${code}`) : t("uninstall.errors.other", { error: code });
  }

  async function start() {
    if (settings.restorePoint && !isStore) {
      step = { kind: "restorePoint" };
      const outcome = await createRestorePoint().catch(() => "failed" as const);
      if (outcome !== "created") {
        step = { kind: "restorePointFailed", outcome };
        return;
      }
    }
    await uninstall();
  }

  async function uninstall() {
    step = { kind: "running" };
    try {
      const outcome = await uninstallApp(app.id);
      if (outcome === "removed") await afterRemoved();
      else if (outcome === "stillInstalled") waitForRemoval();
      else step = { kind: "notRemoved", reason: outcome };
    } catch (e) {
      step = { kind: "error", message: errorText(e) };
    }
  }

  /** Many uninstallers hand over to a copy of themselves and close at once: keep looking. */
  function waitForRemoval() {
    step = { kind: "waiting" };
    const check = async () => {
      if (step.kind !== "waiting") return;
      if (await isAppInstalled(app.id).catch(() => true)) {
        pollTimer = setTimeout(check, POLL_MS);
      } else {
        await afterRemoved();
      }
    };
    pollTimer = setTimeout(check, POLL_MS);
  }

  function stopWaiting() {
    clearTimeout(pollTimer);
    step = { kind: "notRemoved", reason: "stillThere" };
  }

  async function afterRemoved() {
    removed.push(app.id);
    if (isStore) {
      step = { kind: "done", report: null, found: 0, undone: false };
      return;
    }
    step = { kind: "searching" };
    try {
      const items = await findAppLeftovers(app.id);
      if (!items.length) {
        step = { kind: "done", report: null, found: 0, undone: false };
        return;
      }
      // Everything safe is ticked; the user's own settings are left for them to decide.
      picked = new Set(items.filter((i) => i.safety === "safe").map((i) => i.id));
      step = { kind: "leftovers", items };
    } catch (e) {
      step = { kind: "error", message: errorText(e) };
    }
  }

  function toggle(id: string) {
    const next = new Set(picked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    picked = next;
  }

  async function cleanUp() {
    if (step.kind !== "leftovers") return;
    const items = step.items;
    const ids = [...picked];
    step = { kind: "cleaning", items };
    try {
      const report = await quarantineLeftovers(ids, items);
      step = { kind: "done", report, found: items.length, undone: false };
    } catch (e) {
      step = { kind: "error", message: errorText(e) };
    }
  }

  function skip() {
    if (step.kind !== "leftovers") return;
    step = { kind: "done", report: null, found: step.items.length, undone: false };
  }

  async function undo() {
    if (step.kind !== "done" || !step.report) return;
    undoing = true;
    try {
      await restoreQuarantine(step.report.entryIds);
      step = { ...step, undone: true };
    } catch (e) {
      step = { kind: "error", message: errorText(e) };
    } finally {
      undoing = false;
    }
  }

  function next() {
    clearTimeout(pollTimer);
    if (last) {
      onClose(removed);
      return;
    }
    index += 1;
    picked = new Set();
    step = { kind: "confirm" };
  }

  function cancel() {
    clearTimeout(pollTimer);
    onClose(removed);
  }

  /** Freed by this app: its own size plus the leftovers moved (unless put back). */
  const freed = $derived(
    step.kind === "done" ? app.sizeBytes + (step.report && !step.undone ? step.report.movedBytes : 0) : 0,
  );

  const busy = $derived(["restorePoint", "running", "searching", "cleaning"].includes(step.kind));

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && (step.kind === "confirm" || step.kind === "error")) cancel();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="backdrop" role="presentation">
  <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="uninstall-title" aria-busy={busy}>
    <div class="top">
      <span class="step">{stepLabel}</span>
      {#if apps.length > 1}<span class="muted count">{t("uninstall.progress", { n: index + 1, total: apps.length })}</span>{/if}
    </div>

    {#if step.kind === "confirm"}
      <h2 id="uninstall-title">{t("uninstall.confirmTitle", { name })}</h2>
      <div class="app">
        {#if app.icon}
          <img class="ico" src={app.icon} alt="" width="44" height="44" />
        {:else}
          <div class="ico letter" style="background:{app.iconColor}">{name[0]}</div>
        {/if}
        <div>
          <div class="app-size">{app.sizeBytes ? formatBytes(app.sizeBytes) : app.publisher}</div>
          {#if app.sizeBytes && app.publisher}<div class="muted">{app.publisher}</div>{/if}
        </div>
      </div>
      {#if settings.restorePoint && !isStore}
        <p class="note good">🛡️ {t("uninstall.restorePointNote")}</p>
      {/if}
      <p class="note">{isStore ? t("uninstall.storeNote") : t("uninstall.uninstallerNote", { name })}</p>
      {#if forEveryone}<p class="note">🛡️ {t("uninstall.adminNote")}</p>{/if}
      <div class="grow"></div>
      <div class="buttons">
        <button class="btn btn-ghost" onclick={cancel}>{t("common.cancel")}</button>
        <button class="btn btn-danger" onclick={start}>{t("common.uninstall")}</button>
      </div>
    {:else if step.kind === "restorePoint"}
      <h2 id="uninstall-title">{t("uninstall.confirmTitle", { name })}</h2>
      <div class="working" role="status"><span class="spinner"></span>{t("uninstall.creatingRestorePoint")}</div>
      <p class="muted">{t("uninstall.restorePointAsk")}</p>
    {:else if step.kind === "restorePointFailed"}
      <h2 id="uninstall-title">{t("uninstall.confirmTitle", { name })}</h2>
      <p class="note warn" role="alert">⚠️ {t(`uninstall.restorePointNot.${step.outcome}`)}</p>
      <div class="grow"></div>
      <div class="buttons">
        <button class="btn btn-ghost" onclick={cancel}>{t("common.cancel")}</button>
        <button class="btn btn-danger" onclick={uninstall}>{t("uninstall.continueWithout")}</button>
      </div>
    {:else if step.kind === "running" || step.kind === "waiting"}
      <h2 id="uninstall-title">{step.kind === "running" ? t("uninstall.running", { name }) : t("uninstall.waiting", { name })}</h2>
      <div class="bar" aria-hidden="true"><i></i></div>
      <p class="muted">{step.kind === "running" ? t("uninstall.runningNote", { name }) : t("uninstall.waitingNote")}</p>
      {#if step.kind === "waiting"}
        <div class="grow"></div>
        <div class="buttons"><button class="btn btn-ghost" onclick={stopWaiting}>{t("uninstall.stopWaiting")}</button></div>
      {/if}
    {:else if step.kind === "notRemoved"}
      <h2 id="uninstall-title">{t("uninstall.notRemoved", { name })}</h2>
      <p class="note">{step.reason === "stillThere" ? t("uninstall.stillThere", { name }) : t(`uninstall.${step.reason}`)}</p>
      <div class="grow"></div>
      <div class="buttons"><button class="btn btn-primary" onclick={next}>{last ? t("common.close") : t("uninstall.next")}</button></div>
    {:else if step.kind === "searching"}
      <h2 id="uninstall-title">{t("uninstall.searching", { name })}</h2>
      <div class="bar" aria-hidden="true"><i></i></div>
    {:else if step.kind === "leftovers" || step.kind === "cleaning"}
      <h2 id="uninstall-title">{t("uninstall.foundTitle", { count: leftoverCount(step.items.length) })}</h2>
      <p class="muted">{t("uninstall.foundText")}</p>
      <div class="list">
        {#each step.items as item (item.id)}
          <LeftoverRow {item} checked={picked.has(item.id)} onToggle={() => toggle(item.id)} />
        {/each}
      </div>
      <p class="muted small">♻️ {t("uninstall.quarantineNote", { days: QUARANTINE_DAYS })}</p>
      {#if pickedAdmin}<p class="muted small">🛡️ {t("uninstall.adminLeftovers", { count: leftoverCount(pickedAdmin) })}</p>{/if}
      <div class="grow"></div>
      <div class="buttons">
        <button class="btn btn-ghost" disabled={step.kind === "cleaning"} onclick={skip}>{t("uninstall.skip")}</button>
        <button class="btn btn-primary wide" disabled={step.kind === "cleaning" || picked.size === 0} onclick={cleanUp}>
          {step.kind === "cleaning"
            ? t("uninstall.cleaning")
            : pickedBytes
              ? t("uninstall.cleanUp", { size: formatBytes(pickedBytes) })
              : t("uninstall.cleanUpNoSize")}
        </button>
      </div>
    {:else if step.kind === "done"}
      <div class="done">
        <div class="check" aria-hidden="true">✅</div>
        <h2 id="uninstall-title">{t("uninstall.removedTitle", { name })}</h2>
        {#if freed > 0}
          <div class="freed">+{formatBytes(freed)}</div>
          <div class="muted">{t("uninstall.freed")}</div>
        {/if}
        <div class="summary muted">
          {#if step.undone}
            <p>{t("uninstall.undone")}</p>
          {:else if step.report}
            <p>{t("uninstall.movedNote", { count: leftoverCount(step.report.movedCount), days: QUARANTINE_DAYS })}</p>
            {#if step.report.declinedCount}<p>{t("uninstall.declinedNote", { count: leftoverCount(step.report.declinedCount) })}</p>{/if}
            {#if step.report.skippedCount}<p>{t("uninstall.skippedNote", { count: leftoverCount(step.report.skippedCount) })}</p>{/if}
          {:else if step.found}
            <p>{t("uninstall.keptLeftovers")}</p>
          {:else if !isStore}
            <p>{t("uninstall.noLeftovers")}</p>
          {/if}
        </div>
      </div>
      <div class="grow"></div>
      <div class="buttons">
        {#if step.report && step.report.entryIds.length && !step.undone}
          <button class="btn btn-ghost" disabled={undoing} onclick={undo}>{undoing ? t("uninstall.undoing") : `↩ ${t("uninstall.undo")}`}</button>
        {/if}
        <button class="btn btn-primary wide" disabled={undoing} onclick={next}>{last ? t("uninstall.done") : t("uninstall.next")}</button>
      </div>
    {:else if step.kind === "error"}
      <h2 id="uninstall-title">{t("uninstall.notRemoved", { name })}</h2>
      <p class="note warn" role="alert">⚠️ {step.message}</p>
      <div class="grow"></div>
      <div class="buttons"><button class="btn btn-primary" onclick={next}>{last ? t("common.close") : t("uninstall.next")}</button></div>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.32); display: flex; align-items: center; justify-content: center; z-index: 20; padding: 24px; }
  .dialog { width: 520px; max-width: 100%; min-height: 400px; max-height: calc(100vh - 48px); padding: 26px 28px 22px; display: flex; flex-direction: column; box-shadow: 0 16px 48px rgba(0, 0, 0, 0.22); }
  .top { display: flex; justify-content: space-between; align-items: baseline; gap: 12px; }
  .step { font-size: 12px; font-weight: 700; letter-spacing: 0.6px; text-transform: uppercase; color: var(--accent); }
  .count { font-size: 12px; }
  h2 { font-size: 21px; font-weight: 600; margin: 8px 0 14px; line-height: 1.25; }
  .app { display: flex; align-items: center; gap: 14px; margin-bottom: 14px; }
  .ico { width: 52px; height: 52px; border-radius: 12px; flex: none; object-fit: contain; }
  .letter { display: flex; align-items: center; justify-content: center; color: #fff; font-size: 22px; font-weight: 700; }
  .app-size { font-size: 15px; }
  .note { font-size: 13.5px; line-height: 1.45; padding: 12px 14px; border-radius: var(--radius-sm); background: var(--bg); margin-bottom: 10px; }
  .note.good { background: var(--safe-bg); color: var(--safe-fg); }
  .note.warn { background: var(--check-bg); color: var(--check-fg); }
  .working { display: flex; align-items: center; gap: 10px; font-size: 14px; margin-bottom: 8px; }
  .bar { height: 6px; border-radius: 3px; background: var(--accent-soft); overflow: hidden; margin: 6px 0 14px; }
  .bar i { display: block; height: 100%; width: 35%; border-radius: 3px; background: var(--accent); animation: sweep 1.3s ease-in-out infinite; }
  @keyframes sweep { from { transform: translateX(-100%); } to { transform: translateX(300%); } }
  .spinner { display: inline-block; width: 16px; height: 16px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .list { margin: 10px 0 8px; overflow-y: auto; min-height: 0; border-top: 1px solid var(--border); }
  .small { font-size: 12px; margin-top: 4px; }
  .grow { flex: 1; min-height: 12px; }
  .buttons { display: flex; gap: 10px; justify-content: flex-end; }
  .buttons .btn { padding: 10px 18px; font-size: 14px; }
  .buttons .wide { min-width: 170px; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .done { text-align: center; padding-top: 8px; }
  .check { font-size: 54px; line-height: 1; margin-bottom: 6px; }
  .done h2 { margin-bottom: 6px; }
  .freed { font-size: 36px; font-weight: 700; color: var(--safe-fg); margin-top: 6px; }
  .summary { font-size: 13px; margin-top: 14px; line-height: 1.45; }
</style>
