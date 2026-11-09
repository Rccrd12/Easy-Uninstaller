<script lang="ts">
  import AiSetup from "../components/AiSetup.svelte";
  import ExplainCard from "../components/ExplainCard.svelte";
  import ExplainTag from "../components/ExplainTag.svelte";
  import EyeOffIcon from "../components/EyeOffIcon.svelte";
  import FileReview from "../components/FileReview.svelte";
  import StatusIcon from "../components/StatusIcon.svelte";
  import SafetyTag from "../components/SafetyTag.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import {
    addExclusions,
    DELETE_NOW_GROUPS,
    hiddenItem,
    keepFileGroups,
    listExclusions,
    listFileGroups,
    quarantineAppFolder,
    quarantineFileSelection,
    refreshFileGroups,
    removeExclusions,
    removeFileGroups,
    restoreQuarantine,
    usesSampleData,
  } from "../api";
  import { quarantine, settings } from "../settings.svelte";
  import { ai, aiErrorText, aiReady, answerFor, askAll, forgetAnswers, loadSavedAnswers, openSetup, serviceLabel, stopAskAll } from "../ai.svelte";
  import { fileCount, formatBytes, removedLines, restoreSummary } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import { displayPath, folderOf, nameOf } from "../paths";
  import type { Screen } from "../screens";
  import type { Exclusion, FileGroup, FileItem, FileSelection, MoveReport, ScanProgress } from "../types";

  let { onNavigate }: { onNavigate: (s: Screen) => void } = $props();

  /** How many items the details panel lists. */
  const SHOWN_ITEMS = 8;

  /** What the toast at the bottom says after a move or an undo. */
  type Notice =
    | { kind: "moved"; report: MoveReport }
    | { kind: "folderMoved"; report: MoveReport; item: FileItem }
    | { kind: "hidden"; items: Exclusion[]; lines: string[] }
    | { kind: "info"; lines: string[] }
    | { kind: "error"; lines: string[] };

  let groups = $state<FileGroup[]>([]);
  let checked = $state<Set<string>>(new Set());
  let focusedId = $state<string | null>(null);
  /** The item whose "What is this?" explanation the side panel shows. */
  let explained = $state<FileItem | null>(null);
  /** Whether the side panel lists every item of the group, not just the largest. */
  let showAll = $state(false);
  let scanning = $state(true);
  let progress = $state<ScanProgress | null>(null);
  let error = $state<string | null>(null);
  let moving = $state(false);
  let undoing = $state(false);
  let notice = $state<Notice | null>(null);
  /** The group being reviewed file by file, kept even when all its files are gone. */
  let reviewing = $state<FileGroup | null>(null);
  /** Changes after every move, undo or hidden file, so the review reads its files again. */
  let version = $state(0);
  /** How many files, folders and groups are hidden (leftovers have their own screen). */
  let hiddenCount = $state(0);
  /** The ⋯ menu of a group card. */
  let menu = $state<{ group: FileGroup; x: number; y: number } | null>(null);

  const focused = $derived(groups.find((g) => g.id === focusedId) ?? null);

  /** Groups of single files can be reviewed one by one; folder groups (temp) cannot. */
  const reviewable = (g: FileGroup) => g.items.length > 0 && g.items.every((i) => i.lastUsed !== null);

  function openReview(g: FileGroup) {
    reviewing = g;
    notice = null;
  }

  function load(rescan = false) {
    scanning = true;
    error = null;
    progress = null;
    listFileGroups({ rescan, onProgress: (p) => (progress = p) })
      .then((g) => {
        groups = g;
        // Pre-select only what is safe; anything else needs the user's eyes.
        checked = new Set(g.filter((x) => x.safety === "safe" && !x.reviewOnly).map((x) => x.id));
        if (!g.some((x) => x.id === focusedId)) focusedId = null;
        loadSavedAnswers(g.filter((x) => x.reviewOnly).flatMap((x) => unknownItems(x).map((i) => i.path)));
      })
      .catch((e) => (error = String(e)))
      .finally(() => (scanning = false));
  }

  $effect(() => load());
  $effect(() => countHidden());

  function countHidden() {
    listExclusions()
      .then((list) => (hiddenCount = list.filter((e) => e.kind !== "leftover").length))
      .catch(() => {});
  }

  function focus(id: string) {
    if (focusedId !== id) showAll = false;
    focusedId = id;
    explained = null;
  }

  const sections = $derived([
    { title: "files.safeSection", items: groups.filter((g) => g.safety === "safe") },
    { title: "files.checkSection", items: groups.filter((g) => g.safety === "check") },
  ]);
  const selectedBytes = $derived(groups.filter((g) => checked.has(g.id)).reduce((s, g) => s + g.sizeBytes, 0));
  const selectionDeletes = $derived([...checked].some((id) => DELETE_NOW_GROUPS.includes(id)));
  const selectionMoves = $derived([...checked].some((id) => !DELETE_NOW_GROUPS.includes(id)));
  /** What the button does: "Delete", "Move to quarantine", or both ("Free up"). */
  function actionLabel(deletes: boolean, moves: boolean, size: number): string {
    if (deletes && moves) return t("files.freeUp", { size: formatBytes(size) });
    return deletes ? t("files.deleteNow", { size: formatBytes(size) }) : `${t("common.moveToQuarantine")} (${formatBytes(size)})`;
  }

  function toggle(id: string) {
    const next = new Set(checked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    checked = next;
  }

  const count = (n: number) => n.toLocaleString(locale());

  /**
   * Removes whole groups: temporary files and caches are deleted, the rest goes to
   * Quarantine, and the toast offers to undo that part.
   */
  async function remove(ids: string[]) {
    if (!ids.length || moving) return;
    moving = true;
    notice = null;
    try {
      const report = await removeFileGroups(ids);
      await afterMove(report);
    } catch (e) {
      notice = { kind: "error", lines: [t("files.moveFailed", { error: String(e) })] };
    } finally {
      moving = false;
    }
  }

  /** Moves the files picked in the review. */
  async function moveSelection(selection: FileSelection) {
    if (moving) return;
    moving = true;
    notice = null;
    try {
      await afterMove(await quarantineFileSelection([selection]));
    } catch (e) {
      notice = { kind: "error", lines: [t("files.moveFailed", { error: String(e) })] };
    } finally {
      moving = false;
    }
  }

  async function afterMove(report: MoveReport) {
    if (report.movedCount === 0 && report.deletedCount === 0) {
      notice = { kind: "info", lines: [t("files.nothingMoved")] };
      return;
    }
    notice = { kind: "moved", report };
    await refresh();
  }

  /** Reads the groups again from the last scan: what was moved, put back or hidden shows up. */
  async function refresh() {
    try {
      setGroups(await refreshFileGroups());
    } finally {
      version++;
    }
  }

  /** The name a hidden item goes by in a sentence. */
  function hiddenName(e: Exclusion): string {
    if (e.kind === "group") return t(e.label ?? e.path);
    return e.kind === "folder" ? displayPath(e.path) : nameOf(e.path);
  }

  const itemCount = (n: number) => (n === 1 ? t("hidden.countOne") : t("hidden.count", { count: count(n) }));

  /** "Hide": nothing is moved or deleted, the files, folders or groups are only left out from now on. */
  async function hide(items: Exclusion[]) {
    if (!items.length) return;
    try {
      await addExclusions(items);
      const many = items.every((e) => e.kind === "file") ? fileCount(items.length) : itemCount(items.length);
      const line = items.length === 1 ? t("hidden.done", { name: hiddenName(items[0]) }) : t("hidden.doneMany", { count: many });
      notice = { kind: "hidden", items, lines: [line] };
      await refresh();
    } catch (e) {
      notice = { kind: "error", lines: [t("hidden.failed", { error: String(e) })] };
    } finally {
      countHidden();
    }
  }

  /** Undoes a hide: the items are listed again where they were. */
  async function unhide(items: Exclusion[]) {
    undoing = true;
    try {
      await removeExclusions(items);
      await refresh();
      notice = { kind: "info", lines: [t("hidden.shownAgain")] };
    } catch (e) {
      notice = { kind: "error", lines: [t("hidden.failed", { error: String(e) })] };
    } finally {
      undoing = false;
      countHidden();
    }
  }

  function openMenu(e: MouseEvent, group: FileGroup) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const width = 290;
    menu = { group, x: Math.max(8, Math.min(r.right - width, window.innerWidth - width - 8)), y: r.bottom + 4 };
  }

  /** Runs a menu action with the menu's group, then closes the menu. */
  function act(action: (g: FileGroup) => unknown) {
    const g = menu?.group;
    menu = null;
    if (g) action(g);
  }

  /** Folder items (temp) hide the whole folder; the others one file. */
  const hideDetail = (item: FileItem) => hide([hiddenItem(item.lastUsed === null ? "folder" : "file", item.path)]);

  async function undo(report: MoveReport) {
    undoing = true;
    try {
      const restored = await restoreQuarantine(report.entryIds);
      await refresh();
      const clean = restored.renamedCount === 0 && restored.failedCount === 0;
      notice = { kind: "info", lines: clean ? [t("files.undone")] : restoreSummary(restored) };
    } catch (e) {
      notice = { kind: "error", lines: [t("quarantine.failed", { error: String(e) })] };
    } finally {
      undoing = false;
    }
  }

  /** Folders of a review-only group the app has no explanation for: those the AI can be asked about. */
  const unknownItems = (g: FileGroup) => (g.reviewOnly ? g.items.filter((i) => i.explainId === null) : []);

  /** "Ask AI for all": shown first when the user wants to see what is sent. */
  let confirmAll = $state(false);
  let allFailed = $state<string | null>(null);

  function startAskAll(g: FileGroup) {
    allFailed = null;
    if (!aiReady()) {
      openSetup(() => startAskAll(g));
      return;
    }
    if (settings.ai.confirm && !confirmAll) {
      confirmAll = true;
      return;
    }
    confirmAll = false;
    askAll(unknownItems(g))
      .then((failed) => {
        if (failed) allFailed = t("ai.all.failed", { count: failed });
      })
      .catch((e) => (allFailed = aiErrorText(e)));
  }

  /** Moves one folder of a review-only group (unknown folders) into quarantine. */
  async function moveFolder(group: FileGroup, item: FileItem) {
    if (moving) return;
    moving = true;
    notice = null;
    const name = nameOf(item.path);
    try {
      const report = await quarantineAppFolder(item);
      if (report.movedCount === 0) {
        const key = report.declinedCount ? "files.folderDeclined" : "files.folderSkipped";
        notice = { kind: "info", lines: [t(key, { name })] };
        return;
      }
      explained = null;
      notice = { kind: "folderMoved", report, item };
      await refresh();
    } catch (e) {
      notice = { kind: "error", lines: [String(e) === "declined" ? t("files.folderDeclined", { name }) : t("files.moveFailed", { error: String(e) })] };
    } finally {
      moving = false;
    }
  }

  /** Updates the list here and the scan result kept for the other screens. */
  function setGroups(next: FileGroup[]) {
    groups = next;
    keepFileGroups(next);
    if (reviewing) reviewing = next.find((g) => g.id === reviewing?.id) ?? reviewing;
    checked = new Set([...checked].filter((id) => next.some((g) => g.id === id)));
    if (!next.some((g) => g.id === focusedId)) focusedId = null;
    if (!focusedId) explained = null;
  }

  function folderMovedLines(r: MoveReport, item: FileItem): string[] {
    return [t("files.folderMoved", { name: nameOf(item.path), size: formatBytes(r.movedBytes), days: quarantine.keepDays })];
  }
  /** The last two parts of a path, which say the most in little room: "…\\Pictures\\photo.jpg". */
  const shortPath = (path: string) => {
    const parts = path.split(/[\\/]/);
    return parts.length > 2 ? `…\\${parts.slice(-2).join("\\")}` : path;
  };
  const dateOf = (iso: string) => new Date(`${iso}T00:00:00`).toLocaleDateString(locale(), { year: "numeric", month: "short", day: "numeric" });

  function itemNote(item: FileItem): string {
    if (item.duplicateOf) return t("files.copyOf", { path: shortPath(item.duplicateOf) });
    if (item.originalPath) return t("files.wasIn", { path: folderOf(item.originalPath) });
    // "C:\\Users\\you\\AppData\\Local" says less than "AppData\\Local" in a narrow panel.
    const folder = folderOf(item.path).replace(/^.*?[\\/](AppData[\\/])/i, "$1");
    // What the AI said it is, beside where it is: the tag only says whether it can go.
    const label = item.explainId === null ? answerFor(item.path)?.label : undefined;
    if (label) return `${label} · ${folder}`;
    return item.lastUsed ? `${folder} · ${t("files.lastUsed", { date: dateOf(item.lastUsed) })}` : folder;
  }

  /** Groups listed by folder or Recycle Bin entry (no dates) rather than file by file. */
  const byFolder = (g: FileGroup) => g.items.every((i) => i.lastUsed === null);
  /** Items the scan found but did not send (it sends the largest 200). */
  const notListed = (g: FileGroup) => (byFolder(g) ? 0 : Math.max(g.itemCount - g.items.length, 0));
  /** Unknown folders count folders; Recycle Bin entries count deleted items; the rest count files. */
  function countLabel(g: FileGroup): string {
    if (g.reviewOnly) return g.itemCount === 1 ? t("files.folderCountOne") : t("files.folderCount", { count: count(g.itemCount) });
    if (g.id === "recycle-bin") return g.items.length === 1 ? t("files.entryCountOne") : t("files.entryCount", { count: count(g.items.length) });
    return fileCount(g.itemCount);
  }
  /** Folders get a "What is this?" tag and open their explanation. */
  const explainable = (g: FileGroup, item: FileItem) => g.reviewOnly || (item.explainId !== null && byFolder(g) && !item.originalPath);
</script>

{#if reviewing}
  {@const g = reviewing}
  <FileReview group={g} {version} busy={moving || undoing} onBack={() => (reviewing = null)} onMove={moveSelection} onHide={hide} />
{:else}
<div class="layout">
  <div class="col">
    <div class="head">
      <div>
        <h1>{t("files.title")}</h1>
        <p class="muted sub">{t("files.subtitle")}</p>
      </div>
      {#if !scanning}
        <button class="btn btn-ghost" onclick={() => load(true)}>↻ {t("files.rescan")}</button>
      {/if}
    </div>
    {#if usesSampleData()}<SampleBanner />{/if}

    {#if scanning}
      <div class="card scan" role="status" aria-live="polite">
        <div class="scan-title">{t("files.scanning")}</div>
        <div class="steps" aria-hidden="true">
          {#each Array(progress?.stepCount ?? 4) as _, i (i)}
            <i class:done={progress && i + 1 < progress.stepIndex} class:current={(progress?.stepIndex ?? 1) === i + 1}></i>
          {/each}
        </div>
        {#if progress}
          <div class="scan-row">
            <b>{t("files.stepOf", { n: progress.stepIndex, total: progress.stepCount })} · {t(`files.steps.${progress.step}`)}</b>
            <span class="muted">{t("files.checked", { count: count(progress.filesChecked) })}</span>
          </div>
        {/if}
        <p class="muted note">🛡️ {t("files.scanNote")}</p>
      </div>
    {:else if error}
      <div class="card message">
        <p>{t("files.failed", { error })}</p>
        <button class="btn btn-primary" onclick={() => load(true)}>{t("files.retry")}</button>
      </div>
    {:else if groups.length === 0}
      <div class="card message"><span class="big-icon" aria-hidden="true">✨</span><p>{t("files.empty")}</p></div>
    {:else}
      {#each sections as section (section.title)}
        {#if section.items.length}
          <h2>{t(section.title)}</h2>
          {#each section.items as group (group.id)}
            <div class="card group" class:focused={focusedId === group.id} role="button" tabindex="0"
                 onclick={() => focus(group.id)} onkeydown={(e) => e.key === "Enter" && focus(group.id)}>
              {#if group.reviewOnly}
                <span class="no-check" aria-hidden="true">🔎</span>
              {:else}
                <input type="checkbox" checked={checked.has(group.id)} onclick={(e) => e.stopPropagation()} onchange={() => toggle(group.id)} aria-label={t(group.titleKey)} />
              {/if}
              <div class="info">
                <div class="title">{t(group.titleKey)}</div>
                <div class="muted desc">{t(group.descriptionKey)} · {countLabel(group)}</div>
              </div>
              {#if reviewable(group)}
                <button class="review-link" onclick={(e) => { e.stopPropagation(); openReview(group); }}>{t("files.review.start")} ›</button>
              {/if}
              <SafetyTag safety={group.safety} />
              <span class="size">{formatBytes(group.sizeBytes)}</span>
              <button class="dots" aria-label={t("files.groupMenu", { name: t(group.titleKey) })} aria-haspopup="menu"
                      onclick={(e) => openMenu(e, group)}>⋯</button>
            </div>
          {/each}
        {/if}
      {/each}

      <div class="footer">
        <b>{t("files.selected", { size: formatBytes(selectedBytes) })}</b>
        <span class="grow"></span>
        <button class="btn btn-primary" disabled={selectedBytes === 0 || moving} onclick={() => remove([...checked])}>
          {moving ? t("files.moving") : actionLabel(selectionDeletes, selectionMoves, selectedBytes)}
        </button>
      </div>
      {#if selectionDeletes}
        <p class="muted how">{t("files.howRemoved", { days: quarantine.keepDays })}</p>
      {/if}
    {/if}

    {#if !scanning && hiddenCount > 0}
      <button class="hidden-link" onclick={() => onNavigate("hidden")}>
        <EyeOffIcon />
        <span>{t("files.hiddenLink", { count: itemCount(hiddenCount) })}</span>
        <span class="link-action">{t("files.hiddenLinkAction")} ›</span>
      </button>
    {/if}
  </div>

  <aside class="card side">
    {#if focused && explained}
      {@const group = focused}
      {@const item = explained}
      {#key item.path}
        <ExplainCard {item} groupTitle={t(group.titleKey)} onBack={() => (explained = null)}
          onMove={group.reviewOnly ? () => moveFolder(group, item) : undefined} {moving} />
      {/key}
    {:else if focused}
      <div class="label">{t("explain.title")}</div>
      <h3>{t(focused.titleKey)}</h3>
      <p class="muted desc">{t(focused.descriptionKey)}</p>
      <h4>{t("explain.what")}</h4>
      <p class="about">{t(focused.titleKey.replace(/title$/, "what"))}</p>
      <h4>{t("explain.ifRemovedGroup")}</h4>
      <p class="about">{t(focused.titleKey.replace(/title$/, "ifRemoved"))}</p>
      <dl>
        <dt>{t("files.size")}</dt><dd>{formatBytes(focused.sizeBytes)}</dd>
        {#if !focused.reviewOnly}<dt>{t("files.files")}</dt><dd>{count(focused.itemCount)}</dd>{/if}
      </dl>
      {#if focused.reviewOnly && unknownItems(focused).length}
        {@const unknown = unknownItems(focused)}
        {@const answered = unknown.filter((i) => answerFor(i.path))}
        {@const left = unknown.length - answered.length}
        <div class="ai-bar">
          {#if ai.batch}
            <p><span class="spinner" aria-hidden="true"></span>{t("ai.all.asking", { done: ai.batch.done, total: ai.batch.total })}</p>
            <button class="link" onclick={stopAskAll}>{t("ai.all.stop")}</button>
          {:else if confirmAll}
            <p>{t("ai.all.confirm", { count: left, service: serviceLabel(settings.ai.service, settings.ai.model) })}</p>
            <div class="ai-buttons">
              <button class="btn btn-ai" onclick={() => startAskAll(focused)}>✨ {t("ai.all.ask", { count: left })}</button>
              <button class="btn btn-ghost" onclick={() => (confirmAll = false)}>{t("common.cancel")}</button>
            </div>
          {:else}
            {#if answered.length}
              {@const first = answerFor(answered[0].path)}
              <p>✨ {t(answered.length === 1 ? "ai.all.lookedOne" : "ai.all.looked", { count: answered.length })}{first ? ` · ${serviceLabel(first.service, first.model)}` : ""}</p>
            {:else}
              <p>✨ {t(left === 1 ? "ai.all.introOne" : "ai.all.intro", { count: left })}</p>
            {/if}
            <div class="ai-buttons">
              {#if left}
                <button class="btn btn-ai" onclick={() => startAskAll(focused)}>✨ {answered.length ? t("ai.all.askRest", { count: left }) : t("ai.all.askAll")}</button>
              {/if}
              {#if answered.length}
                <button class="link" onclick={() => forgetAnswers(answered.map((i) => i.path))}>{t("ai.all.forget")}</button>
              {/if}
            </div>
          {/if}
          {#if allFailed}<p class="ai-error" role="alert">{allFailed}</p>{/if}
        </div>
      {/if}
      <div class="label list-label">{t("files.largest")}</div>
      <ul class="items" class:all={showAll}>
        {#each showAll ? focused.items : focused.items.slice(0, SHOWN_ITEMS) as item (item.path)}
          {@const canExplain = explainable(focused, item)}
          <li title={item.duplicateOf ? `${item.path}\n${t("files.copyOf", { path: item.duplicateOf })}` : (item.originalPath ?? item.path)}>
            <svelte:element this={canExplain ? "button" : "div"} class="item-row" class:clickable={canExplain}
              onclick={canExplain ? () => (explained = item) : undefined} role={canExplain ? undefined : "presentation"}>
              <div class="item-text">
                <div class="item-name">{nameOf(item.originalPath ?? item.path)}</div>
                <div class="muted item-note">{itemNote(item)}</div>
              </div>
              {#if canExplain}<ExplainTag explainId={item.explainId} answer={answerFor(item.path)} />{/if}
              <span class="item-size">{formatBytes(item.sizeBytes)}</span>
            </svelte:element>
            <button class="hide-btn" disabled={moving} onclick={() => hideDetail(item)} aria-label={t("files.hideItem", { name: nameOf(item.originalPath ?? item.path) })} title={t("files.hide")}>
              <EyeOffIcon size={15} />
            </button>
          </li>
        {/each}
      </ul>
      {#if focused.items.length > SHOWN_ITEMS}
        <button class="show-all" onclick={() => (showAll = !showAll)}>
          {showAll ? t("files.showLess") : t("files.showAll", { count: count(focused.items.length) })}
        </button>
      {/if}
      {#if notListed(focused) > 0 && (showAll || focused.items.length <= SHOWN_ITEMS)}
        <div class="muted more">{t("files.notListed", { count: count(notListed(focused)) })}</div>
      {/if}
      <div class="grow"></div>
      {#if reviewable(focused)}
        {@const g = focused}
        <button class="btn btn-ghost wide" onclick={() => openReview(g)}>🔍 {t("files.review.oneByOne")}</button>
      {/if}
      {#if focused.reviewOnly}
        <p class="muted review-note">🔎 {t("files.reviewOnly")}</p>
      {:else}
        {@const id = focused.id}
        {@const deletes = DELETE_NOW_GROUPS.includes(id)}
        {#if deletes}<p class="muted how">{t("files.deletedRightAway")}</p>{/if}
        <button class="btn btn-primary wide" disabled={moving} onclick={() => remove([id])}>
          {moving ? t("files.moving") : actionLabel(deletes, !deletes, focused.sizeBytes)}
        </button>
      {/if}
    {:else}
      <p class="muted">{t("files.pickGroup")}</p>
    {/if}
  </aside>
</div>
{/if}

{#if ai.setup}
  <AiSetup />
{/if}

{#if notice}
  {@const n = notice}
  <div class="card toast" class:error={n.kind === "error"} class:raised={reviewing !== null} role="status" aria-live="polite">
    {#if n.kind === "hidden"}<span class="toast-icon" aria-hidden="true"><EyeOffIcon size={20} /></span>{:else}<StatusIcon kind={n.kind === "error" ? "warn" : "ok"} />{/if}
    <div class="toast-text">
      {#each n.kind === "moved" ? removedLines(n.report, quarantine.keepDays) : n.kind === "folderMoved" ? folderMovedLines(n.report, n.item) : n.lines as line, i (i)}<p>{line}</p>{/each}
    </div>
    {#if n.kind === "hidden"}
      <button class="btn btn-ghost" disabled={undoing} onclick={() => unhide(n.items)}>↩ {t("files.undo")}</button>
    {/if}
    {#if (n.kind === "moved" || n.kind === "folderMoved") && n.report.entryIds.length}
      <button class="btn btn-ghost" disabled={undoing} onclick={() => undo(n.report)}>
        {undoing ? t("files.undoing") : `↩ ${t("files.undo")}`}
      </button>
    {/if}
    <button class="close" aria-label={t("common.close")} onclick={() => (notice = null)}>✕</button>
  </div>
{/if}

{#if menu}
  {@const m = menu}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="card menu" style="left:{m.x}px;top:{m.y}px" role="menu" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    {#if reviewable(m.group)}
      <button role="menuitem" onclick={() => act(openReview)}>🔍 {t("files.review.oneByOne")}</button>
      <hr />
    {/if}
    <button role="menuitem" class="menu-hide" onclick={() => act((g) => hide([hiddenItem("group", g.id, g.titleKey)]))}>
      <EyeOffIcon />
      <span>{t("files.hideGroup")}<span class="muted menu-hint">{t("files.hideGroupHint")}</span></span>
    </button>
  </div>
{/if}

<svelte:window onclick={() => (menu = null)} onkeydown={(e) => e.key === "Escape" && (menu = null)} onresize={() => (menu = null)} />

<style>
  .layout { display: flex; gap: 18px; align-items: flex-start; }
  .col { flex: 1; min-width: 0; }
  .head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .head .btn { flex: none; margin-top: 4px; }
  .sub { margin: 3px 0 10px; }
  h2 { font-size: 12px; font-weight: 600; color: var(--text-muted); letter-spacing: 0.3px; text-transform: uppercase; margin: 14px 0 6px; }
  .group { display: flex; align-items: center; gap: 12px; padding: 11px 14px; margin-bottom: 7px; border-width: 2px; border-color: transparent; cursor: pointer; }
  .group.focused { border-color: var(--accent); }
  .group input { width: 18px; height: 18px; accent-color: var(--accent); }
  .no-check { width: 18px; text-align: center; font-size: 14px; }
  .info { min-width: 0; flex: 1; }
  .title { font-size: 14px; font-weight: 600; }
  .desc { font-size: 12px; }
  .size { font-size: 14px; font-weight: 600; width: 76px; text-align: right; }
  .footer { display: flex; align-items: center; gap: 10px; margin-top: 14px; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .grow { flex: 1; }
  .how { font-size: 12px; margin-top: 8px; text-align: right; }
  .side .how { text-align: left; margin-top: 12px; }

  .scan { padding: 22px 24px; margin-top: 8px; }
  .scan-title { font-size: 17px; font-weight: 600; }
  .steps { display: flex; gap: 6px; margin: 14px 0 10px; }
  .steps i { flex: 1; height: 10px; border-radius: 5px; background: var(--border); }
  .steps i.done { background: var(--accent); }
  .steps i.current { background: var(--accent-soft); position: relative; overflow: hidden; }
  .steps i.current::after { content: ""; position: absolute; inset: 0; width: 40%; border-radius: 5px; background: var(--accent); animation: sweep 1.2s ease-in-out infinite; }
  @keyframes sweep { from { transform: translateX(-100%); } to { transform: translateX(250%); } }
  .scan-row { display: flex; justify-content: space-between; gap: 12px; font-size: 13px; flex-wrap: wrap; }
  .note { font-size: 12.5px; margin-top: 14px; }
  .message { padding: 22px 24px; margin-top: 8px; display: flex; align-items: center; gap: 14px; }
  .message p { flex: 1; }
  .big-icon { font-size: 26px; }

  /* Wider on a wide window: the group rows need less room than the folder list. */
  .side { width: clamp(340px, 34vw, 600px); flex: none; padding: 18px; min-height: 420px; display: flex; flex-direction: column; position: sticky; top: 0; }
  .label { font-size: 11px; font-weight: 600; letter-spacing: 0.3px; color: var(--text-muted); text-transform: uppercase; }
  .side h3 { font-size: 17px; margin: 6px 0 4px; }
  .side h4 { font-size: 12.5px; margin-top: 10px; }
  .about { font-size: 12.5px; line-height: 1.45; margin-top: 2px; }
  .review-note { font-size: 12.5px; margin-top: 14px; }
  dl { display: grid; grid-template-columns: 1fr auto; row-gap: 3px; font-size: 12.5px; margin-top: 10px; border-top: 1px solid var(--border); padding-top: 8px; }
  dt { color: var(--text-muted); }
  dd { font-weight: 600; }
  .list-label { margin-top: 14px; }
  .items { list-style: none; margin-top: 6px; }
  /* Hide sits last on every row, in its own place: it never covers the tag or the size. */
  .items li { border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 4px; }
  .item-row { display: flex; align-items: center; gap: 8px; padding: 6px 0; flex: 1; min-width: 0; border: none; background: none; color: inherit; font: inherit; text-align: left; }
  .item-row.clickable { cursor: pointer; padding: 6px 6px; margin-left: -6px; border-radius: var(--radius-sm); }
  .item-row.clickable:hover { background: var(--hover); }
  .item-text { flex: 1; min-width: 0; }
  .item-name, .item-note { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .item-name { font-size: 12.5px; font-weight: 600; }
  .item-note { font-size: 11px; }
  .item-size { font-size: 12px; font-weight: 600; flex: none; }
  .more { font-size: 12px; margin-top: 6px; }
  .items.all { max-height: 46vh; overflow-y: auto; padding-right: 6px; }
  .show-all { border: none; background: none; color: var(--accent); font-size: 12.5px; padding: 6px 0 0; text-align: left; cursor: pointer; }
  .wide { width: 100%; padding: 10px; margin-top: 14px; }
  .wide + .wide { margin-top: 8px; }
  .ai-bar { margin-top: 12px; border: 1px solid var(--tip-border); background: var(--tip-bg); border-radius: var(--radius-sm); padding: 9px 11px; font-size: 12.5px; line-height: 1.4; }
  .ai-bar p { display: flex; align-items: center; gap: 6px; }
  .ai-buttons { display: flex; align-items: center; gap: 10px; margin-top: 8px; flex-wrap: wrap; }
  .ai-buttons .btn { padding: 6px 12px; font-size: 12.5px; }
  .ai-bar .link { border: none; background: none; color: var(--accent); padding: 0; font-size: 12.5px; }
  .ai-error { color: var(--keep-fg); margin-top: 6px; }
  .spinner { width: 12px; height: 12px; border-radius: 50%; border: 2px solid var(--tip-border); border-top-color: var(--ai); animation: spin 0.9s linear infinite; flex: none; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .review-link { border: none; background: none; color: var(--accent); font-size: 12.5px; font-weight: 600; padding: 4px 6px; border-radius: var(--radius-sm); white-space: nowrap; }
  .review-link:hover { background: var(--accent-soft); }

  .dots { flex: none; border: none; background: none; font-size: 18px; line-height: 1; padding: 4px 8px; margin-right: -6px; border-radius: var(--radius-sm); color: var(--text-muted); }
  .dots:hover { background: var(--hover); color: var(--text); }
  /* Over the size on hover, so the name keeps its room. */
  .hide-btn { flex: none; display: inline-flex; align-items: center; justify-content: center; width: 26px; height: 26px; margin-right: -4px; border: none; background: none; border-radius: var(--radius-sm); color: var(--text-muted); opacity: 0.55; transition: opacity 0.12s, background 0.12s; }
  .items li:hover .hide-btn, .hide-btn:focus-visible { opacity: 1; }
  .hide-btn:hover { background: var(--hover); color: var(--text); }
  .hide-btn:disabled { opacity: 0.3; cursor: default; }
  .hidden-link { display: flex; align-items: center; gap: 8px; margin-top: 16px; border: none; background: none; padding: 6px 2px; font-size: 13px; color: var(--text-muted); text-align: left; }
  .link-action { color: var(--accent); font-weight: 600; }
  .hidden-link:hover .link-action { text-decoration: underline; }
  .menu { position: fixed; z-index: 20; width: 290px; padding: 6px; box-shadow: var(--shadow-pop); display: flex; flex-direction: column; }
  .menu button { border: none; background: none; text-align: left; padding: 8px 10px; border-radius: 5px; font-size: 13px; }
  .menu button:hover, .menu button:focus-visible { background: var(--hover); outline: none; }
  .menu hr { border: none; border-top: 1px solid var(--border); margin: 4px 0; }
  .menu-hide { display: flex; align-items: flex-start; gap: 8px; }
  .menu-hide :global(.eye-off) { margin-top: 2px; }
  .menu-hint { display: block; font-size: 12px; margin-top: 2px; }
  .toast { position: fixed; right: 30px; bottom: 22px; max-width: 560px; display: flex; align-items: center; gap: 12px; padding: 12px 14px 12px 16px; box-shadow: var(--shadow-pop); border-left: 4px solid var(--safe-fg); z-index: 10; }
  .toast.error { border-left-color: var(--danger); }
  /* Above the review's sticky footer, so its buttons stay reachable. */
  .toast.raised { bottom: 96px; }
  .toast-icon { font-size: 18px; display: flex; color: var(--text-muted); }
  .toast-text { flex: 1; font-size: 13px; line-height: 1.4; }
  .toast-text p + p { color: var(--text-muted); margin-top: 2px; }
  .toast .btn { flex: none; }
  .close { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 4px; }
</style>
