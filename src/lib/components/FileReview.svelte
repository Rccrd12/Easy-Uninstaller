<script lang="ts">
  import { untrack } from "svelte";
  import FileThumb from "./FileThumb.svelte";
  import { openScannedFile, reviewFileGroup, showScannedFile } from "../api";
  import { quarantine } from "../settings.svelte";
  import { fileCount, formatBytes, formatDate } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import { displayPath, folderOf, nameOf, topFolder, topFolderName } from "../paths";
  import type { Exclusion, FileGroup, FileSelection, GroupReview, ReviewFile } from "../types";

  let {
    group,
    version,
    busy,
    onBack,
    onMove,
    onHide,
  }: {
    group: FileGroup;
    /** Changes after every move, undo or hidden file: the list is read again. */
    version: number;
    busy: boolean;
    onBack: () => void;
    onMove: (selection: FileSelection) => Promise<void>;
    onHide: (exclusion: Exclusion) => Promise<void>;
  } = $props();

  type View = "list" | "grid";
  type Sort = "size" | "date" | "name";

  let review = $state<GroupReview | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let view = $state<View>("list");
  let sort = $state<Sort>("size");
  let selected = $state<Set<string>>(new Set());
  /** Duplicates: "always keep the copy in" this top folder ("" = no preference), then the oldest or newest. */
  let keepFolder = $state("");
  let keepOrder = $state<"oldest" | "newest">("oldest");
  /** Duplicates: copies the user chose to keep by hand ("keep this one instead"). */
  let keepPaths = $state<Set<string>>(new Set());
  let menu = $state<{ file: ReviewFile; set: ReviewFile[] | null; x: number; y: number } | null>(null);

  const isDuplicates = $derived(group.id === "duplicates");
  let loadedOnce = false;

  $effect(() => {
    const id = group.id;
    void version;
    // Only the group and `version` start a new read; the rest is read without tracking.
    untrack(() => {
      loading = !review;
      error = null;
    });
    reviewFileGroup(id)
      .then((r) => {
        const first = !loadedOnce;
        loadedOnce = true;
        review = r;
        if (!r) return;
        if (r.duplicateSets.length && first) {
          applyRule();
        } else if (first) {
          // Safe groups start selected; anything that needs a look starts empty.
          selected = new Set(group.safety === "safe" ? r.files.map((f) => f.path) : []);
        } else {
          // After a move or a hidden file: keep the choices made for what is still there.
          const left = new Set(selectable);
          selected = new Set([...selected].filter((p) => left.has(p)));
        }
      })
      .catch((e) => (error = String(e)))
      .finally(() => (loading = false));
  });

  const sizeOf = $derived(new Map([...(review?.files ?? []), ...(review?.duplicateSets.flat() ?? [])].map((f) => [f.path, f.sizeBytes])));
  const selectedBytes = $derived([...selected].reduce((s, p) => s + (sizeOf.get(p) ?? 0), 0));

  const byName = (a: ReviewFile, b: ReviewFile) => nameOf(a.path).localeCompare(nameOf(b.path), locale());
  const files = $derived.by(() => {
    const list = [...(review?.files ?? [])];
    if (sort === "date") list.sort((a, b) => a.lastUsed.localeCompare(b.lastUsed));
    else if (sort === "name") list.sort(byName);
    return list;
  });

  /** Duplicate sets in the chosen order. */
  const sets = $derived.by(() => {
    const list = [...(review?.duplicateSets ?? [])];
    const oldest = (s: ReviewFile[]) => s.reduce((m, f) => (f.lastUsed < m ? f.lastUsed : m), s[0].lastUsed);
    if (sort === "date") list.sort((a, b) => oldest(a).localeCompare(oldest(b)));
    else if (sort === "name") list.sort((a, b) => byName(a[0], b[0]));
    return list;
  });

  /** Top folders the duplicates are in, most copies first: the choices of the keep rule. */
  const folders = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const f of review?.duplicateSets.flat() ?? []) {
      const top = topFolder(f.path);
      if (top) counts.set(top, (counts.get(top) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([folder]) => folder);
  });

  /** The copy the rule keeps: one in the chosen folder if any, then the oldest (or newest). */
  function ruleKeeps(set: ReviewFile[]): string {
    const inFolder = keepFolder ? set.filter((f) => topFolder(f.path) === keepFolder) : [];
    const pool = [...(inFolder.length ? inFolder : set)];
    pool.sort((a, b) => a.modified.localeCompare(b.modified) || a.path.localeCompare(b.path));
    return (keepOrder === "oldest" ? pool[0] : pool[pool.length - 1]).path;
  }

  const keptOf = (set: ReviewFile[]) => set.find((f) => keepPaths.has(f.path))?.path ?? ruleKeeps(set);

  /** Selects every copy except the one each set keeps. */
  function applyRule() {
    const next = new Set<string>();
    (review?.duplicateSets ?? []).forEach((set) => {
      const kept = keptOf(set);
      set.forEach((f) => f.path !== kept && next.add(f.path));
    });
    selected = next;
  }

  function changeRule() {
    keepPaths = new Set();
    applyRule();
  }

  function keepInstead(file: ReviewFile, set: ReviewFile[]) {
    const before = keptOf(set);
    const keep = new Set(keepPaths);
    set.forEach((f) => keep.delete(f.path));
    keepPaths = keep.add(file.path);
    const next = new Set(selected);
    next.delete(file.path);
    next.add(before);
    selected = next;
  }

  function toggle(path: string) {
    const next = new Set(selected);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    selected = next;
  }

  /** Everything that can be picked: every file, or every copy a set does not keep. */
  const selectable = $derived(
    isDuplicates
      ? (review?.duplicateSets ?? []).flatMap((set) => set.filter((f) => f.path !== keptOf(set)).map((f) => f.path))
      : (review?.files ?? []).map((f) => f.path),
  );
  const allSelected = $derived(selectable.length > 0 && selectable.every((p) => selected.has(p)));

  function selectAll() {
    selected = allSelected ? new Set() : new Set(selectable);
  }

  async function move() {
    if (!selected.size || busy) return;
    await onMove({ groupId: group.id, paths: [...selected] });
  }

  function openMenu(e: MouseEvent, file: ReviewFile, set: ReviewFile[] | null) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const width = 270;
    menu = { file, set, x: Math.max(8, Math.min(r.right - width, window.innerWidth - width - 8)), y: r.bottom + 4 };
  }

  function closeMenu() {
    menu = null;
  }

  /** Runs a menu action, then closes the menu (the action reads the menu's file first). */
  async function act(action: () => unknown) {
    let pending: unknown = null;
    try {
      pending = action();
    } catch {
      // Handled like a failed promise below.
    }
    closeMenu();
    try {
      await pending;
    } catch {
      // Open and show are best effort: nothing to undo if Windows refuses.
    }
  }

  const count = (n: number) => n.toLocaleString(locale());
  const folderName = (path: string) => {
    const folder = folderOf(path);
    const top = topFolder(path);
    return top && folder.endsWith(`\\${top}`) ? topFolderName(top) : nameOf(folder);
  };
</script>

<svelte:window onclick={closeMenu} onkeydown={(e) => e.key === "Escape" && closeMenu()} onresize={closeMenu} />

<div class="review">
  <button class="back" onclick={onBack}>‹ {t("files.title")}</button>
  <div class="head">
    <div class="titles">
      <h1>{t(group.titleKey)}</h1>
      {#if review && review.totalCount > 0}
        <p class="muted sub">
          {isDuplicates
            ? t("files.review.dupSubtitle", { count: count(review.totalCount), size: formatBytes(review.totalBytes) })
            : t("files.review.subtitle", { files: fileCount(review.totalCount), size: formatBytes(review.totalBytes) })}
        </p>
      {/if}
    </div>
    <div class="segmented" role="radiogroup" aria-label={t("files.review.view")}>
      <button role="radio" aria-checked={view === "list"} class:on={view === "list"} onclick={() => (view = "list")}>☰ {t("files.review.list")}</button>
      <button role="radio" aria-checked={view === "grid"} class:on={view === "grid"} onclick={() => (view = "grid")}>▦ {t("files.review.previews")}</button>
    </div>
  </div>

  {#if loading}
    <div class="card message" role="status">{t("files.review.loading")}</div>
  {:else if error}
    <div class="card message">{t("files.failed", { error })}</div>
  {:else if !review || (review.files.length === 0 && review.duplicateSets.length === 0)}
    <div class="card message">
      <span class="big-icon" aria-hidden="true">✨</span>
      <p>{t("files.review.empty")}</p>
      <button class="btn btn-ghost" onclick={onBack}>{t("files.review.backToFiles")}</button>
    </div>
  {:else}
    <div class="card toolbar">
      {#if isDuplicates}
        <label class="rule">
          <span>{t("files.review.keepIn")}</span>
          <select bind:value={keepFolder} onchange={changeRule}>
            <option value="">{t("files.review.anyFolder")}</option>
            {#each folders as folder (folder)}<option value={folder}>{topFolderName(folder)}</option>{/each}
          </select>
        </label>
        <label class="rule">
          <span class="muted">{t("files.review.then")}</span>
          <select bind:value={keepOrder} onchange={changeRule}>
            <option value="oldest">{t("files.review.oldest")}</option>
            <option value="newest">{t("files.review.newest")}</option>
          </select>
        </label>
      {:else}
        <span class="muted hint">{group.safety === "safe" ? t("files.review.hintSafe") : t("files.review.hintCheck")}</span>
      {/if}
      <span class="grow"></span>
      <label class="rule">
        <span class="muted">{t("files.review.sortBy")}</span>
        <select bind:value={sort}>
          <option value="size">{t("files.review.sortSize")}</option>
          <option value="date">{t("files.review.sortDate")}</option>
          <option value="name">{t("files.review.sortName")}</option>
        </select>
      </label>
    </div>

    {#if review.totalCount > review.files.length && !isDuplicates}
      <p class="muted capped">{t("files.review.capped", { shown: count(review.files.length), total: count(review.totalCount) })}</p>
    {/if}

    {#if isDuplicates}
      {#each sets as set (set[0].path)}
        {@const kept = keptOf(set)}
        <section class="card set">
          <div class="set-head">
            <b class="set-name">{nameOf(set[0].path)}</b>
            <span class="muted">{t("files.review.copies", { count: set.length, size: formatBytes(set[0].sizeBytes) })}</span>
          </div>
          <div class="copies" class:grid={view === "grid"}>
            {#each [...set].sort((a, b) => Number(b.path === kept) - Number(a.path === kept)) as file (file.path)}
              {@const keep = file.path === kept}
              <div class="copy" class:keep class:picked={selected.has(file.path)}>
                {#if keep}
                  <span class="check-space"></span>
                {:else}
                  <input type="checkbox" checked={selected.has(file.path)} onchange={() => toggle(file.path)}
                         aria-label={t("files.review.pick", { name: displayPath(file.path) })} />
                {/if}
                <FileThumb path={file.path} size={view === "grid" ? 96 : 44} />
                <div class="text">
                  <div class="line">
                    {#if keep}<span class="tag tag-safe">{t("files.review.keep")}</span>{/if}
                    <span class="path" title={file.path}>
                      {displayPath(nameOf(file.path) === nameOf(set[0].path) ? folderOf(file.path) : file.path)}
                    </span>
                  </div>
                  <div class="muted small">{t("files.review.changed", { date: formatDate(file.modified) })}</div>
                </div>
                <button class="dots" aria-label={t("files.review.more")} onclick={(e) => openMenu(e, file, set)}>⋯</button>
              </div>
            {/each}
          </div>
        </section>
      {/each}
    {:else if view === "list"}
      <div class="card rows">
        {#each files as file (file.path)}
          <div class="row" class:picked={selected.has(file.path)}>
            <input type="checkbox" checked={selected.has(file.path)} onchange={() => toggle(file.path)}
                   aria-label={t("files.review.pick", { name: nameOf(file.path) })} />
            <FileThumb path={file.path} size={40} />
            <div class="text">
              <div class="name" title={file.path}>{nameOf(file.path)}</div>
              <div class="muted small">{displayPath(folderOf(file.path))} · {t("files.lastUsed", { date: formatDate(file.lastUsed) })}</div>
            </div>
            <span class="size">{formatBytes(file.sizeBytes)}</span>
            <button class="dots" aria-label={t("files.review.more")} onclick={(e) => openMenu(e, file, null)}>⋯</button>
          </div>
        {/each}
      </div>
    {:else}
      <div class="tiles">
        {#each files as file (file.path)}
          <div class="card tile" class:picked={selected.has(file.path)}>
            <label class="tile-pick">
              <input type="checkbox" checked={selected.has(file.path)} onchange={() => toggle(file.path)}
                     aria-label={t("files.review.pick", { name: nameOf(file.path) })} />
            </label>
            <button class="dots tile-dots" aria-label={t("files.review.more")} onclick={(e) => openMenu(e, file, null)}>⋯</button>
            <button class="tile-thumb" onclick={() => toggle(file.path)} tabindex="-1" aria-hidden="true">
              <FileThumb path={file.path} size={132} />
            </button>
            <div class="name" title={file.path}>{nameOf(file.path)}</div>
            <div class="muted small">{formatBytes(file.sizeBytes)} · {formatDate(file.lastUsed)}</div>
          </div>
        {/each}
      </div>
    {/if}

    <div class="card footer">
      <div class="footer-text">
        <b>{t("files.review.selected", { files: fileCount(selected.size), size: formatBytes(selectedBytes) })}</b>
        <span class="muted small">
          {isDuplicates ? t("files.review.oneCopyStays") : t("files.review.restorable", { days: quarantine.keepDays })}
        </span>
      </div>
      <span class="grow"></span>
      <button class="btn btn-ghost" onclick={selectAll}>{allSelected ? t("files.review.selectNone") : t("files.review.selectAll")}</button>
      <button class="btn btn-primary" disabled={!selected.size || busy} onclick={move}>
        {busy ? t("files.moving") : `${t("common.moveToQuarantine")} (${formatBytes(selectedBytes)})`}
      </button>
    </div>
  {/if}
</div>

{#if menu}
  {@const m = menu}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="card menu" style="left:{m.x}px;top:{m.y}px" role="menu" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    {#if m.file.canOpen}
      <button role="menuitem" onclick={() => act(() => openScannedFile(m.file.path))}>📂 {t("files.review.open")}</button>
    {/if}
    <button role="menuitem" onclick={() => act(() => showScannedFile(m.file.path))}>🔎 {t("files.review.showInFolder")}</button>
    {#if m.set && keptOf(m.set) !== m.file.path}
      {@const set = m.set}
      <button role="menuitem" onclick={() => act(() => keepInstead(m.file, set))}>⇄ {t("files.review.keepInstead")}</button>
    {/if}
    <hr />
    <button role="menuitem" onclick={() => act(() => onHide({ path: m.file.path, folder: false }))}>🚫 {t("files.review.hideFile")}</button>
    <button role="menuitem" onclick={() => act(() => onHide({ path: folderOf(m.file.path), folder: true }))}>
      🚫 {t("files.review.hideFolder", { folder: folderName(m.file.path) })}
    </button>
  </div>
{/if}

<style>
  .review { padding-bottom: 8px; }
  .back { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 2px 0; margin-bottom: 4px; }
  .back:hover { color: var(--accent); }
  .head { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .titles { min-width: 0; padding-bottom: 12px; }
  .sub { margin-top: 3px; }
  .segmented { display: inline-flex; flex: none; background: var(--track); border-radius: 8px; padding: 3px; margin-top: 4px; }
  .segmented button { border: none; background: none; padding: 5px 12px; font-size: 12.5px; border-radius: 6px; }
  .segmented .on { background: var(--raised); font-weight: 600; box-shadow: var(--shadow); }

  .toolbar { display: flex; align-items: center; gap: 14px; padding: 10px 14px; margin-bottom: 10px; flex-wrap: wrap; }
  .rule { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .hint { font-size: 12.5px; }
  select { font-family: inherit; font-size: 13px; padding: 5px 8px; border-radius: var(--radius-sm); border: 1px solid var(--border-strong); }
  .grow { flex: 1; }
  .capped { font-size: 12.5px; margin: -4px 0 8px; }
  .small { font-size: 12px; }
  .message { padding: 22px 24px; display: flex; align-items: center; gap: 14px; }
  .message p { flex: 1; }
  .big-icon { font-size: 26px; }

  input[type="checkbox"] { width: 18px; height: 18px; accent-color: var(--accent); flex: none; }
  .dots { flex: none; border: none; background: none; font-size: 18px; line-height: 1; padding: 4px 8px; border-radius: var(--radius-sm); color: var(--text-muted); }
  .dots:hover { background: var(--hover); color: var(--text); }
  .text { flex: 1; min-width: 0; }
  .name, .path { font-weight: 600; font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .text .small { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .rows { overflow: hidden; }
  .row { display: flex; align-items: center; gap: 12px; padding: 8px 12px 8px 14px; border-bottom: 1px solid var(--border); }
  .row:last-child { border-bottom: none; }
  .row.picked { background: var(--accent-soft); }
  .size { font-size: 13px; font-weight: 600; width: 72px; text-align: right; flex: none; }

  .tiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(156px, 1fr)); gap: 10px; }
  .tile { position: relative; padding: 10px; display: flex; flex-direction: column; gap: 3px; border-width: 2px; }
  .tile.picked { border-color: var(--accent); }
  .tile-thumb { border: none; background: none; padding: 0; margin-bottom: 4px; display: flex; justify-content: center; }
  .tile-thumb :global(.thumb) { width: 100% !important; aspect-ratio: 1; height: auto !important; }
  .tile-pick { position: absolute; top: 14px; left: 14px; z-index: 1; background: var(--surface); border-radius: 4px; display: flex; padding: 2px; }
  .tile-dots { position: absolute; top: 12px; right: 12px; z-index: 1; background: var(--surface); }

  .set { padding: 12px 14px; margin-bottom: 10px; }
  .set-head { display: flex; justify-content: space-between; gap: 12px; font-size: 13px; margin-bottom: 8px; }
  .set-name { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .set-head .muted { flex: none; }
  .copies { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 10px; }
  .copies.grid { grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); }
  .copy { display: flex; align-items: center; gap: 10px; border: 1px solid var(--border); border-radius: 8px; padding: 8px 6px 8px 10px; min-width: 0; }
  .copies.grid .copy { flex-wrap: wrap; align-items: flex-start; }
  .copies.grid .copy .text { flex-basis: 100%; }
  .copy.keep { background: var(--keep-copy-bg); border-color: var(--keep-copy-border); }
  .copy.picked { background: var(--accent-soft); }
  .check-space { width: 18px; flex: none; }
  .line { display: flex; align-items: center; gap: 6px; min-width: 0; }

  .footer { position: sticky; bottom: 0; display: flex; align-items: center; gap: 10px; padding: 12px 16px; margin-top: 12px; box-shadow: var(--shadow-up); z-index: 2; }
  .footer-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .btn:disabled { opacity: 0.5; cursor: default; }

  .menu { position: fixed; z-index: 20; width: 270px; padding: 6px; box-shadow: var(--shadow-pop); display: flex; flex-direction: column; }
  .menu button { border: none; background: none; text-align: left; padding: 8px 10px; border-radius: 5px; font-size: 13px; }
  .menu button:hover, .menu button:focus-visible { background: var(--hover); outline: none; }
  .menu hr { border: none; border-top: 1px solid var(--border); margin: 4px 0; }
</style>
