<script lang="ts">
  import Icon from "./Icon.svelte";
  import StatusIcon from "./StatusIcon.svelte";
  import { keepOnThisPc, makeOnlineOnly, moveBackFiles, moveToDrive, showMovedFile } from "../api";
  import { driveName, offload, refreshOffload } from "../offload.svelte";
  import { fileCount, formatBytes, formatDate } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import { folderOf, nameOf } from "../paths";
  import type { DriveMoveReport, MoveProgress, OffloadFile } from "../types";

  /** The side panel of one "without deleting" card: what happens, the files, and the button. */
  let { kind }: { kind: "onedrive" | "move" } = $props();

  /** How many files the panel lists before "Show all". */
  const SHOWN = 5;

  type Result =
    | { kind: "freed"; count: number; bytes: number; skipped: number }
    | { kind: "moved"; report: DriveMoveReport }
    | { kind: "info"; lines: string[] }
    | { kind: "error"; lines: string[] };

  /** Listed files the user unticked. Files not listed (past 200) always go. */
  let except = $state<Set<string>>(new Set());
  let showAll = $state(false);
  let busy = $state(false);
  let progress = $state<MoveProgress | null>(null);
  let result = $state<Result | null>(null);
  let pickedDrive = $state<string | null>(null);

  const od = $derived(offload.scan?.oneDrive);
  const mv = $derived(offload.scan?.moveFiles);
  const files = $derived((kind === "onedrive" ? od?.files : mv?.files) ?? []);
  const total = $derived((kind === "onedrive" ? od?.sizeBytes : mv?.sizeBytes) ?? 0);
  const count = $derived((kind === "onedrive" ? od?.fileCount : mv?.fileCount) ?? 0);
  const left = $derived(files.filter((f) => except.has(f.path)));
  const selectedBytes = $derived(total - left.reduce((n, f) => n + f.sizeBytes, 0));
  const selectedCount = $derived(count - left.length);
  const drive = $derived(mv?.drives.find((d) => d.mountPoint === pickedDrive) ?? mv?.drives[0]);
  const systemLetter = $derived((mv?.systemDrive ?? "C:").slice(0, 1).toUpperCase());
  const folderName = $derived(t("offload.move.folderName", { letter: systemLetter }));
  const n = (x: number) => x.toLocaleString(locale());

  function toggle(path: string) {
    const next = new Set(except);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    except = next;
  }

  /** "Videos\\Trip" for a file under the user's folder: where it is, in few words. */
  const where = (path: string) => folderOf(path).replace(/^[A-Z]:\\Users\\[^\\]+\\?/i, "") || folderOf(path);

  async function freeUp() {
    busy = true;
    result = null;
    try {
      const r = await makeOnlineOnly([...except]);
      result = r.doneCount
        ? { kind: "freed", count: r.doneCount, bytes: r.doneBytes, skipped: r.skippedCount }
        : { kind: "info", lines: [t("offload.onedrive.nothing")] };
      except = new Set();
      await refreshOffload();
    } catch (e) {
      result = { kind: "error", lines: [t("offload.onedrive.failed", { error: String(e) })] };
    } finally {
      busy = false;
    }
  }

  async function keepHere() {
    busy = true;
    try {
      await keepOnThisPc();
      result = { kind: "info", lines: [t("offload.onedrive.undone")] };
      await refreshOffload();
    } catch (e) {
      result = { kind: "error", lines: [t("offload.onedrive.failed", { error: String(e) })] };
    } finally {
      busy = false;
    }
  }

  async function move() {
    if (!drive) return;
    busy = true;
    result = null;
    progress = null;
    try {
      const r = await moveToDrive([...except], drive.mountPoint, folderName, (p) => (progress = p));
      result = r.movedCount ? { kind: "moved", report: r } : { kind: "info", lines: [t(r.noRoomCount ? "offload.move.nothingNoRoom" : "offload.move.nothing", { drive: drive.mountPoint })] };
      except = new Set();
      await refreshOffload();
    } catch (e) {
      result = { kind: "error", lines: [t("offload.move.failed", { error: String(e) })] };
    } finally {
      busy = false;
      progress = null;
    }
  }

  async function moveBack(ids: string[]) {
    busy = true;
    result = null;
    try {
      const r = await moveBackFiles(ids);
      const lines = [t("offload.move.movedBack", { count: fileCount(r.restoredCount), size: formatBytes(r.restoredBytes) })];
      if (r.renamedCount) lines.push(t("offload.move.renamed", { count: fileCount(r.renamedCount) }));
      if (r.failedCount) lines.push(t("offload.move.backFailed", { count: fileCount(r.failedCount) }));
      result = { kind: r.restoredCount ? "info" : "error", lines };
      await refreshOffload();
    } catch (e) {
      result = { kind: "error", lines: [t("offload.move.failed", { error: String(e) })] };
    } finally {
      busy = false;
    }
  }

  function movedLines(r: DriveMoveReport): string[] {
    const lines = [t("offload.move.done", { count: fileCount(r.movedCount), size: formatBytes(r.movedBytes), folder: r.folder })];
    if (r.skippedCount) lines.push(t("offload.move.skipped", { count: fileCount(r.skippedCount) }));
    if (r.noRoomCount) lines.push(t("offload.move.noRoom", { count: fileCount(r.noRoomCount), drive: drive?.mountPoint ?? "" }));
    return lines;
  }

  function freedLines(r: Extract<Result, { kind: "freed" }>): string[] {
    const lines = [t("offload.onedrive.done", { count: fileCount(r.count), size: formatBytes(r.bytes) })];
    if (r.skipped) lines.push(t("offload.onedrive.skipped", { count: fileCount(r.skipped) }));
    return lines;
  }
</script>

{#snippet fileList(list: OffloadFile[])}
  <div class="label list-label">{t("files.largest")}</div>
  <ul class="items" class:all={showAll}>
    {#each showAll ? list : list.slice(0, SHOWN) as f (f.path)}
      <li>
        <label title={f.path}>
          <input type="checkbox" checked={!except.has(f.path)} disabled={busy} onchange={() => toggle(f.path)} />
          <span class="item-text">
            <span class="item-name">{nameOf(f.path)}</span>
            <span class="muted item-note">{where(f.path)} · {t("files.lastUsed", { date: formatDate(f.lastUsed) })}</span>
          </span>
          <span class="item-size">{formatBytes(f.sizeBytes)}</span>
        </label>
      </li>
    {/each}
  </ul>
  {#if list.length > SHOWN}
    <button class="show-all" onclick={() => (showAll = !showAll)}>{showAll ? t("files.showLess") : t("files.showAll", { count: n(list.length) })}</button>
  {/if}
  {#if count > list.length && (showAll || list.length <= SHOWN)}
    <div class="muted more">{t("files.notListed", { count: n(count - list.length) })}</div>
  {/if}
{/snippet}

{#snippet resultBox(r: Result)}
  <div class="result" class:error={r.kind === "error"} role="status" aria-live="polite">
    <StatusIcon kind={r.kind === "error" ? "warn" : "ok"} />
    <div class="result-text">
      {#each r.kind === "freed" ? freedLines(r) : r.kind === "moved" ? movedLines(r.report) : r.lines as line, i (i)}<p>{line}</p>{/each}
      {#if r.kind === "freed"}
        <button class="link" disabled={busy} onclick={keepHere}><Icon name="undo" size={14} />{t("offload.onedrive.undo")}</button>
      {:else if r.kind === "moved" && r.report.ids.length}
        {@const ids = r.report.ids}
        <button class="link" disabled={busy} onclick={() => moveBack(ids)}><Icon name="undo" size={14} />{t("offload.move.undo")}</button>
      {/if}
    </div>
  </div>
{/snippet}

<div class="label">{t("explain.title")}</div>
{#if kind === "onedrive" && od}
  <h3><Icon name="cloud" size={20} />{t("offload.onedrive.title")}</h3>
  <p class="about">{t("offload.onedrive.about")}</p>
  <div class="how">
    <div class="how-row"><Icon name="checkCircle" /><span>{t("offload.onedrive.before")}</span><span class="muted when">{t("offload.before")}</span></div>
    <div class="how-row"><Icon name="cloud" /><span>{t("offload.onedrive.after")}</span><span class="muted when">{t("offload.after")}</span></div>
  </div>
  {#if od.freeingCount > 0}
    <p class="muted freeing icon-line"><Icon name="hourglass" />{t("offload.onedrive.freeing", { count: fileCount(od.freeingCount), size: formatBytes(od.freeingBytes) })}</p>
  {/if}
  {#if files.length}
    {@render fileList(files)}
  {/if}
  <p class="note icon-line"><Icon name="info" />{t("offload.onedrive.note")}</p>
  <div class="grow"></div>
  {#if result}{@render resultBox(result)}{/if}
  {#if count > 0}
    <button class="btn btn-primary wide" disabled={busy || selectedCount === 0} onclick={freeUp}>
      {busy ? t("offload.onedrive.working") : t("offload.onedrive.button", { size: formatBytes(selectedBytes) })}
    </button>
  {/if}
{:else if kind === "move" && mv && drive}
  <h3><Icon name="drive" size={20} />{t("offload.move.title")}</h3>
  <p class="about">{t("offload.move.about", { folder: folderName, drive: driveName(drive) })}</p>
  {#if mv.drives.length > 1}
    <label class="pick">
      <span class="muted">{t("offload.move.drive")}</span>
      <select disabled={busy} value={drive.mountPoint} onchange={(e) => (pickedDrive = e.currentTarget.value)}>
        {#each mv.drives as d (d.mountPoint)}
          <option value={d.mountPoint}>{driveName(d)} · {t("offload.move.freeOn", { size: formatBytes(d.freeBytes) })}</option>
        {/each}
      </select>
    </label>
  {/if}
  {#if files.length}
    <div class="how">
      <div class="how-row"><Icon name="desktop" /><span class="path">{systemLetter}:\Users\…\{where(files[0].path)}</span><span class="muted when">{t("offload.before")}</span></div>
      <div class="how-row"><Icon name="drive" /><span class="path">{drive.mountPoint}\{folderName}\{where(files[0].path)}</span><span class="muted when">{t("offload.after")}</span></div>
    </div>
    {@render fileList(files)}
  {/if}
  {#if mv.moved.length}
    <div class="label list-label moved-label">
      <span>{t("offload.move.movedTitle")}</span>
      {#if mv.moved.length > 1}
        {@const all = mv.moved.map((m) => m.id)}
        <button class="link" disabled={busy} onclick={() => moveBack(all)}>{t("offload.move.moveBackAll")}</button>
      {/if}
    </div>
    <ul class="items moved">
      {#each mv.moved as m (m.id)}
        <li>
          <span class="item-text" title={m.path}>
            <span class="item-name">{nameOf(m.path)}</span>
            <span class="muted item-note">{folderOf(m.path)}</span>
          </span>
          <span class="item-size">{formatBytes(m.sizeBytes)}</span>
          <button class="icon-btn" disabled={busy} title={t("offload.move.show")} aria-label={t("offload.move.showName", { name: nameOf(m.path) })} onclick={() => showMovedFile(m.id)}><Icon name="folderOpen" /></button>
          <button class="icon-btn" disabled={busy} title={t("offload.move.moveBack")} aria-label={t("offload.move.moveBackName", { name: nameOf(m.path) })} onclick={() => moveBack([m.id])}><Icon name="undo" /></button>
        </li>
      {/each}
    </ul>
  {/if}
  <div class="grow"></div>
  {#if progress}
    <div class="progress" role="status" aria-live="polite">
      <div class="bar"><i style="width:{progress.totalBytes ? (100 * progress.doneBytes) / progress.totalBytes : 0}%"></i></div>
      <p class="muted">{t("offload.move.moving", { n: progress.doneCount, total: progress.totalCount, done: formatBytes(progress.doneBytes), size: formatBytes(progress.totalBytes) })}</p>
    </div>
  {/if}
  {#if result}{@render resultBox(result)}{/if}
  {#if count > 0}
    <button class="btn btn-primary wide" disabled={busy || selectedCount === 0} onclick={move}>
      {busy && progress ? t("offload.move.working") : t("offload.move.button", { size: formatBytes(selectedBytes), drive: drive.mountPoint })}
    </button>
  {/if}
{/if}

<style>
  .label { font-size: 11px; font-weight: 600; letter-spacing: 0.3px; color: var(--text-muted); text-transform: uppercase; }
  h3 { font-size: 17px; margin: 6px 0 4px; display: flex; align-items: center; gap: 8px; }
  h3 :global(.icon) { color: var(--accent); }
  .about { font-size: 12.5px; line-height: 1.45; margin-top: 2px; }
  .how { background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 6px 10px; margin-top: 10px; font-size: 12.5px; }
  .how-row { display: flex; align-items: center; gap: 8px; padding: 4px 0; min-width: 0; }
  .how-row + .how-row { border-top: 1px solid var(--border); }
  .how-row :global(.icon) { color: var(--text-muted); }
  .how-row:last-child :global(.icon) { color: var(--accent); }
  .how-row > span:not(.when) { flex: 1; min-width: 0; }
  .path { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .when { font-size: 11.5px; flex: none; }
  .freeing { font-size: 12.5px; margin-top: 10px; }
  .pick { display: flex; align-items: center; gap: 8px; margin-top: 10px; font-size: 12.5px; }
  .pick select { flex: 1; min-width: 0; padding: 5px 6px; border: 1px solid var(--border-strong); border-radius: var(--radius-sm); font-size: 12.5px; }
  .list-label { margin-top: 14px; }
  .moved-label { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-top: 16px; }
  .moved-label .link { text-transform: none; letter-spacing: 0; font-size: 12px; }
  .items { list-style: none; margin-top: 6px; }
  .items li { border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 6px; min-width: 0; }
  .items label { display: flex; align-items: center; gap: 8px; padding: 6px 0; flex: 1; min-width: 0; cursor: pointer; }
  .items input { width: 15px; height: 15px; accent-color: var(--accent); flex: none; }
  .moved li { padding: 5px 0; }
  .item-text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .item-name, .item-note { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .item-name { font-size: 12.5px; font-weight: 600; }
  .item-note { font-size: 11px; }
  .item-size { font-size: 12px; font-weight: 600; flex: none; }
  .items.all { max-height: 40vh; overflow-y: auto; padding-right: 6px; }
  .show-all { border: none; background: none; color: var(--accent); font-size: 12.5px; padding: 6px 0 0; text-align: left; cursor: pointer; }
  .more { font-size: 12px; margin-top: 6px; }
  .note { font-size: 12.5px; line-height: 1.45; margin-top: 14px; padding: 9px 11px; border-radius: var(--radius-sm); background: var(--accent-soft); color: var(--text); }
  .note :global(.icon) { color: var(--accent); }
  .grow { flex: 1; }
  .icon-btn { flex: none; display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px; border: none; background: none; border-radius: var(--radius-sm); color: var(--text-muted); }
  .icon-btn:hover { background: var(--hover); color: var(--text); }
  .icon-btn:disabled, .link:disabled { opacity: 0.4; cursor: default; }
  .link { border: none; background: none; color: var(--accent); font-size: 12.5px; font-weight: 600; padding: 0; display: inline-flex; align-items: center; gap: 4px; }
  .result { display: flex; gap: 10px; align-items: flex-start; margin-top: 14px; padding: 10px 12px; border-radius: var(--radius-sm); border: 1px solid var(--border); border-left: 4px solid var(--safe-fg); font-size: 12.5px; line-height: 1.45; }
  .result.error { border-left-color: var(--danger); }
  .result-text { flex: 1; }
  .result-text p + p { color: var(--text-muted); margin-top: 2px; }
  .result-text .link { margin-top: 6px; }
  .progress { margin-top: 14px; font-size: 12.5px; }
  .bar { height: 8px; border-radius: 4px; background: var(--track); overflow: hidden; margin-bottom: 6px; }
  .bar i { display: block; height: 100%; background: var(--accent); transition: width 0.3s; }
  .wide { width: 100%; padding: 10px; margin-top: 14px; }
  .btn:disabled { opacity: 0.5; cursor: default; }
</style>
