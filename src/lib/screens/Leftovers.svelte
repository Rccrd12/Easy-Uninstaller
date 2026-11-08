<script lang="ts">
  import StatusIcon from "../components/StatusIcon.svelte";
  import LeftoverRow from "../components/LeftoverRow.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import { listLeftovers, quarantineLeftovers, restoreQuarantine, usesSampleData } from "../api";
  import { formatBytes, leftoverCount, restoreSummary } from "../format";
  import { t } from "../i18n/index.svelte";
  import { quarantine } from "../settings.svelte";
  import type { LeftoverItem, MoveReport } from "../types";

  type Notice = { kind: "moved"; report: MoveReport } | { kind: "info" | "error"; lines: string[] };

  let items = $state<LeftoverItem[]>([]);
  let picked = $state<Set<string>>(new Set());
  let loading = $state(true);
  let error = $state<string | null>(null);
  let moving = $state(false);
  let undoing = $state(false);
  let notice = $state<Notice | null>(null);

  function load() {
    loading = true;
    error = null;
    listLeftovers()
      .then((l) => {
        items = l;
        // Safe ones are ticked; the user's own app settings are left for them to decide.
        picked = new Set(l.filter((i) => i.safety === "safe").map((i) => i.id));
      })
      .catch((e) => (error = String(e)))
      .finally(() => (loading = false));
  }

  $effect(() => load());

  /** Broken shortcuts and broken app-list entries get a group each; the rest are grouped by app. */
  const groups = $derived.by(() => {
    const out = new Map<string, { title: string; items: LeftoverItem[] }>();
    for (const item of items) {
      const special = item.reason === "brokenShortcut" || item.reason === "brokenEntry";
      const key = special ? `#${item.reason}` : (item.appName ?? "");
      const title = special
        ? t(`leftovers.groups.${item.reason}`)
        : item.appName
          ? t("leftovers.from", { app: item.appName })
          : t("leftovers.title");
      if (!out.has(key)) out.set(key, { title, items: [] });
      out.get(key)!.items.push(item);
    }
    return [...out.values()].map((g) => ({ ...g, size: g.items.reduce((s, i) => s + i.sizeBytes, 0) }));
  });

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
        picked = new Set(l.filter((i) => before.has(i.id) || i.safety === "safe").map((i) => i.id));
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
    <button class="btn btn-ghost" onclick={load}>↻ {t("leftovers.rescan")}</button>
  {/if}
</div>
{#if usesSampleData()}<SampleBanner />{/if}

{#if loading}
  <div class="card message" role="status">
    <span class="spinner"></span>
    <div>
      <p>{t("leftovers.searching")}</p>
      <p class="muted small">🛡️ {t("leftovers.scanNote")}</p>
    </div>
  </div>
{:else if error}
  <div class="card message">
    <p>{t("leftovers.failed", { error })}</p>
    <button class="btn btn-primary" onclick={load}>{t("leftovers.retry")}</button>
  </div>
{:else if items.length === 0}
  <div class="card message">
    <span class="big-icon" aria-hidden="true">✨</span>
    <div>
      <p>{t("leftovers.empty")}</p>
      <p class="muted small">{t("leftovers.historyNote")}</p>
    </div>
  </div>
{:else}
  {#each groups as group (group.title)}
    {@const all = group.items.every((i) => picked.has(i.id))}
    <div class="card group">
      <div class="group-head">
        <input type="checkbox" checked={all} indeterminate={!all && group.items.some((i) => picked.has(i.id))} onchange={() => toggleGroup(group.items)} aria-label={group.title} />
        <b>{group.title}</b>
        <span class="muted">· {leftoverCount(group.items.length)}</span>
        <span class="grow"></span>
        <span class="group-size" class:muted={!group.size}>{group.size ? formatBytes(group.size) : "—"}</span>
      </div>
      {#each group.items as item (item.id)}
        <LeftoverRow {item} checked={picked.has(item.id)} onToggle={() => toggle(item.id)} />
      {/each}
    </div>
  {/each}
  <div class="footer">
    <b>{t("leftovers.selected", { size: formatBytes(pickedBytes) })}</b>
    {#if pickedAdmin}<span class="muted small">· 🛡️ {t("uninstall.adminLeftovers", { count: leftoverCount(pickedAdmin) })}</span>{/if}
    <span class="grow"></span>
    <button class="btn btn-primary" disabled={!pickedItems.length || moving} onclick={move}>
      {moving ? t("leftovers.moving") : t("common.moveToQuarantine")}
    </button>
  </div>
{/if}

{#if notice}
  {@const n = notice}
  <div class="card toast" class:error={n.kind === "error"} role="status" aria-live="polite">
    <StatusIcon kind={n.kind === "error" ? "warn" : "ok"} />
    <div class="toast-text">
      {#each n.kind === "moved" ? movedLines(n.report) : n.lines as line, i (i)}<p>{line}</p>{/each}
    </div>
    {#if n.kind === "moved"}
      <button class="btn btn-ghost" disabled={undoing} onclick={() => undo(n.report)}>
        {undoing ? t("leftovers.undoing") : `↩ ${t("leftovers.undo")}`}
      </button>
    {/if}
    <button class="close" aria-label={t("common.close")} onclick={() => (notice = null)}>✕</button>
  </div>
{/if}

<style>
  .head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .head .btn { flex: none; margin-top: 4px; }
  .sub { margin: 3px 0 10px; }
  .group { padding: 6px 14px 4px; margin-bottom: 10px; }
  .group-head { display: flex; align-items: center; gap: 8px; padding: 8px 4px; border-bottom: 1px solid var(--border); font-size: 14px; }
  .group-head input { width: 18px; height: 18px; accent-color: var(--accent); margin-right: 3px; }
  .group-size { font-weight: 600; font-size: 13.5px; }
  .small { font-size: 12px; }
  .footer { position: sticky; bottom: 0; display: flex; align-items: center; gap: 10px; margin-top: 12px; padding: 12px 0; background: var(--bg); }
  .grow { flex: 1; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .message { padding: 22px 24px; margin-top: 8px; display: flex; align-items: center; gap: 14px; }
  .message > :not(.big-icon):not(.spinner) { flex: 1; }
  .big-icon { font-size: 26px; }
  .spinner { flex: none; width: 18px; height: 18px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .toast { position: fixed; right: 30px; bottom: 22px; max-width: 560px; display: flex; align-items: center; gap: 12px; padding: 12px 14px 12px 16px; box-shadow: var(--shadow-pop); border-left: 4px solid var(--safe-fg); z-index: 10; }
  .toast.error { border-left-color: var(--danger); }
  .toast-text { flex: 1; font-size: 13px; line-height: 1.4; }
  .toast-text p + p { color: var(--text-muted); margin-top: 2px; }
  .toast .btn { flex: none; }
  .close { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 4px; }
</style>
