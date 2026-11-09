<script lang="ts">
  import LeftoverRow from "./LeftoverRow.svelte";
  import StatusIcon from "./StatusIcon.svelte";
  import Icon from "./Icon.svelte";
  import {
    createRestorePoint,
    findAppLeftovers,
    isAppInstalled,
    quarantineLeftovers,
    rememberUninstalled,
    restoreQuarantine,
    uninstallApp,
  } from "../api";
  import { formatBytes, leftoverCount } from "../format";
  import { t } from "../i18n/index.svelte";
  import { quarantine, settings } from "../settings.svelte";
  import type { InstalledApp, LeftoverItem, MoveReport, RestorePointOutcome } from "../types";

  /**
   * Uninstalls `apps` with one click: each app is removed (quietly when Windows
   * allows it), its leftovers are found and, at the end, all of them go to
   * Quarantine together, the app's settings included. A summary
   * closes the dialog, with an undo for the cleanup. `onClose` gets the ids of the
   * apps removed.
   */
  let { apps, onClose }: { apps: InstalledApp[]; onClose: (removed: string[]) => void } = $props();

  type Failure = "cancelled" | "declined" | "stillThere";
  type Result = { app: InstalledApp; ok: true } | { app: InstalledApp; ok: false; reason: Failure | null; message?: string };

  type Step =
    | { kind: "confirm" }
    | { kind: "restorePoint" }
    | { kind: "restorePointFailed"; outcome: Exclude<RestorePointOutcome, "created"> }
    | { kind: "running" }
    | { kind: "waiting" }
    | { kind: "searching" }
    | { kind: "cleaning" }
    | { kind: "done"; report: MoveReport | null; undone: boolean; cleanupError: string | null };

  /** How often to look whether the app is gone while its uninstaller still runs. */
  const POLL_MS = 2000;

  let index = $state(0);
  let step = $state<Step>({ kind: "confirm" });
  let results = $state<Result[]>([]);
  /** Safe leftovers of the apps removed so far: they go to Quarantine at the end. */
  let toClean = $state<LeftoverItem[]>([]);
  let undoing = $state(false);
  let showItems = $state(false);
  let pollTimer: ReturnType<typeof setTimeout> | undefined;

  const app = $derived(apps[index]);
  const single = $derived(apps.length === 1);
  const anyStore = $derived(apps.some((a) => a.source === "store"));
  const allStore = $derived(apps.every((a) => a.source === "store"));
  /** Installed for everyone (registry key in HKLM): Windows asks for permission. */
  const anyForEveryone = $derived(apps.some((a) => a.id.startsWith("reg:hklm")));
  const totalSize = $derived(apps.reduce((s, a) => s + a.sizeBytes, 0));

  const removedApps = $derived(results.filter((r) => r.ok).map((r) => r.app));
  const failures = $derived(results.filter((r) => !r.ok));

  const stepLabel = $derived(
    step.kind === "confirm" || step.kind === "restorePointFailed"
      ? t("uninstall.stepConfirm")
      : step.kind === "done"
        ? t("uninstall.stepDone")
        : t("uninstall.stepWorking"),
  );

  function errorText(e: unknown): string {
    const code = String(e);
    const known = ["notInstalled", "noUninstaller", "itself", "stillInstalled", "declined"];
    return known.includes(code) ? t(`uninstall.errors.${code}`) : t("uninstall.errors.other", { error: code });
  }

  function failureText(r: Result): string {
    if (r.ok) return "";
    if (r.message) return r.message;
    return r.reason === "stillThere" ? t("uninstall.stillThere", { name: r.app.name }) : t(`uninstall.${r.reason}`);
  }

  async function start() {
    if (settings.restorePoint && !allStore) {
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
      else await finishApp({ app, ok: false, reason: outcome });
    } catch (e) {
      await finishApp({ app, ok: false, reason: null, message: errorText(e) });
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

  async function stopWaiting() {
    clearTimeout(pollTimer);
    await finishApp({ app, ok: false, reason: "stillThere" });
  }

  async function afterRemoved() {
    // Listed in Apps › Uninstalled, to reinstall it later. Best effort.
    await rememberUninstalled(app).catch(() => {});
    if (app.source !== "store") {
      step = { kind: "searching" };
      try {
        const items = await findAppLeftovers(app.id);
        // All of it goes, the app's settings too: Quarantine keeps them, and the summary can undo.
        toClean = [...toClean, ...items];
      } catch {
        // The app is gone; its leftovers can still be found later in Leftovers.
      }
    }
    await finishApp({ app, ok: true });
  }

  async function finishApp(result: Result) {
    results = [...results, result];
    if (index < apps.length - 1) {
      index += 1;
      await uninstall();
    } else {
      await cleanUp();
    }
  }

  async function cleanUp() {
    if (!toClean.length) {
      step = { kind: "done", report: null, undone: false, cleanupError: null };
      return;
    }
    step = { kind: "cleaning" };
    try {
      const report = await quarantineLeftovers(
        toClean.map((i) => i.id),
        toClean,
      );
      step = { kind: "done", report, undone: false, cleanupError: null };
    } catch (e) {
      step = { kind: "done", report: null, undone: false, cleanupError: errorText(e) };
    }
  }

  async function undo() {
    if (step.kind !== "done" || !step.report) return;
    undoing = true;
    try {
      await restoreQuarantine(step.report.entryIds);
      step = { ...step, undone: true };
    } catch (e) {
      step = { ...step, cleanupError: errorText(e) };
    } finally {
      undoing = false;
    }
  }

  function close() {
    clearTimeout(pollTimer);
    onClose(removedApps.map((a) => a.id));
  }

  const movedBytes = $derived(step.kind === "done" && step.report && !step.undone ? step.report.movedBytes : 0);
  /** Freed: the apps removed plus the leftovers moved (unless put back). */
  const freed = $derived(removedApps.reduce((s, a) => s + a.sizeBytes, 0) + movedBytes);

  const doneTitle = $derived.by(() => {
    if (single) {
      return removedApps.length ? t("uninstall.removedTitle", { name: apps[0].name }) : t("uninstall.notRemoved", { name: apps[0].name });
    }
    if (!removedApps.length) return t("uninstall.noneRemoved");
    return removedApps.length === apps.length
      ? t("uninstall.allRemoved", { count: apps.length })
      : t("uninstall.someRemoved", { n: removedApps.length, total: apps.length });
  });

  /** Whether the app's own settings (AppData\Roaming) were among the leftovers. */
  const withSettings = $derived(toClean.some((i) => i.safety !== "safe"));

  /** The leftovers actually moved, to show on request. */
  const movedItems = $derived(step.kind === "done" && step.report && !step.undone ? toClean : []);

  const busy = $derived(["restorePoint", "running", "searching", "cleaning"].includes(step.kind));

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && (step.kind === "confirm" || step.kind === "done")) close();
  }
</script>

<svelte:window onkeydown={onKey} />

{#snippet icon(a: InstalledApp, size: number)}
  {#if a.icon}
    <img class="ico" src={a.icon} alt="" width={size} height={size} style="width:{size}px;height:{size}px" />
  {:else}
    <div class="ico letter" style="background:{a.iconColor};width:{size}px;height:{size}px;font-size:{size * 0.42}px">{a.name[0]}</div>
  {/if}
{/snippet}

<div class="backdrop" role="presentation">
  <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="uninstall-title" aria-busy={busy}>
    <div class="top">
      <span class="step">{stepLabel}</span>
      {#if !single && (step.kind === "running" || step.kind === "waiting" || step.kind === "searching")}
        <span class="muted count">{t("uninstall.progress", { n: index + 1, total: apps.length })}</span>
      {/if}
    </div>

    {#if step.kind === "confirm"}
      <h2 id="uninstall-title">{single ? t("uninstall.confirmTitle", { name: app.name }) : t("uninstall.confirmMany", { count: apps.length })}</h2>
      {#if single}
        <div class="app">
          {@render icon(app, 52)}
          <div>
            <div class="app-size">{app.sizeBytes ? formatBytes(app.sizeBytes) : app.publisher}</div>
            {#if app.sizeBytes && app.publisher}<div class="muted">{app.publisher}</div>{/if}
          </div>
        </div>
      {:else}
        <div class="apps">
          {#each apps as a (a.id)}
            <div class="app-row">
              {@render icon(a, 26)}
              <span class="app-name">{a.name}</span>
              <span class="muted">{a.sizeBytes ? formatBytes(a.sizeBytes) : ""}</span>
            </div>
          {/each}
        </div>
        {#if totalSize}<p class="muted total">{t("uninstall.totalSize", { size: formatBytes(totalSize) })}</p>{/if}
      {/if}
      {#if settings.restorePoint && !allStore}
        <p class="note good"><Icon name="shieldCheck" />{t("uninstall.restorePointNote")}</p>
      {/if}
      {#if anyStore}<p class="note">{allStore ? t("uninstall.storeNote") : t("uninstall.storeAlso")}</p>{/if}
      {#if anyForEveryone}<p class="note"><Icon name="shield" />{t("uninstall.adminNote")}</p>{/if}
      <div class="grow"></div>
      <div class="buttons">
        <button class="btn btn-ghost" onclick={close}>{t("common.cancel")}</button>
        <button class="btn btn-danger" onclick={start}>{t("common.uninstall")}</button>
      </div>
    {:else if step.kind === "restorePoint"}
      <h2 id="uninstall-title">{t("uninstall.creatingRestorePoint")}</h2>
      <div class="bar" aria-hidden="true"><i></i></div>
      <p class="muted">{t("uninstall.restorePointAsk")}</p>
    {:else if step.kind === "restorePointFailed"}
      <h2 id="uninstall-title">{single ? t("uninstall.confirmTitle", { name: app.name }) : t("uninstall.confirmMany", { count: apps.length })}</h2>
      <p class="note warn" role="alert"><Icon name="warning" />{t(`uninstall.restorePointNot.${step.outcome}`)}</p>
      <div class="grow"></div>
      <div class="buttons">
        <button class="btn btn-ghost" onclick={close}>{t("common.cancel")}</button>
        <button class="btn btn-danger" onclick={uninstall}>{t("uninstall.continueWithout")}</button>
      </div>
    {:else if step.kind === "running" || step.kind === "waiting" || step.kind === "searching"}
      <div class="current">
        {@render icon(app, 44)}
        <h2 id="uninstall-title">
          {step.kind === "running"
            ? t("uninstall.running", { name: app.name })
            : step.kind === "waiting"
              ? t("uninstall.waiting", { name: app.name })
              : t("uninstall.searching", { name: app.name })}
        </h2>
      </div>
      <div class="bar" aria-hidden="true"><i></i></div>
      <p class="muted">
        {step.kind === "running"
          ? t("uninstall.runningNote", { name: app.name })
          : step.kind === "waiting"
            ? t("uninstall.waitingNote", { name: app.name })
            : t("uninstall.searchingNote")}
      </p>
      {#if step.kind === "waiting"}
        <div class="grow"></div>
        <div class="buttons"><button class="btn btn-ghost" onclick={stopWaiting}>{t("uninstall.stopWaiting")}</button></div>
      {/if}
    {:else if step.kind === "cleaning"}
      <h2 id="uninstall-title">{t("uninstall.cleaning", { count: leftoverCount(toClean.length) })}</h2>
      <div class="bar" aria-hidden="true"><i></i></div>
      <p class="muted">{t("uninstall.quarantineNote", { days: quarantine.keepDays })}</p>
    {:else if step.kind === "done"}
      <div class="done">
        <div class="check"><StatusIcon kind={removedApps.length ? "ok" : "warn"} size={56} /></div>
        <h2 id="uninstall-title">{doneTitle}</h2>
        {#if freed > 0}
          <div class="freed">+{formatBytes(freed)}</div>
          <div class="muted">{t("uninstall.freed")}</div>
        {/if}
      </div>

      {#if !single}
        <div class="results">
          {#each results as r (r.app.id)}
            <div class="result">
              <StatusIcon kind={r.ok ? "ok" : "warn"} size={18} />
              <span class="app-name">{r.app.name}</span>
              {#if r.ok}
                <span class="muted">{r.app.sizeBytes ? formatBytes(r.app.sizeBytes) : ""}</span>
              {:else}
                <span class="muted why">{failureText(r)}</span>
              {/if}
            </div>
          {/each}
        </div>
      {:else if failures.length}
        <p class="note">{failureText(failures[0])}</p>
      {/if}

      <div class="summary muted">
        {#if step.undone}
          <p>{t("uninstall.undone")}</p>
        {:else if step.report}
          <p>
            {t(withSettings ? "uninstall.movedNoteSettings" : "uninstall.movedNote", { count: leftoverCount(step.report.movedCount), days: quarantine.keepDays })}
            {#if movedItems.length}
              <button class="link" onclick={() => (showItems = !showItems)}>{showItems ? t("quarantine.hideItems") : t("quarantine.showItems")}</button>
            {/if}
          </p>
          {#if step.report.declinedCount}<p>{t("uninstall.declinedNote", { count: leftoverCount(step.report.declinedCount) })}</p>{/if}
          {#if step.report.skippedCount}<p>{t("uninstall.skippedNote", { count: leftoverCount(step.report.skippedCount) })}</p>{/if}
        {:else if removedApps.some((a) => a.source !== "store") && !step.cleanupError}
          <p>{t("uninstall.noLeftovers")}</p>
        {/if}
        {#if removedApps.length}<p>{t("uninstall.reinstallNote")}</p>{/if}
        {#if step.cleanupError}<p class="warn-text" role="alert"><Icon name="warning" />{step.cleanupError}</p>{/if}
      </div>
      {#if showItems && movedItems.length}
        <div class="list">
          {#each movedItems as item (item.id)}
            <LeftoverRow {item} />
          {/each}
        </div>
      {/if}

      <div class="grow"></div>
      <div class="buttons">
        {#if step.report && step.report.entryIds.length && !step.undone}
          <button class="btn btn-ghost" disabled={undoing} onclick={undo}>{#if !undoing}<Icon name="undo" />{/if}{undoing ? t("uninstall.undoing") : t("uninstall.undo")}</button>
        {/if}
        <button class="btn btn-primary wide" disabled={undoing} onclick={close}>{t("uninstall.done")}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.32); display: flex; align-items: center; justify-content: center; z-index: 20; padding: 24px; }
  .dialog { width: 520px; max-width: 100%; min-height: 320px; max-height: calc(100vh - 48px); padding: 26px 28px 22px; display: flex; flex-direction: column; box-shadow: 0 16px 48px rgba(0, 0, 0, 0.22); }
  .top { display: flex; justify-content: space-between; align-items: baseline; gap: 12px; }
  .step { font-size: 12px; font-weight: 700; letter-spacing: 0.6px; text-transform: uppercase; color: var(--accent); }
  .count { font-size: 12px; }
  h2 { font-size: 21px; font-weight: 600; margin: 8px 0 14px; line-height: 1.25; }
  .app { display: flex; align-items: center; gap: 14px; margin-bottom: 14px; }
  .ico { border-radius: 22%; flex: none; object-fit: contain; }
  .letter { display: flex; align-items: center; justify-content: center; color: #fff; font-weight: 700; }
  .app-size { font-size: 15px; }
  .apps { overflow-y: auto; max-height: 190px; margin-bottom: 6px; border-top: 1px solid var(--border); }
  .app-row, .result { display: flex; align-items: center; gap: 10px; padding: 7px 2px; border-bottom: 1px solid var(--border); font-size: 14px; }
  .app-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .total { font-size: 13px; margin-bottom: 10px; }
  .current { display: flex; align-items: center; gap: 14px; margin: 8px 0 6px; }
  .current h2 { margin: 0; }
  .note { display: flex; gap: 8px; align-items: flex-start; font-size: 13.5px; line-height: 1.45; padding: 12px 14px; border-radius: var(--radius-sm); background: var(--bg); margin-bottom: 10px; }
  .note.good { background: var(--safe-bg); color: var(--safe-fg); }
  .note.warn { background: var(--check-bg); color: var(--check-fg); }
  .warn-text { color: var(--check-fg); display: flex; gap: 6px; align-items: flex-start; }
  .note :global(.icon), .warn-text :global(.icon) { margin-top: 0.15em; }
  .bar { height: 6px; border-radius: 3px; background: var(--accent-soft); overflow: hidden; margin: 6px 0 14px; }
  .bar i { display: block; height: 100%; width: 35%; border-radius: 3px; background: var(--accent); animation: sweep 1.3s ease-in-out infinite; }
  @keyframes sweep { from { transform: translateX(-100%); } to { transform: translateX(300%); } }
  .results { margin: 14px 0 4px; overflow-y: auto; max-height: 170px; border-top: 1px solid var(--border); }
  .why { font-size: 12.5px; max-width: 55%; text-align: right; }
  .list { margin: 4px 0 8px; overflow-y: auto; min-height: 0; max-height: 200px; border-top: 1px solid var(--border); }
  .grow { flex: 1; min-height: 12px; }
  .buttons { display: flex; gap: 10px; justify-content: flex-end; }
  .buttons .btn { padding: 10px 18px; font-size: 14px; }
  .buttons .wide { min-width: 170px; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .done { text-align: center; padding-top: 8px; }
  .check { line-height: 0; margin-bottom: 10px; }
  .done h2 { margin-bottom: 6px; }
  .freed { font-size: 36px; font-weight: 700; color: var(--safe-fg); margin-top: 6px; }
  .summary { font-size: 13px; margin-top: 14px; line-height: 1.45; text-align: center; }
  .summary p + p { margin-top: 4px; }
  .link { background: none; border: none; padding: 0; margin-left: 4px; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; }
</style>
