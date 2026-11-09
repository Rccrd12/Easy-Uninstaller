<script lang="ts">
  import { untrack } from "svelte";
  import FileThumb from "./FileThumb.svelte";
  import Icon from "./Icon.svelte";
  import SearchBar from "./SearchBar.svelte";
  import StatusIcon from "./StatusIcon.svelte";
  import { openScannedFile, quarantineFound, restoreQuarantine, searchFiles, searchUnderstand, showScannedFile } from "../api";
  import { aiConfig, aiErrorText, aiReady } from "../ai.svelte";
  import { fileCount, formatBytes, formatDate, GB, MB, removedLines, restoreSummary } from "../format";
  import { getLanguage, locale, t } from "../i18n/index.svelte";
  import { displayPath, fileKind, folderOf, nameOf } from "../paths";
  import { quarantine, settings } from "../settings.svelte";
  import type { MoveReport, SearchFilter, SearchKind, SearchPlace, SearchResult, SearchSort } from "../types";

  /**
   * Finding files by a sentence (AI help, phase 2). The AI turns the sentence into a
   * filter, shown as chips the user can remove or change; the search itself runs on this
   * PC. Nothing is picked or moved on its own: picked files go to Quarantine.
   */
  let { sentence, onBack }: { sentence: string; onBack: () => void } = $props();

  const KINDS: SearchKind[] = ["video", "image", "document", "audio", "archive", "program", "disc"];
  const PLACES: SearchPlace[] = ["desktop", "documents", "downloads", "pictures", "videos", "music"];
  const SIZES = [10 * MB, 100 * MB, 500 * MB, GB, 5 * GB];
  const UNUSED = [30, 90, 180, 365, 730, 1825];
  const RECENT = [1, 7, 30, 365];
  const empty = (): SearchFilter => ({ kinds: [], minBytes: null, maxBytes: null, unusedDays: null, recentDays: null, places: [], words: [], sort: "size" });

  let current = $state("");
  let filter = $state<SearchFilter | null>(null);
  let byName = $state(false);
  let phase = $state<"understanding" | "searching" | "done" | "error">("understanding");
  let error = $state<string | null>(null);
  let result = $state<SearchResult | null>(null);
  let selected = $state<Set<string>>(new Set());
  let editing = $state(false);
  let moving = $state(false);
  let undoing = $state(false);
  let notice = $state<{ kind: "moved"; report: MoveReport } | { kind: "info" | "error"; lines: string[] } | null>(null);

  // Whichever answer comes back last for an older sentence or filter is dropped.
  let runId = 0;
  let searchId = 0;

  $effect.pre(() => {
    current = sentence;
  });
  $effect(() => {
    const s = current;
    untrack(() => run(s));
  });

  const service = $derived(t(`ai.services.${settings.ai.service}.name`));

  function errorText(e: unknown): string {
    const key = String(e);
    return key.startsWith("search.error.") || key.startsWith("tidy.error.") ? t(key) : key.startsWith("ai.error.") ? aiErrorText(e) : t("search.failed", { error: key });
  }

  async function run(s: string) {
    const id = ++runId;
    error = null;
    result = null;
    editing = false;
    selected = new Set();
    let next: SearchFilter;
    if (aiReady()) {
      byName = false;
      phase = "understanding";
      try {
        next = await searchUnderstand(aiConfig(), s, getLanguage());
      } catch (e) {
        if (id !== runId) return;
        error = errorText(e);
        phase = "error";
        return;
      }
    } else {
      byName = true;
      next = { ...empty(), words: s.split(/[\s,]+/).filter(Boolean) };
    }
    if (id !== runId) return;
    filter = next;
    await search();
  }

  async function search() {
    if (!filter) return;
    const id = ++searchId;
    phase = "searching";
    try {
      const r = await searchFiles($state.snapshot(filter));
      if (id !== searchId) return;
      result = r;
      filter = r.filter;
      selected = new Set([...selected].filter((p) => r.files.some((f) => f.path === p)));
      phase = "done";
    } catch (e) {
      if (id !== searchId) return;
      error = errorText(e);
      phase = "error";
    }
  }

  function change(next: Partial<SearchFilter>) {
    if (!filter) return;
    filter = { ...filter, ...next };
    search();
  }

  /** 365 -> "1 year", 180 -> "6 months", 14 -> "2 weeks". */
  function daysText(d: number): string {
    const unit = (n: number, one: string, many: string) => (n === 1 ? t(`search.time.${one}`) : t(`search.time.${many}`, { count: n.toLocaleString(locale()) }));
    if (d % 365 === 0) return unit(d / 365, "year", "years");
    if (d % 30 === 0) return unit(d / 30, "month", "months");
    if (d % 7 === 0) return unit(d / 7, "week", "weeks");
    return unit(d, "day", "days");
  }

  const placeName = (p: SearchPlace) => t(`files.review.folders.${p}`);

  /** The filter in plain words, one chip per part, each with how to take it away. */
  function chips(f: SearchFilter): { label: string; remove: Partial<SearchFilter> }[] {
    const out: { label: string; remove: Partial<SearchFilter> }[] = [];
    for (const k of f.kinds) out.push({ label: t(`search.kinds.${k}`), remove: { kinds: f.kinds.filter((x) => x !== k) } });
    if (f.minBytes !== null) out.push({ label: t("search.chips.minSize", { size: formatBytes(f.minBytes) }), remove: { minBytes: null } });
    if (f.maxBytes !== null) out.push({ label: t("search.chips.maxSize", { size: formatBytes(f.maxBytes) }), remove: { maxBytes: null } });
    if (f.unusedDays !== null) out.push({ label: t("search.chips.unused", { time: daysText(f.unusedDays) }), remove: { unusedDays: null } });
    if (f.recentDays !== null) out.push({ label: t("search.chips.recent", { time: daysText(f.recentDays) }), remove: { recentDays: null } });
    if (f.places.length) out.push({ label: t("search.chips.places", { list: f.places.map(placeName).join(", ") }), remove: { places: [] } });
    for (const w of f.words) out.push({ label: t("search.chips.word", { word: w }), remove: { words: f.words.filter((x) => x !== w) } });
    if (f.sort !== "size") out.push({ label: t(f.sort === "oldest" ? "search.chips.sortOldest" : "search.chips.sortNewest"), remove: { sort: "size" } });
    return out;
  }

  const chipIcon = (f: SearchFilter, i: number) => {
    const kinds = f.kinds.length;
    if (i < kinds) return ({ video: "video", image: "image", audio: "music", document: "document", archive: "archive", program: "box", disc: "drive" } as const)[f.kinds[i]];
    return null;
  };

  function toggleKind(k: SearchKind) {
    if (!filter) return;
    change({ kinds: filter.kinds.includes(k) ? filter.kinds.filter((x) => x !== k) : [...filter.kinds, k] });
  }
  function togglePlace(p: SearchPlace) {
    if (!filter) return;
    change({ places: filter.places.includes(p) ? filter.places.filter((x) => x !== p) : [...filter.places, p] });
  }
  /** A select's options: the presets, plus the filter's own value when it is not one of them. */
  const withValue = (presets: number[], value: number | null) => (value !== null && !presets.includes(value) ? [...presets, value].sort((a, b) => a - b) : presets);
  const numberOrNull = (v: string) => (v === "" ? null : Number(v));

  const files = $derived(result?.files ?? []);
  const selectedBytes = $derived(files.filter((f) => selected.has(f.path)).reduce((s, f) => s + f.sizeBytes, 0));
  const allSelected = $derived(files.length > 0 && files.every((f) => selected.has(f.path)));
  const count = (n: number) => n.toLocaleString(locale());
  const canOpen = (path: string) => !["program", "other"].includes(fileKind(path));

  function toggle(path: string) {
    const next = new Set(selected);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    selected = next;
  }

  async function move() {
    if (!result || !selected.size || moving) return;
    moving = true;
    notice = null;
    try {
      const report = await quarantineFound([...selected], result);
      selected = new Set();
      notice = report.movedCount || report.deletedCount ? { kind: "moved", report } : { kind: "info", lines: [t("files.nothingMoved")] };
      await search();
    } catch (e) {
      notice = { kind: "error", lines: [t("files.moveFailed", { error: String(e) })] };
    } finally {
      moving = false;
    }
  }

  async function undo(report: MoveReport) {
    undoing = true;
    try {
      const restored = await restoreQuarantine(report.entryIds);
      const clean = restored.renamedCount === 0 && restored.failedCount === 0;
      notice = { kind: "info", lines: clean ? [t("files.undone")] : restoreSummary(restored) };
      await search();
    } catch (e) {
      notice = { kind: "error", lines: [t("quarantine.failed", { error: String(e) })] };
    } finally {
      undoing = false;
    }
  }
</script>

<div class="search-view">
  <button class="back" onclick={onBack}><Icon name="chevronLeft" size={14} />{t("files.title")}</button>
  <h1>{t("search.title")}</h1>
  <div class="bar-wrap">
    <SearchBar value={current} busy={phase === "understanding"} onSearch={(s) => (current === s ? run(s) : (current = s))} />
  </div>

  {#if phase === "understanding"}
    <div class="card status" role="status"><span class="spinner" aria-hidden="true"></span>{t("search.understanding", { service })}</div>
  {:else if phase === "error" && !filter}
    <div class="card status error-card" role="alert"><StatusIcon kind="warn" /><span>{error}</span></div>
  {/if}

  {#if filter && phase !== "understanding"}
    {@const f = filter}
    {@const list = chips(f)}
    <div class="chips">
      <span class="muted chips-label">{t(byName ? "search.byName" : "search.understood")}</span>
      {#if list.length === 0}<span class="chip fixed">{t("search.anyFile")}</span>{/if}
      {#each list as chip, i (chip.label)}
        {@const icon = chipIcon(f, i)}
        <span class="chip">
          {#if icon}<Icon name={icon} size={14} />{/if}{chip.label}
          <button class="chip-x" aria-label={t("search.removeChip", { what: chip.label })} onclick={() => change(chip.remove)}><Icon name="dismiss" size={12} /></button>
        </span>
      {/each}
      <button class="link edit" onclick={() => (editing = !editing)}>{editing ? t("search.doneEditing") : t("search.edit")}</button>
    </div>

    {#if editing}
      <div class="card editor">
        <div class="field wide">
          <span class="field-label">{t("search.editor.kinds")}</span>
          <div class="toggles">
            {#each KINDS as k (k)}
              <button class="toggle" class:on={f.kinds.includes(k)} aria-pressed={f.kinds.includes(k)} onclick={() => toggleKind(k)}>{t(`search.kinds.${k}`)}</button>
            {/each}
          </div>
        </div>
        <div class="field wide">
          <span class="field-label">{t("search.editor.places")}</span>
          <div class="toggles">
            <button class="toggle" class:on={f.places.length === 0} aria-pressed={f.places.length === 0} onclick={() => change({ places: [] })}>{t("search.editor.placesAll")}</button>
            {#each PLACES as p (p)}
              <button class="toggle" class:on={f.places.includes(p)} aria-pressed={f.places.includes(p)} onclick={() => togglePlace(p)}>{placeName(p)}</button>
            {/each}
          </div>
        </div>
        <label class="field">
          <span class="field-label">{t("search.editor.size")}</span>
          <select value={f.minBytes ?? ""} onchange={(e) => change({ minBytes: numberOrNull(e.currentTarget.value) })}>
            <option value="">{t("search.editor.any")}</option>
            {#each withValue(SIZES, f.minBytes) as b (b)}<option value={b}>{formatBytes(b)}</option>{/each}
          </select>
        </label>
        <label class="field">
          <span class="field-label">{t("search.editor.unused")}</span>
          <select value={f.unusedDays ?? ""} onchange={(e) => change({ unusedDays: numberOrNull(e.currentTarget.value) })}>
            <option value="">{t("search.editor.any")}</option>
            {#each withValue(UNUSED, f.unusedDays) as d (d)}<option value={d}>{daysText(d)}</option>{/each}
          </select>
        </label>
        <label class="field">
          <span class="field-label">{t("search.editor.recent")}</span>
          <select value={f.recentDays ?? ""} onchange={(e) => change({ recentDays: numberOrNull(e.currentTarget.value) })}>
            <option value="">{t("search.editor.any")}</option>
            {#each withValue(RECENT, f.recentDays) as d (d)}<option value={d}>{daysText(d)}</option>{/each}
          </select>
        </label>
        <label class="field">
          <span class="field-label">{t("search.editor.sort")}</span>
          <select value={f.sort} onchange={(e) => change({ sort: e.currentTarget.value as SearchSort })}>
            <option value="size">{t("search.editor.sortSize")}</option>
            <option value="oldest">{t("search.editor.sortOldest")}</option>
            <option value="newest">{t("search.editor.sortNewest")}</option>
          </select>
        </label>
        <label class="field wide">
          <span class="field-label">{t("search.editor.words")}</span>
          <input class="words" value={f.words.join(", ")} placeholder={t("search.editor.wordsHint")}
                 onchange={(e) => change({ words: e.currentTarget.value.split(",").map((w) => w.trim()).filter(Boolean) })} />
        </label>
      </div>
    {/if}

    {#if phase === "searching" && !result}
      <div class="card status" role="status"><span class="spinner" aria-hidden="true"></span>{t("search.searching")}</div>
    {:else if phase === "error"}
      <div class="card status error-card" role="alert"><StatusIcon kind="warn" /><span>{error}</span></div>
    {:else if result}
      <div class="summary">
        <b>{t("search.found", { files: fileCount(result.totalCount), size: formatBytes(result.totalBytes) })}</b>
        {#if phase === "searching"}<span class="spinner small-spin" aria-label={t("search.searching")}></span>{/if}
        {#if result.totalCount > files.length}<span class="muted small">{t("search.capped", { shown: count(files.length), total: count(result.totalCount) })}</span>{/if}
      </div>
      {#if result.tooMany}<p class="muted small warn-line">{t("search.tooMany")}</p>{/if}
      {#if result.partial}<p class="muted small warn-line">{t("search.partial")}</p>{/if}
      {#if files.length === 0}
        <div class="card status"><Icon name="search" />{t("search.none")}</div>
      {:else}
        <div class="card rows">
          {#each files as file (file.path)}
            <div class="row" class:picked={selected.has(file.path)}>
              <input type="checkbox" checked={selected.has(file.path)} onchange={() => toggle(file.path)} aria-label={t("files.review.pick", { name: nameOf(file.path) })} />
              <FileThumb path={file.path} size={40} />
              <div class="text">
                <div class="name" title={file.path}>{nameOf(file.path)}</div>
                <div class="muted small">{displayPath(folderOf(file.path))} · {t("files.lastUsed", { date: formatDate(file.lastUsed) })}</div>
              </div>
              <span class="actions">
                {#if canOpen(file.path)}
                  <button class="icon-btn" title={t("files.review.open")} aria-label={`${t("files.review.open")}: ${nameOf(file.path)}`} onclick={() => openScannedFile(file.path).catch(() => {})}><Icon name="openExternal" /></button>
                {/if}
                <button class="icon-btn" title={t("files.review.showInFolder")} aria-label={`${t("files.review.showInFolder")}: ${nameOf(file.path)}`} onclick={() => showScannedFile(file.path).catch(() => {})}><Icon name="folderOpen" /></button>
              </span>
              <span class="size">{formatBytes(file.sizeBytes)}</span>
            </div>
          {/each}
        </div>

        <div class="card footer">
          <div class="footer-text">
            <b>{t("files.review.selected", { files: fileCount(selected.size), size: formatBytes(selectedBytes) })}</b>
            <span class="muted small">{t("files.review.restorable", { days: quarantine.keepDays })}</span>
          </div>
          <span class="grow"></span>
          <button class="btn btn-ghost" onclick={() => (selected = allSelected ? new Set() : new Set(files.map((f) => f.path)))}>
            {allSelected ? t("files.review.selectNone") : t("files.review.selectAll")}
          </button>
          <button class="btn btn-primary" disabled={!selected.size || moving} onclick={move}>
            {moving ? t("files.moving") : `${t("common.moveToQuarantine")} (${formatBytes(selectedBytes)})`}
          </button>
        </div>
      {/if}
    {/if}
  {/if}
</div>

{#if notice}
  {@const n = notice}
  <div class="card toast" class:error={n.kind === "error"} role="status" aria-live="polite">
    <StatusIcon kind={n.kind === "error" ? "warn" : "ok"} />
    <div class="toast-text">
      {#each n.kind === "moved" ? removedLines(n.report, quarantine.keepDays) : n.lines as line, i (i)}<p>{line}</p>{/each}
    </div>
    {#if n.kind === "moved" && n.report.entryIds.length}
      <button class="btn btn-ghost" disabled={undoing} onclick={() => undo(n.report)}>
        {#if !undoing}<Icon name="undo" />{/if}{undoing ? t("files.undoing") : t("files.undo")}
      </button>
    {/if}
    <button class="close" aria-label={t("common.close")} onclick={() => (notice = null)}><Icon name="dismiss" /></button>
  </div>
{/if}

<style>
  .search-view { padding-bottom: 8px; }
  .back { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 2px 0; margin-bottom: 4px; display: inline-flex; align-items: center; gap: 4px; }
  .back:hover { color: var(--accent); }
  h1 { margin-bottom: 12px; }
  .status { padding: 16px 18px; display: flex; align-items: center; gap: 10px; font-size: 13.5px; }
  .error-card { border-left: 3px solid var(--danger); }
  .spinner { width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--tip-border); border-top-color: var(--ai); animation: spin 0.9s linear infinite; flex: none; display: inline-block; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .chips { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; margin-bottom: 10px; }
  .chips-label { font-size: 13px; margin-right: 2px; }
  .chip { display: inline-flex; align-items: center; gap: 6px; font-size: 12.5px; background: var(--tip-bg); color: var(--ai-fg); border: 1px solid var(--tip-border); border-radius: 999px; padding: 3px 4px 3px 10px; }
  .chip.fixed { padding-right: 10px; }
  .chip-x { border: none; background: none; color: inherit; display: inline-flex; padding: 3px; border-radius: 50%; }
  .chip-x:hover { background: var(--hover); }
  .link { border: none; background: none; color: var(--accent); padding: 0 4px; font-size: 13px; }

  .editor { display: grid; grid-template-columns: repeat(auto-fill, minmax(190px, 1fr)); gap: 12px 16px; padding: 14px 16px; margin-bottom: 12px; }
  .field { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .field.wide { grid-column: 1 / -1; }
  .field-label { font-size: 11.5px; font-weight: 700; letter-spacing: 0.4px; text-transform: uppercase; color: var(--text-muted); }
  .toggles { display: flex; flex-wrap: wrap; gap: 6px; }
  .toggle { border: 1px solid var(--border-strong); background: var(--surface); border-radius: 999px; padding: 4px 12px; font-size: 12.5px; }
  .toggle.on { background: var(--accent-soft); border-color: var(--accent); color: var(--accent); font-weight: 600; }
  select, .words { font-family: inherit; font-size: 13px; padding: 6px 8px; border-radius: var(--radius-sm); border: 1px solid var(--border-strong); background: var(--surface); }
  .words { user-select: text; }


  .summary { display: flex; align-items: center; gap: 10px; margin: 2px 0 8px; font-size: 14px; }
  .small-spin { width: 12px; height: 12px; }
  .small { font-size: 12px; }
  .warn-line { margin: -2px 0 8px; }

  .rows { overflow: hidden; }
  .row { display: flex; align-items: center; gap: 12px; padding: 8px 12px 8px 14px; border-bottom: 1px solid var(--border); }
  .row:last-child { border-bottom: none; }
  .row.picked { background: var(--accent-soft); }
  input[type="checkbox"] { width: 18px; height: 18px; accent-color: var(--accent); flex: none; }
  .text { flex: 1; min-width: 0; }
  .name { font-weight: 600; font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .text .small { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .actions { display: flex; gap: 2px; opacity: 0; transition: opacity 0.12s; }
  .row:hover .actions, .actions:focus-within { opacity: 1; }
  .icon-btn { border: none; background: none; display: inline-flex; padding: 6px; border-radius: var(--radius-sm); color: var(--text-muted); }
  .icon-btn:hover { background: var(--hover); color: var(--text); }
  .size { font-size: 13px; font-weight: 600; width: 72px; text-align: right; flex: none; }

  .footer { position: sticky; bottom: 0; display: flex; align-items: center; gap: 10px; padding: 12px 16px; margin-top: 12px; box-shadow: var(--shadow-up); z-index: 2; }
  .footer-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .grow { flex: 1; }
  .btn:disabled { opacity: 0.5; cursor: default; }

  .toast { position: fixed; right: 30px; bottom: 96px; max-width: 560px; display: flex; align-items: center; gap: 12px; padding: 12px 14px 12px 16px; box-shadow: var(--shadow-pop); border-left: 4px solid var(--safe-fg); z-index: 10; }
  .toast.error { border-left-color: var(--danger); }
  .toast-text { flex: 1; font-size: 13px; line-height: 1.45; }
  .close { border: none; background: none; display: flex; padding: 4px; color: var(--text-muted); border-radius: var(--radius-sm); }
  .close:hover { background: var(--hover); }
</style>
