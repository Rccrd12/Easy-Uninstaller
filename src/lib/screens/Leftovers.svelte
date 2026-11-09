<script lang="ts">
  import EyeOffIcon from "../components/EyeOffIcon.svelte";
  import Icon from "../components/Icon.svelte";
  import StatusIcon from "../components/StatusIcon.svelte";
  import LeftoverRow from "../components/LeftoverRow.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import {
    addExclusions,
    hiddenItem,
    listExclusions,
    listLeftovers,
    quarantineLeftovers,
    removeExclusions,
    restoreQuarantine,
    usesSampleData,
  } from "../api";
  import { formatBytes, leftoverCount, longAgo, restoreSummary } from "../format";
  import { t } from "../i18n/index.svelte";
  import { quarantine } from "../settings.svelte";
  import type { Screen } from "../screens";
  import type { Exclusion, LeftoverItem, MoveReport } from "../types";

  let { onNavigate }: { onNavigate: (s: Screen) => void } = $props();

  type Notice =
    | { kind: "moved"; report: MoveReport }
    | { kind: "hidden"; items: Exclusion[]; lines: string[] }
    | { kind: "info" | "error"; lines: string[] };

  let items = $state<LeftoverItem[]>([]);
  let picked = $state<Set<string>>(new Set());
  let loading = $state(true);
  let error = $state<string | null>(null);
  let moving = $state(false);
  let undoing = $state(false);
  let notice = $state<Notice | null>(null);
  /** How many leftovers are hidden. */
  let hiddenCount = $state(0);
  /** The ⋯ menu of a group. */
  let menu = $state<{ group: Group; x: number; y: number } | null>(null);

  function load() {
    loading = true;
    error = null;
    listLeftovers()
      .then((l) => {
        items = l;
        // Safe ones are ticked; the user's own app settings are left for them to decide.
        picked = new Set(l.filter(preselected).map((i) => i.id));
      })
      .catch((e) => (error = String(e)))
      .finally(() => (loading = false));
  }

  $effect(() => load());
  $effect(() => countHidden());

  function countHidden() {
    listExclusions()
      .then((list) => (hiddenCount = list.filter((e) => e.kind === "leftover").length))
      .catch(() => {});
  }

  /** "Hide": the leftovers stay where they are and are not suggested again. */
  async function hide(list: LeftoverItem[]) {
    if (!list.length) return;
    const hidden = list.map((i) => hiddenItem("leftover", i.path, i.appName));
    try {
      await addExclusions(hidden);
      const line = list.length === 1
        ? t("hidden.done", { name: t(`leftovers.reasons.${list[0].reason}`) })
        : t("hidden.doneMany", { count: leftoverCount(list.length) });
      notice = { kind: "hidden", items: hidden, lines: [line] };
    } catch (e) {
      notice = { kind: "error", lines: [t("hidden.failed", { error: String(e) })] };
    } finally {
      countHidden();
      reloadQuietly();
    }
  }

  async function unhide(list: Exclusion[]) {
    undoing = true;
    try {
      await removeExclusions(list);
      notice = { kind: "info", lines: [t("hidden.shownAgain")] };
    } catch (e) {
      notice = { kind: "error", lines: [t("hidden.failed", { error: String(e) })] };
    } finally {
      undoing = false;
      countHidden();
      reloadQuietly();
    }
  }

  function openMenu(e: MouseEvent, group: Group) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const width = 300;
    menu = { group, x: Math.max(8, Math.min(r.right - width, window.innerWidth - width - 8)), y: r.bottom + 4 };
  }

  /** The app a group belongs to, when it is one app's leftovers. */
  const appOf = (g: Group) => (g.key.startsWith("#") ? null : (g.items[0]?.appName ?? null));

  type Group = { key: string; title: string; items: LeftoverItem[]; size: number; modifiedAt: number | null };

  /**
   * Broken shortcuts and broken app-list entries get a group each; the rest are grouped
   * by app. Folders no installed app owns (`unowned`) are grouped by name, apart.
   */
  function grouped(list: LeftoverItem[]): Group[] {
    const out = new Map<string, Group>();
    for (const item of list) {
      const special = !item.unowned && (item.reason === "brokenShortcut" || item.reason === "brokenEntry");
      const key = special ? `#${item.reason}` : (item.appName ?? "");
      const title = special
        ? t(`leftovers.groups.${item.reason}`)
        : item.unowned
          ? (item.appName ?? t("leftovers.title"))
          : item.appName
            ? t("leftovers.from", { app: item.appName })
            : t("leftovers.title");
      if (!out.has(key)) out.set(key, { key, title, items: [], size: 0, modifiedAt: null });
      const g = out.get(key)!;
      g.items.push(item);
      g.size += item.sizeBytes;
      if (item.modifiedAt !== null) g.modifiedAt = Math.max(g.modifiedAt ?? 0, item.modifiedAt);
    }
    return [...out.values()];
  }

  const groups = $derived(grouped(items.filter((i) => !i.unowned)));
  const maybeGroups = $derived(grouped(items.filter((i) => i.unowned)));

  /** Ticked at first: only what surely belonged to an uninstalled app and is safe. */
  const preselected = (i: LeftoverItem) => i.safety === "safe" && !i.unowned;

  const pickedItems = $derived(items.filter((i) => picked.has(i.id)));
  const pickedBytes = $derived(pickedItems.reduce((s, i) => s + i.sizeBytes, 0));
  const pickedAdmin = $derived(pickedItems.filter((i) => i.needsAdmin).length);

  function toggle(id: string) {
    const next = new Set(picked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    picked = next;
  }

  function toggleGroup(list: LeftoverItem[]) {
    const all = list.every((i) => picked.has(i.id));
    const next = new Set(picked);
    for (const i of list) {
      if (all) next.delete(i.id);
      else next.add(i.id);
    }
    picked = next;
  }

  async function move() {
    if (!pickedItems.length || moving) return;
    moving = true;
    notice = null;
    try {
      const report = await quarantineLeftovers([...picked], items);
      if (report.movedCount === 0) {
        const lines = [report.declinedCount ? t("leftovers.declined", { count: leftoverCount(report.declinedCount) }) : t("leftovers.nothingMoved")];
        notice = { kind: "info", lines };
      } else {
        notice = { kind: "moved", report };
      }
    } catch (e) {
      notice = { kind: "error", lines: [t("leftovers.moveFailed", { error: String(e) })] };
    } finally {
      moving = false;
      reloadQuietly();
    }
  }

  /** Searches again without the spinner, keeping the user's choices where they still apply. */
  function reloadQuietly() {
    const before = picked;
    listLeftovers().then(
      (l) => {
        items = l;
        picked = new Set(l.filter((i) => before.has(i.id) || preselected(i)).map((i) => i.id));
      },
      () => {},
    );
  }

  async function undo(report: MoveReport) {
    undoing = true;
    try {
      const restored = await restoreQuarantine(report.entryIds);
      const clean = restored.renamedCount === 0 && restored.failedCount === 0;
      notice = { kind: "info", lines: clean ? [t("leftovers.undone")] : restoreSummary(restored) };
    } catch (e) {
      notice = { kind: "error", lines: [String(e) === "declined" ? t("quarantine.declined") : t("quarantine.failed", { error: String(e) })] };
    } finally {
      undoing = false;
      reloadQuietly();
    }
  }

  function movedLines(r: MoveReport): string[] {
    const lines = [t("leftovers.moved", { count: leftoverCount(r.movedCount), days: quarantine.keepDays })];
    if (r.declinedCount) lines.push(t("leftovers.declined", { count: leftoverCount(r.declinedCount) }));
    if (r.skippedCount) lines.push(t("uninstall.skippedNote", { count: leftoverCount(r.skippedCount) }));
    return lines;
  }
</script>

<div class="head">
  <div>
    <h1>{t("leftovers.title")}</h1>
    <p class="muted sub">{t("leftovers.subtitle")}</p>
  </div>
  {#if !loading}
    <button class="btn btn-ghost" onclick={load}><Icon name="refresh" />{t("leftovers.rescan")}</button>
  {/if}
</div>
{#if usesSampleData()}<SampleBanner />{/if}

{#if loading}
  <div class="card message" role="status">
    <span class="spinner"></span>
    <div>
      <p>{t("leftovers.searching")}</p>
    </div>
  </div>
{:else if error}
  <div class="card message">
    <p>{t("leftovers.failed", { error })}</p>
    <button class="btn btn-primary" onclick={load}>{t("leftovers.retry")}</button>
  </div>
{:else if items.length === 0}
  <div class="card message">
    <span class="big-icon"><Icon name="checkCircle" size={26} /></span>
    <div>
      <p>{t("leftovers.empty")}</p>
      <p class="muted small">{t("leftovers.historyNote")}</p>
    </div>
  </div>
{:else}
  {#snippet groupCard(group: Group)}
    {@const all = group.items.every((i) => picked.has(i.id))}
    <div class="card group">
      <div class="group-head">
        <input type="checkbox" checked={all} indeterminate={!all && group.items.some((i) => picked.has(i.id))} onchange={() => toggleGroup(group.items)} aria-label={group.title} />
        <b>{group.title}</b>
        <span class="muted">· {leftoverCount(group.items.length)}</span>
        {#if group.modifiedAt}<span class="muted">· {t("leftovers.lastChange", { when: longAgo(group.modifiedAt) })}</span>{/if}
        <span class="grow"></span>
        <span class="group-size" class:muted={!group.size}>{group.size ? formatBytes(group.size) : "—"}</span>
        <button class="dots" aria-label={t("files.groupMenu", { name: group.title })} aria-haspopup="menu" onclick={(e) => openMenu(e, group)}>⋯</button>
      </div>
      {#each group.items as item (item.id)}
        <LeftoverRow {item} checked={picked.has(item.id)} onToggle={() => toggle(item.id)} onHide={() => hide([item])} />
      {/each}
    </div>
  {/snippet}
  {#each groups as group (group.key)}
    {@render groupCard(group)}
  {/each}
  {#if maybeGroups.length}
    <div class="maybe">
      <h2>{t("leftovers.maybeTitle")} <span class="tag tag-check">{t("common.check")}</span></h2>
      <p class="muted">{t("leftovers.maybeText")}</p>
    </div>
    {#each maybeGroups as group (group.key)}
      {@render groupCard(group)}
    {/each}
  {/if}
  <div class="footer">
    <b>{t("leftovers.selected", { size: formatBytes(pickedBytes) })}</b>
    {#if pickedAdmin}<span class="muted small">· <Icon name="shield" size={13} /> {t("uninstall.adminLeftovers", { count: leftoverCount(pickedAdmin) })}</span>{/if}
    <span class="grow"></span>
    <button class="btn btn-primary" disabled={!pickedItems.length || moving} onclick={move}>
      {moving ? t("leftovers.moving") : t("common.moveToQuarantine")}
    </button>
  </div>
{/if}

{#if !loading && hiddenCount > 0}
  <button class="hidden-link" onclick={() => onNavigate("hidden")}>
    <EyeOffIcon />
    <span>{t("files.hiddenLink", { count: leftoverCount(hiddenCount) })}</span>
    <span class="link-action">{t("files.hiddenLinkAction")}<Icon name="chevronRight" size={12} /></span>
  </button>
{/if}

{#if menu}
  {@const m = menu}
  {@const app = appOf(m.group)}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="card menu" style="left:{m.x}px;top:{m.y}px" role="menu" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <button role="menuitem" class="menu-hide" onclick={() => { const list = m.group.items; menu = null; hide(list); }}>
      <EyeOffIcon />
      <span>
        {app ? t("leftovers.hideApp", { app }) : t("leftovers.hideGroup")}
        <span class="muted menu-hint">{t("leftovers.hideHint")}</span>
      </span>
    </button>
  </div>
{/if}

<svelte:window onclick={() => (menu = null)} onkeydown={(e) => e.key === "Escape" && (menu = null)} onresize={() => (menu = null)} />

{#if notice}
  {@const n = notice}
  <div class="card toast" class:error={n.kind === "error"} role="status" aria-live="polite">
    {#if n.kind === "hidden"}<span class="toast-icon"><EyeOffIcon size={20} /></span>{:else}<StatusIcon kind={n.kind === "error" ? "warn" : "ok"} />{/if}
    <div class="toast-text">
      {#each n.kind === "moved" ? movedLines(n.report) : n.lines as line, i (i)}<p>{line}</p>{/each}
    </div>
    {#if n.kind === "hidden"}
      <button class="btn btn-ghost" disabled={undoing} onclick={() => unhide(n.items)}><Icon name="undo" />{t("leftovers.undo")}</button>
    {/if}
    {#if n.kind === "moved"}
      <button class="btn btn-ghost" disabled={undoing} onclick={() => undo(n.report)}>
        {#if !undoing}<Icon name="undo" />{/if}{undoing ? t("leftovers.undoing") : t("leftovers.undo")}
      </button>
    {/if}
    <button class="close" aria-label={t("common.close")} onclick={() => (notice = null)}><Icon name="dismiss" /></button>
  </div>
{/if}

<style>
  .head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .head .btn { flex: none; margin-top: 4px; }
  .sub { margin: 3px 0 10px; }
  .group { padding: 6px 14px 4px; margin-bottom: 10px; }
  .group-head { display: flex; align-items: center; gap: 8px; padding: 8px 4px; border-bottom: 1px solid var(--border); font-size: 14px; }
  .group-head input { width: 18px; height: 18px; accent-color: var(--accent); margin-right: 3px; }
  .maybe { margin: 20px 0 10px; }
  .maybe h2 { font-size: 16px; font-weight: 600; display: flex; align-items: center; gap: 8px; margin-bottom: 3px; }
  .maybe p { font-size: 13px; line-height: 1.45; max-width: 760px; }
  .group-size { font-weight: 600; font-size: 13.5px; }
  .small { font-size: 12px; }
  .footer { position: sticky; bottom: 0; display: flex; align-items: center; gap: 10px; margin-top: 12px; padding: 12px 0; background: var(--bg); }
  .grow { flex: 1; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .message { padding: 22px 24px; margin-top: 8px; display: flex; align-items: center; gap: 14px; }
  .message > :not(.big-icon):not(.spinner) { flex: 1; }
  .big-icon { display: flex; color: var(--safe-fg); }
  .spinner { flex: none; width: 18px; height: 18px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .dots { flex: none; border: none; background: none; font-size: 18px; line-height: 1; padding: 2px 8px; margin-right: -6px; border-radius: var(--radius-sm); color: var(--text-muted); }
  .dots:hover { background: var(--hover); color: var(--text); }
  .hidden-link { display: flex; align-items: center; gap: 8px; margin-top: 6px; border: none; background: none; padding: 6px 2px; font-size: 13px; color: var(--text-muted); text-align: left; }
  .link-action { color: var(--accent); font-weight: 600; display: inline-flex; align-items: center; gap: 2px; }
  .hidden-link:hover .link-action { text-decoration: underline; }
  .menu { position: fixed; z-index: 20; width: 300px; padding: 6px; box-shadow: var(--shadow-pop); display: flex; flex-direction: column; }
  .menu button { border: none; background: none; text-align: left; padding: 8px 10px; border-radius: 5px; font-size: 13px; }
  .menu button:hover, .menu button:focus-visible { background: var(--hover); outline: none; }
  .menu-hide { display: flex; align-items: flex-start; gap: 8px; }
  .menu-hide :global(.icon) { margin-top: 2px; }
  .menu-hint { display: block; font-size: 12px; margin-top: 2px; }
  .toast-icon { display: flex; color: var(--text-muted); }
  .toast { position: fixed; right: 30px; bottom: 22px; max-width: 560px; display: flex; align-items: center; gap: 12px; padding: 12px 14px 12px 16px; box-shadow: var(--shadow-pop); border-left: 4px solid var(--safe-fg); z-index: 10; }
  .toast.error { border-left-color: var(--danger); }
  .toast-text { flex: 1; font-size: 13px; line-height: 1.4; }
  .toast-text p + p { color: var(--text-muted); margin-top: 2px; }
  .toast .btn { flex: none; }
  .close { border: none; background: none; color: var(--text-muted); padding: 4px; display: flex; border-radius: var(--radius-sm); }
  .close:hover { background: var(--hover); }
</style>
